//! The GPU backend on wgpu: a frame prepared on the CPU into instances (rects covered analytically, everything else
//! by coverage rasterised into an atlas), then drawn with fixed-function blending, in one render pass per stretch of
//! the frame between layers. A layer draws into a texture of its own, its filters run as fragment passes over it, and
//! it is composited into its parent by an instance.
//!
//! It uses only what WebGL 2 also has (vertex and instance buffers, uniform buffers, textures, fixed-function blending;
//! no compute, no storage buffers), and its device is made with WebGL 2's limits, so wgpu's GL backend can serve
//! WebGL 2 later without a second design.

mod arena;
mod atlas;
mod prepare;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use prepare::{Blend, INSTANCE, Op, PAGE, Prepare, Source, Uniform};

use crate::Resources;
use crate::cpu::{Frame, Stats};
use crate::fx::FastMap;
use crate::image::ImageId;
use crate::paint::Color;

/// Twip's adapter, device and queue, chosen as Twip prefers: native backends in the order D3D12 (when DXC is there to
/// compile its shaders), Vulkan, Metal (the browser's WebGPU on the web), hardware only (Twip draws on the CPU instead
/// of on a software adapter), and WebGL 2's limits at the adapter's texture sizes.
#[derive(Debug)]
pub struct Context {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    lost: Arc<AtomicBool>,
}

/// Why there is no GPU context: no hardware adapter on any backend, or the device was refused.
#[derive(Debug)]
pub enum ContextError {
    NoAdapter,
    Device(wgpu::RequestDeviceError),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextError::NoAdapter => f.write_str("no hardware GPU adapter"),
            ContextError::Device(e) => write!(f, "the GPU device was refused: {e}"),
        }
    }
}

impl std::error::Error for ContextError {}

impl Context {
    /// The first hardware adapter of the backends in Twip's order that can draw to `surface` (if given), and its
    /// device.
    pub async fn new(surface: Option<&wgpu::Surface<'_>>) -> Result<Context, ContextError> {
        let order: &[wgpu::Backends] = if cfg!(target_arch = "wasm32") {
            &[wgpu::Backends::BROWSER_WEBGPU]
        } else {
            &[wgpu::Backends::DX12, wgpu::Backends::VULKAN, wgpu::Backends::METAL]
        };
        for &backends in order {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = backends;
            // D3D12 compiles shaders with DXC only (`dxcompiler.dll`, found as Windows finds DLLs): FXC, wgpu's
            // fallback, miscompiles the instance shader (it drops colour channels of atlas-covered draws). Without
            // DXC, D3D12 has no adapter here and the next backend is tried.
            desc.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::default_dynamic_dxc();
            let instance = wgpu::Instance::new(desc);
            let options = wgpu::RequestAdapterOptions { compatible_surface: surface, ..wgpu::RequestAdapterOptions::default() };
            let Ok(adapter) = instance.request_adapter(&options).await else { continue };
            if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
                continue;
            }
            let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
            let desc = wgpu::DeviceDescriptor { label: Some("twip-graphics"), required_limits: limits, ..wgpu::DeviceDescriptor::default() };
            let (device, queue) = adapter.request_device(&desc).await.map_err(ContextError::Device)?;
            let lost = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&lost);
            device.set_device_lost_callback(move |_, _| flag.store(true, Ordering::Relaxed));
            // An error no scope catches loses the context: out of memory or an internal error (a driver reset) as it
            // comes, a validation error (a bug here) in release builds; debug builds stop on those to show them.
            let flag = Arc::clone(&lost);
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                if cfg!(debug_assertions) && matches!(e, wgpu::Error::Validation { .. }) {
                    panic!("twip-graphics: {e}");
                }
                flag.store(true, Ordering::Relaxed);
            }));
            return Ok(Context { instance, adapter, device, queue, lost });
        }
        Err(ContextError::NoAdapter)
    }

    /// Whether the device has been lost: draw with a new context (or on the CPU).
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// A configuration for drawing to `surface` at `width × height`: a non-sRGB format (blending is in encoded sRGB, as
    /// on the CPU), copies from it allowed (for the blend modes that read the destination) when it offers them, and
    /// Mailbox presentation when offered, else Fifo. `None` when the adapter can't draw to it.
    pub fn surface_config(&self, surface: &wgpu::Surface<'_>, width: u32, height: u32) -> Option<wgpu::SurfaceConfiguration> {
        let caps = surface.get_capabilities(&self.adapter);
        let format = *caps.formats.iter().find(|f| !f.is_srgb())?;
        let present_mode =
            if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) { wgpu::PresentMode::Mailbox } else { wgpu::PresentMode::Fifo };
        Some(wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | (caps.usages & wgpu::TextureUsages::COPY_SRC),
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: Vec::new(),
            color_space: wgpu::SurfaceColorSpace::default(),
        })
    }
}

/// Why a frame can't be prepared: the device was lost (see [`Context::is_lost`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Lost,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the GPU device was lost")
    }
}

impl std::error::Error for Error {}

/// A prepared frame: what [`Renderer::render`] draws. The passes stay in the renderer; this names the frame.
#[derive(Debug)]
pub struct Prepared {
    clear: Option<wgpu::Color>,
    serial: u64,
    stats: Stats,
}

impl Prepared {
    /// What the frame couldn't draw.
    pub fn stats(&self) -> Stats {
        self.stats
    }
}

/// An image on the GPU: its texture, all its mip levels uploaded, their bytes, and the frame that last drew it.
#[derive(Debug)]
struct Image {
    view: wgpu::TextureView,
    bytes: usize,
    used: u64,
}

/// The images' memory on the GPU past which the least recently drawn go (as the CPU backend's image cache).
const IMAGE_BUDGET: usize = 256 << 20;

/// A layer's two textures at one nesting depth (its pixels, and the other side of its filters' passes), and their size:
/// the largest layer drawn at that depth so far.
#[derive(Debug)]
struct Slot {
    size: (u32, u32),
    views: [wgpu::TextureView; 2],
    /// The filter passes' bind groups reading from each texture (made again with the uniform buffer).
    groups: Option<[wgpu::BindGroup; 2]>,
}

/// The bytes of a draw's uniforms (its target's size, its instances' origin, its ramp and filter bases) and of a filter
/// pass's (see the filter shader).
const DRAW_UNIFORMS: u64 = 32;
const FILTER_UNIFORMS: u64 = 96;

/// The GPU renderer for one device and target format. It keeps its pipelines, textures and buffers between frames.
pub struct Renderer {
    format: wgpu::TextureFormat,
    over: wgpu::RenderPipeline,
    multiply: [wgpu::RenderPipeline; 2],
    erase: wgpu::RenderPipeline,
    /// The other blend modes, reading a copy of the destination.
    general: wgpu::RenderPipeline,
    filter: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    image_layout: wgpu::BindGroupLayout,
    filter_layout: wgpu::BindGroupLayout,
    dst_layout: wgpu::BindGroupLayout,
    /// The copy of the destination a general blend reads, its size, and its bind group.
    dst: Option<(wgpu::Texture, (u32, u32), wgpu::BindGroup)>,
    /// The ramps' and colour filters' textures (and views), and how many rows each holds.
    ramps: Option<(wgpu::Texture, wgpu::TextureView, u32)>,
    filters: Option<(wgpu::Texture, wgpu::TextureView, u32)>,
    /// The frame's bind group, made again when what it binds is.
    frame_group: Option<wgpu::BindGroup>,
    /// Each texture a batch samples: its bind group.
    sources: FastMap<Source, wgpu::BindGroup>,
    /// Scratch for this frame's rows.
    scratch: Vec<u8>,
    /// The last frame prepared, the one [`Renderer::render`] draws.
    serial: u64,
    /// Each op's uniforms, `stride` bytes apart, and the buffer's size.
    uniforms: Option<(wgpu::Buffer, u64)>,
    stride: u32,
    /// This frame's uniforms, before they are written.
    bytes: Vec<u8>,
    slots: Vec<Slot>,
    atlas: Option<(wgpu::Texture, u32)>,
    /// The colour glyphs' atlas, and the kept coverage atlas.
    colour: Option<(wgpu::Texture, u32)>,
    kept: Option<(wgpu::Texture, u32)>,
    instances: Option<(wgpu::Buffer, u64)>,
    /// Kept pictures' instances, and how many the buffer holds.
    kept_instances: Option<(wgpu::Buffer, u32)>,
    images: FastMap<ImageId, Image>,
    image_bytes: usize,
    image_budget: usize,
    /// A 1 × 1 transparent image, bound where a batch samples none.
    empty: wgpu::TextureView,
    /// Linear or nearest, clamped or repeating.
    samplers: [wgpu::Sampler; 4],
    prepare: Prepare,
    lost: Arc<AtomicBool>,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer").field("atlas_pages", &self.prepare.atlas.pages()).field("images", &self.images.len()).finish()
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType, view_dimension: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type, view_dimension, multisampled: false },
        count: None,
    }
}

/// A 2D texture of `format`, written with `data` (rows packed).
fn texture(context: &Context, label: &str, format: wgpu::TextureFormat, (w, h): (u32, u32), texel: u32, data: &[u8]) -> wgpu::TextureView {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue.write_texture(
        texture.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * texel), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

impl Renderer {
    /// Tests: how many images are on the GPU.
    #[doc(hidden)]
    pub fn images_held(&self) -> usize {
        self.images.len()
    }

    /// Tests: the images' memory on the GPU past which the least recently drawn go.
    #[doc(hidden)]
    pub fn set_image_budget(&mut self, bytes: usize) {
        self.image_budget = bytes;
    }

    /// Tools: how many pictures have been prepared to be kept, and how many kept pictures drawn again, so far.
    #[doc(hidden)]
    pub fn kept_counts(&self) -> (u64, u64) {
        self.prepare.memo.counts
    }

    /// A renderer drawing into targets of `format` on `context`'s device.
    pub fn new(context: &Context, format: wgpu::TextureFormat) -> Renderer {
        let device = &context.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("twip-graphics instances"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        use wgpu::TextureSampleType as Sample;
        use wgpu::TextureViewDimension as Dim;
        let uniform = |size: u64| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("twip-graphics frame"),
            entries: &[
                uniform(DRAW_UNIFORMS),
                texture_entry(1, Sample::Float { filterable: false }, Dim::D2Array),
                texture_entry(2, Sample::Float { filterable: false }, Dim::D2),
                texture_entry(3, Sample::Float { filterable: false }, Dim::D2),
                texture_entry(4, Sample::Float { filterable: false }, Dim::D2Array),
                texture_entry(5, Sample::Float { filterable: false }, Dim::D2Array),
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("twip-graphics image"),
            entries: &[
                texture_entry(0, Sample::Float { filterable: true }, Dim::D2),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let dst_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("twip-graphics destination"),
            entries: &[texture_entry(0, Sample::Float { filterable: false }, Dim::D2)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("twip-graphics instances"),
            bind_group_layouts: &[Some(&frame_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let general_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("twip-graphics general blend"),
            bind_group_layouts: &[Some(&frame_layout), Some(&image_layout), Some(&dst_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Unorm8x4, 3 => Uint32, 4 => Uint32, 5 => Uint32,
            6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Uint32
        ];
        let instanced = |layout: &wgpu::PipelineLayout, entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("twip-graphics instances"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: INSTANCE as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..wgpu::PrimitiveState::default() },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = |blend: wgpu::BlendComponent| instanced(&pipeline_layout, "fragment", Some(wgpu::BlendState { color: blend, alpha: blend }));
        let component = |src_factor, dst_factor| wgpu::BlendComponent { src_factor, dst_factor, operation: wgpu::BlendOperation::Add };
        use wgpu::BlendFactor as F;
        let filter_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("twip-graphics filters"),
            source: wgpu::ShaderSource::Wgsl(include_str!("filter.wgsl").into()),
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("twip-graphics filter"),
            entries: &[uniform(FILTER_UNIFORMS), texture_entry(1, Sample::Float { filterable: false }, Dim::D2)],
        });
        let filter = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("twip-graphics filter"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("twip-graphics filter"),
                bind_group_layouts: &[Some(&filter_layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &filter_shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &filter_shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |linear: bool, repeat: bool| {
            let filter = if linear { wgpu::FilterMode::Linear } else { wgpu::FilterMode::Nearest };
            let address = if repeat { wgpu::AddressMode::Repeat } else { wgpu::AddressMode::ClampToEdge };
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("twip-graphics image"),
                address_mode_u: address,
                address_mode_v: address,
                mag_filter: filter,
                min_filter: filter,
                ..wgpu::SamplerDescriptor::default()
            })
        };
        let limits = device.limits();
        Renderer {
            format,
            over: pipeline(component(F::One, F::OneMinusSrcAlpha)),
            multiply: [pipeline(component(F::Dst, F::OneMinusSrcAlpha)), pipeline(component(F::OneMinusDstAlpha, F::One))],
            erase: pipeline(component(F::Zero, F::OneMinusSrcAlpha)),
            general: instanced(&general_layout, "blend", None),
            filter,
            frame_layout,
            image_layout,
            filter_layout,
            dst_layout,
            dst: None,
            ramps: None,
            filters: None,
            frame_group: None,
            sources: FastMap::default(),
            scratch: Vec::new(),
            serial: 0,
            uniforms: None,
            stride: (FILTER_UNIFORMS as u32).next_multiple_of(limits.min_uniform_buffer_offset_alignment),
            bytes: Vec::new(),
            slots: Vec::new(),
            atlas: None,
            colour: None,
            kept: None,
            instances: None,
            kept_instances: None,
            images: FastMap::default(),
            image_bytes: 0,
            image_budget: IMAGE_BUDGET,
            empty: texture(context, "twip-graphics empty image", wgpu::TextureFormat::Rgba8Unorm, (1, 1), 4, &[0; 4]),
            samplers: [sampler(false, false), sampler(true, false), sampler(false, true), sampler(true, true)],
            prepare: Prepare::new(limits.max_texture_dimension_2d),
            lost: Arc::clone(&context.lost),
        }
    }

    /// Prepares `frame` for a `width × height` target: its instances built, its coverage, ramps, colour filters and
    /// new images uploaded. What the GPU keeps (textures, buffers, bind groups) is made again only when it must grow.
    pub fn prepare(&mut self, context: &Context, res: &Resources, frame: &Frame<'_>, (width, height): (u32, u32)) -> Result<Prepared, Error> {
        if self.lost.load(Ordering::Relaxed) {
            return Err(Error::Lost);
        }
        let (device, queue) = (&context.device, &context.queue);
        self.prepare.frame(res, frame, (width, height));
        let p = &mut self.prepare;
        // The images this frame draws: an evicted one (a kept picture drawn again may sample it) is decoded again.
        let now = self.serial + 1;
        for i in 0..p.out.batches.len() {
            if let Source::Image(id, _) = p.out.batches[i].source {
                match self.images.get_mut(&id) {
                    Some(image) => image.used = now,
                    None => p.reload(res, id),
                }
            }
        }
        // Images decoded this frame: every mip level uploaded.
        for (id, mips) in p.pending.drain(..) {
            let base = &mips.levels[0];
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("twip-graphics image"),
                size: wgpu::Extent3d { width: base.width, height: base.height, depth_or_array_layers: 1 },
                mip_level_count: mips.levels.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, l) in mips.levels.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &l.data,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l.width * 4), rows_per_image: Some(l.height) },
                    wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 },
                );
            }
            let bytes = mips.bytes();
            self.images.insert(id, Image { view: texture.create_view(&wgpu::TextureViewDescriptor::default()), bytes, used: now });
            self.image_bytes += bytes;
            // A bind group made while it was missing bound the empty image.
            self.sources.retain(|s, _| !matches!(s, Source::Image(i, _) if *i == id));
        }
        // Past the budget, the least recently drawn images go (never one this frame draws), to be decoded again when
        // drawn again.
        if self.image_bytes > self.image_budget {
            let mut by_age: Vec<(u64, ImageId)> = self.images.iter().filter(|(_, i)| i.used < now).map(|(k, i)| (i.used, *k)).collect();
            by_age.sort_unstable_by_key(|(used, _)| *used);
            for (_, id) in by_age {
                if self.image_bytes <= self.image_budget {
                    break;
                }
                if let Some(image) = self.images.remove(&id) {
                    self.image_bytes -= image.bytes;
                    p.known.remove(&id);
                    self.sources.retain(|s, _| !matches!(s, Source::Image(i, _) if *i == id));
                }
            }
        }
        // The atlases: a texture made again unbinds the frame's bind group.
        let mut remade = false;
        for (texture, atlas, format, texel) in [
            (&mut self.atlas, &mut p.atlas, wgpu::TextureFormat::R8Unorm, 1),
            (&mut self.colour, &mut p.colour, wgpu::TextureFormat::Rgba8Unorm, 4),
            (&mut self.kept, &mut p.kept, wgpu::TextureFormat::R8Unorm, 1),
        ] {
            remade |= upload(context, texture, atlas, format, texel);
        }
        let bytes = (p.out.instances.len() as u64).max(INSTANCE as u64);
        if self.instances.as_ref().is_none_or(|(_, size)| *size < bytes) {
            let size = bytes.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("twip-graphics instances"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instances = Some((buffer, size));
        }
        let Some((buffer, _)) = &self.instances else { return Err(Error::Lost) };
        if !p.out.instances.is_empty() {
            queue.write_buffer(buffer, 0, &p.out.instances);
        }
        // Kept pictures' instances: those prepared this frame, or all of them into a buffer grown to the arena.
        let capacity = p.memo.arena.capacity().max(1);
        let grown = self.kept_instances.as_ref().is_none_or(|(_, c)| *c < capacity);
        if grown {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("twip-graphics kept instances"),
                size: u64::from(capacity) * INSTANCE as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.kept_instances = Some((buffer, capacity));
        }
        let Some((kept_buffer, _)) = &self.kept_instances else { return Err(Error::Lost) };
        for (start, bytes) in p.memo.uploads(!grown) {
            queue.write_buffer(kept_buffer, u64::from(start) * INSTANCE as u64, bytes);
        }
        // The uniforms: a draw's target size, its instances' origin and its ramp and filter bases; a filter step's.
        let stride = self.stride as usize;
        self.bytes.clear();
        self.bytes.resize(p.out.uniforms.len().max(1) * stride, 0);
        for (u, out) in p.out.uniforms.iter().zip(self.bytes.chunks_mut(stride)) {
            let mut at = 0;
            let mut put = |v: [u8; 4]| {
                out[at..at + 4].copy_from_slice(&v);
                at += 4;
            };
            match *u {
                Uniform::Draw { size, origin, ramps, filters } => {
                    for v in [size.0 as f32, size.1 as f32, origin.0 as f32, origin.1 as f32] {
                        put(v.to_le_bytes());
                    }
                    for v in [ramps, filters] {
                        put(v.to_le_bytes());
                    }
                }
                Uniform::Filter { size, step } => {
                    let head = [u32::from(step.horizontal), step.kind, step.left, step.right, size.0, size.1, 0, 0];
                    for v in head.iter().chain(&step.weights) {
                        put(v.to_le_bytes());
                    }
                }
            }
        }
        let bytes = self.bytes.len() as u64;
        if self.uniforms.as_ref().is_none_or(|(_, size)| *size < bytes) {
            let size = bytes.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("twip-graphics uniforms"),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.uniforms = Some((buffer, size));
            // The frame's and the filters' bind groups bind the old buffer.
            remade = true;
            for s in &mut self.slots {
                s.groups = None;
            }
        }
        let Some((uniforms, _)) = &self.uniforms else { return Err(Error::Lost) };
        queue.write_buffer(uniforms, 0, &self.bytes);
        // Each layer depth's textures, grown to the largest layer drawn at that depth.
        for (depth, &(w, h)) in p.out.slots.iter().enumerate() {
            let have = self.slots.get(depth).map(|s| s.size);
            if have.is_some_and(|(sw, sh)| sw >= w && sh >= h) {
                continue;
            }
            let size = have.map_or((w, h), |(sw, sh)| (sw.max(w), sh.max(h)));
            let make = || {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("twip-graphics layer"),
                        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            };
            let slot = Slot { size, views: [make(), make()], groups: None };
            match self.slots.get_mut(depth) {
                Some(s) => *s = slot,
                None => self.slots.push(slot),
            }
            // Composites read the old texture.
            self.sources.remove(&Source::Layer(depth));
        }
        for s in &mut self.slots {
            if s.groups.is_none() {
                let group = |view: &wgpu::TextureView| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("twip-graphics filter"),
                        layout: &self.filter_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: uniform_binding(uniforms, FILTER_UNIFORMS) },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                        ],
                    })
                };
                s.groups = Some([group(&s.views[0]), group(&s.views[1])]);
            }
        }
        // The destination copy, grown to the largest a general blend needs (at least a pixel, to bind).
        let need = (p.out.dst.0.max(1), p.out.dst.1.max(1));
        let have = self.dst.as_ref().map(|(_, size, _)| *size);
        if have.is_none_or(|(w, h)| w < need.0 || h < need.1) {
            let size = have.map_or(need, |(w, h)| (w.max(need.0), h.max(need.1)));
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("twip-graphics destination"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("twip-graphics destination"),
                layout: &self.dst_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.create_view(&wgpu::TextureViewDescriptor::default())),
                }],
            });
            self.dst = Some((texture, size, group));
        }
        // This frame's ramps (256 RGBA entries a row) and colour filters (a row each, 32-bit floats), into textures
        // grown when a frame needs more rows.
        self.scratch.clear();
        for r in &p.out.ramps {
            self.scratch.extend_from_slice(r.as_flattened());
        }
        remade |= rows(context, &mut self.ramps, "twip-graphics ramps", wgpu::TextureFormat::Rgba8Unorm, (256, 4), p.out.ramps.len(), &self.scratch);
        self.scratch.clear();
        for r in &p.out.filters {
            self.scratch.extend(r.iter().flat_map(|v| v.to_le_bytes()));
        }
        let filter_row = (prepare::FILTER_TEXELS as u32, 16);
        remade |= rows(
            context,
            &mut self.filters,
            "twip-graphics filters",
            wgpu::TextureFormat::Rgba32Float,
            filter_row,
            p.out.filters.len(),
            &self.scratch,
        );
        if remade || self.frame_group.is_none() {
            let (Some((atlas, _)), Some((colour, _)), Some((kept, _)), Some((_, ramps, _)), Some((_, filters, _))) =
                (&self.atlas, &self.colour, &self.kept, &self.ramps, &self.filters)
            else {
                return Err(Error::Lost);
            };
            let array = wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() };
            let (atlas, colour, kept) = (atlas.create_view(&array), colour.create_view(&array), kept.create_view(&array));
            let view = wgpu::BindingResource::TextureView;
            self.frame_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("twip-graphics frame"),
                layout: &self.frame_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform_binding(uniforms, DRAW_UNIFORMS) },
                    wgpu::BindGroupEntry { binding: 1, resource: view(&atlas) },
                    wgpu::BindGroupEntry { binding: 2, resource: view(ramps) },
                    wgpu::BindGroupEntry { binding: 3, resource: view(filters) },
                    wgpu::BindGroupEntry { binding: 4, resource: view(&colour) },
                    wgpu::BindGroupEntry { binding: 5, resource: view(&kept) },
                ],
            }));
        }
        // Each texture a batch samples: its bind group, made the first time.
        for b in &p.out.batches {
            if self.sources.contains_key(&b.source) {
                continue;
            }
            let (view, sampler) = match b.source {
                Source::Image(id, s) => match self.images.get(&id) {
                    Some(image) => (&image.view, &self.samplers[usize::from(s.linear) + 2 * usize::from(s.repeat)]),
                    None => (&self.empty, &self.samplers[0]),
                },
                Source::Layer(depth) => (&self.slots[depth].views[0], &self.samplers[0]),
                Source::None => (&self.empty, &self.samplers[0]),
            };
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("twip-graphics image"),
                layout: &self.image_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            });
            self.sources.insert(b.source, group);
        }
        self.serial += 1;
        Ok(Prepared { clear: frame.clear.map(premultiplied), serial: self.serial, stats: p.out.stats })
    }

    /// Draws `prepared` into `target` (the size it was prepared for): a render pass for each stretch of the frame
    /// between layers and destination copies, each layer's content and filter steps. Only the last frame prepared can
    /// be drawn; an earlier one draws nothing. A frame that blends by a mode other than source-over, multiply,
    /// destination-in, destination and clear copies from `target`'s texture, which then needs
    /// [`wgpu::TextureUsages::COPY_SRC`] (as [`Context::surface_config`] asks for).
    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, prepared: &Prepared) {
        let (Some((buffer, _)), Some((kept, _)), Some(frame), Some((dst, _, dst_group))) =
            (&self.instances, &self.kept_instances, &self.frame_group, &self.dst)
        else {
            return;
        };
        if prepared.serial != self.serial {
            return;
        }
        let out = &self.prepare.out;
        let begin = |encoder: &mut wgpu::CommandEncoder, view, load, (w, h): (u32, u32)| {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("twip-graphics"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    ..wgpu::RenderPassDescriptor::default()
                })
                .forget_lifetime();
            pass.set_viewport(0.0, 0.0, w as f32, h as f32, 0.0, 1.0);
            pass
        };
        let transparent = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        let stride = self.stride;
        for op in &out.ops {
            match op {
                Op::Draw { slot, region, clear, batches } => {
                    let load = match (slot, clear) {
                        (_, false) => wgpu::LoadOp::Load,
                        (None, true) => prepared.clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                        (Some(_), true) => transparent,
                    };
                    if batches.is_empty() && load == wgpu::LoadOp::Load {
                        continue;
                    }
                    let view = slot.map_or(target, |s| &self.slots[s].views[0]);
                    let mut pass = begin(encoder, view, load, (region.width() as u32, (region.y1 - region.y0) as u32));
                    // The uniform, the instance buffer and the image change only between batches that differ in them.
                    let mut bound: Option<(u32, bool, Source)> = None;
                    for b in &out.batches[batches.clone()] {
                        let Some(image) = self.sources.get(&b.source) else { continue };
                        if bound.is_none_or(|(u, _, _)| u != b.uniform) {
                            pass.set_bind_group(0, frame, &[b.uniform * stride]);
                        }
                        if bound.is_none_or(|(_, k, _)| k != b.kept) {
                            pass.set_vertex_buffer(0, if b.kept { kept.slice(..) } else { buffer.slice(..) });
                        }
                        if bound.is_none_or(|(_, _, s)| s != b.source) {
                            pass.set_bind_group(1, image, &[]);
                        }
                        bound = Some((b.uniform, b.kept, b.source));
                        let pipelines: &[wgpu::RenderPipeline] = match b.blend {
                            Blend::Over => std::slice::from_ref(&self.over),
                            Blend::Erase => std::slice::from_ref(&self.erase),
                            Blend::Multiply => &self.multiply,
                            Blend::General => {
                                pass.set_bind_group(2, dst_group, &[]);
                                std::slice::from_ref(&self.general)
                            }
                        };
                        for p in pipelines {
                            pass.set_pipeline(p);
                            pass.draw(0..4, b.instances.clone());
                        }
                    }
                }
                Op::Filter { slot, size, read, uniform, .. } => {
                    let s = &self.slots[*slot];
                    let Some(groups) = &s.groups else { continue };
                    let mut pass = begin(encoder, &s.views[1 - read], transparent, *size);
                    pass.set_pipeline(&self.filter);
                    pass.set_bind_group(0, &groups[*read], &[uniform * stride]);
                    pass.draw(0..3, 0..1);
                }
                Op::Copy { slot, from, size } => {
                    let texture = slot.map_or(target.texture(), |s| self.slots[s].views[0].texture());
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: from.0, y: from.1, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        dst.as_image_copy(),
                        wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    );
                }
                // Only in kept pictures' passes: a frame's are appended without them.
                Op::Child { .. } => {}
            }
        }
    }
}

/// A uniform binding of `size` bytes, at the offset each draw gives.
fn uniform_binding(buffer: &wgpu::Buffer, size: u64) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset: 0, size: wgpu::BufferSize::new(size) })
}

/// `n` rows of `data` (`(texels, bytes per texel)` a row) written to the top of a texture of `format`, made (or grown,
/// doubling) when it holds fewer. `true` when the texture was made again.
fn rows(
    context: &Context,
    texture: &mut Option<(wgpu::Texture, wgpu::TextureView, u32)>,
    label: &str,
    format: wgpu::TextureFormat,
    (width, texel): (u32, u32),
    n: usize,
    data: &[u8],
) -> bool {
    let n = n as u32;
    let remade = texture.as_ref().is_none_or(|(_, _, have)| *have < n.max(1));
    if remade {
        let have = n.max(1).next_power_of_two();
        let t = context.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height: have, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = t.create_view(&wgpu::TextureViewDescriptor::default());
        *texture = Some((t, view, have));
    }
    if let Some((t, _, _)) = texture
        && n > 0
    {
        context.queue.write_texture(
            t.as_image_copy(),
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * texel), rows_per_image: Some(n) },
            wgpu::Extent3d { width, height: n, depth_or_array_layers: 1 },
        );
    }
    remade
}

/// `atlas`'s rows written since the last upload uploaded to its texture array (`texture`, made or grown to as many
/// layers as the atlas has pages, at least one), whose texels are `format`, `texel` bytes each. A texture made again
/// gets every page's rows. `true` when it was made again.
fn upload(context: &Context, texture: &mut Option<(wgpu::Texture, u32)>, atlas: &mut atlas::Atlas, format: wgpu::TextureFormat, texel: u32) -> bool {
    let layers = atlas.pages().max(1) as u32;
    let have = texture.take().filter(|(_, n)| *n >= layers);
    let remade = have.is_none();
    let (texture, _) = texture.insert(have.unwrap_or_else(|| {
        // A texture made again gets every page's rows.
        atlas.dirty_all();
        let t = context.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("twip-graphics atlas"),
            size: wgpu::Extent3d { width: PAGE, height: PAGE, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        (t, layers)
    }));
    for (page, top, rows, texels) in atlas.take_dirty() {
        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: top, z: page }, aspect: wgpu::TextureAspect::All },
            texels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(PAGE * texel), rows_per_image: Some(rows) },
            wgpu::Extent3d { width: PAGE, height: rows, depth_or_array_layers: 1 },
        );
    }
    remade
}

/// An unpremultiplied colour as a clear value: premultiplied, in 0..1.
fn premultiplied(c: Color) -> wgpu::Color {
    let a = f64::from(c.a) / 255.0;
    let ch = |v: u8| f64::from(v) / 255.0 * a;
    wgpu::Color { r: ch(c.r), g: ch(c.g), b: ch(c.b), a }
}
