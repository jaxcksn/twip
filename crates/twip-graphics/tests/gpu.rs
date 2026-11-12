//! The GPU backend against the CPU backend: each scene drawn by both and compared. A pixel may differ by 2 in each
//! channel inside shapes; beyond that, at most 0.02% of the pixels (anti-aliased edges round differently on the GPU).
//! The tests pass without a hardware adapter (they draw nothing there): CI without a GPU checks the CPU backend only.
//! A failure writes `tests/out/<name>.gpu.png` and `tests/out/<name>.gpu-diff.png`.

#![cfg(feature = "gpu")]

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::task::{Context as TaskContext, Poll, Wake, Waker};

use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer as CpuRenderer, Target};
use twip_graphics::gpu::{Context, Renderer};
use twip_graphics::kurbo::Affine;
use twip_graphics::paint::Color;
use twip_graphics::scene::Picture;

mod common;

/// Runs a future to completion on this thread: wgpu's native futures are ready or become ready while polled.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut cx = TaskContext::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// This machine's GPU context, made once; `None` without a hardware adapter.
fn context() -> Option<&'static Context> {
    static CONTEXT: OnceLock<Option<Context>> = OnceLock::new();
    CONTEXT.get_or_init(|| block_on(Context::new(None)).ok()).as_ref()
}

/// `picture` drawn on the GPU into a `w × h` RGBA8 target over white, read back.
fn gpu(ctx: &Context, picture: &Picture, resources: &Resources, (w, h): (u32, u32)) -> Vec<u8> {
    let (device, queue) = (&ctx.device, &ctx.queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer = Renderer::new(ctx, format);
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    let prepared = renderer.prepare(ctx, resources, &Frame { clear: Some(Color::WHITE), draws: &draws }, (w, h)).expect("not lost");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    renderer.render(&mut encoder, &texture.create_view(&wgpu::TextureViewDescriptor::default()), &prepared);
    // Rows copied out are padded to 256 bytes.
    let row = w * 4;
    let padded = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(padded * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("the readback maps"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("the GPU finishes");
    let data = slice.get_mapped_range().expect("the readback is mapped");
    data.chunks(padded as usize).flat_map(|r| &r[..row as usize]).copied().collect()
}

fn cpu(picture: &Picture, resources: &Resources, (w, h): (u32, u32)) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("a buffer that fits");
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    CpuRenderer::new().render(&mut target, resources, &Frame { clear: Some(Color::WHITE), draws: &draws });
    px
}

/// Draws `picture` on both backends and checks they agree.
fn same(name: &str, size: (u32, u32), picture: &Picture, resources: &Resources) {
    let Some(ctx) = context() else { return };
    let (ours, theirs) = (gpu(ctx, picture, resources, size), cpu(picture, resources, size));
    let mut over = 0usize;
    let mut worst = 0u8;
    let diff: Vec<u8> = ours
        .chunks(4)
        .zip(theirs.chunks(4))
        .flat_map(|(a, b)| {
            let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            worst = worst.max(d);
            over += usize::from(d > 2);
            if d > 2 { [255, 0, 0, 255] } else { [b[0] / 2 + 127, b[1] / 2 + 127, b[2] / 2 + 127, 255] }
        })
        .collect();
    let allowed = (size.0 * size.1) as usize / 5000;
    eprintln!("{name}: {over} pixels over 2 (of {}), worst {worst}", size.0 * size.1);
    if over > allowed {
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("out");
        std::fs::create_dir_all(&out).expect("the output folder");
        let write = |file: &str, px: &[u8]| {
            let f = std::io::BufWriter::new(std::fs::File::create(out.join(file)).expect("a file"));
            let mut e = png::Encoder::new(f, size.0, size.1);
            e.set_color(png::ColorType::Rgba);
            e.write_header().expect("a header").write_image_data(px).expect("pixels");
        };
        write(&format!("{name}.gpu.png"), &ours);
        write(&format!("{name}.gpu-diff.png"), &diff);
        panic!("{name}: {over} pixels differ by more than 2 (at most {allowed} may), by at most {worst}; see tests/out/{name}.gpu*.png");
    }
}

#[test]
fn rects() {
    same("rects", (128, 96), &common::scenes::rects(), &Resources::default());
}

#[test]
fn rrects() {
    same("rrects", (128, 96), &common::scenes::rrects(), &Resources::default());
}

#[test]
fn paths() {
    same("paths", (128, 112), &common::scenes::paths(), &Resources::default());
}

#[test]
fn strokes() {
    same("strokes", (128, 128), &common::scenes::strokes(), &Resources::default());
}

#[test]
fn clips() {
    same("clips", (128, 120), &common::scenes::clips(), &Resources::default());
}

#[test]
fn transforms() {
    same("transforms", (128, 128), &common::scenes::transforms(), &Resources::default());
}

#[test]
fn blends() {
    same("blends", (128, 128), &common::scenes::blends(), &Resources::default());
}

#[test]
fn text() {
    let mut resources = Resources::default();
    let picture = common::scenes::text(&mut resources);
    same("text", (256, 200), &picture, &resources);
}

#[test]
fn images() {
    let mut resources = Resources::default();
    let picture = common::scenes::images(&mut resources);
    same("images", (128, 144), &picture, &resources);
}

#[test]
fn gradients() {
    same("gradients", (128, 144), &common::scenes::gradients(), &Resources::default());
}

#[test]
fn layers() {
    same("layers", (128, 144), &common::scenes::layers(), &Resources::default());
}

#[test]
fn blend_modes() {
    same("blend-modes", (254, 146), &common::scenes::blend_modes(), &Resources::default());
}

#[test]
fn colour_glyphs() {
    let mut resources = Resources::default();
    let picture = common::scenes::colour_glyphs(&mut resources);
    same("colour-glyphs", (256, 240), &picture, &resources);
}

#[test]
fn a_gpu_is_found_where_one_is_expected() {
    // Set TWIP_GRAPHICS_GPU=1 on a machine with a GPU so a missing adapter fails instead of skipping the comparisons.
    if std::env::var_os("TWIP_GRAPHICS_GPU").is_some() {
        assert!(context().is_some(), "no hardware GPU adapter");
    }
    if let Some(ctx) = context() {
        let info = ctx.adapter.get_info();
        eprintln!("GPU: {} on {:?}", info.name, info.backend);
    }
}
