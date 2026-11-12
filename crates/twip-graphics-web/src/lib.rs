//! twip-graphics' cross-backend comparisons in a browser, on its WebGPU: the scenes `twip-graphics/tests/gpu.rs`
//! draws, each drawn by the GPU backend and by the CPU backend and compared with the same tolerance (a pixel may
//! differ by 2 in each channel; beyond that, at most 0.02% of the pixels). Also a run of frames that moves kept
//! pictures. Built for `wasm32-unknown-unknown` (`build.mjs`) and run by `bindings/web/graphics/gpu.spec.mjs`.

#![cfg(target_arch = "wasm32")]

#[path = "../../twip-graphics/tests/common/scenes.rs"]
mod scenes;

use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;
use std::task::{Poll, Waker};

use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer as CpuRenderer, Target};
use twip_graphics::gpu::{Context, Renderer};
use twip_graphics::kurbo::Affine;
use twip_graphics::paint::Color;
use twip_graphics::scene::{Picture, Recorder};
use wasm_bindgen::prelude::wasm_bindgen;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Every comparison, as JSON: `{ "adapter", "backend", "scenes": [{ "name", "over", "allowed", "worst" }] }`, or
/// `{ "error" }` when there is no GPU.
#[wasm_bindgen]
pub async fn run() -> String {
    let ctx = match Context::new(None).await {
        Ok(ctx) => ctx,
        Err(e) => return format!("{{\"error\":{:?}}}", e.to_string()),
    };
    let info = ctx.adapter.get_info();
    let mut scenes = Vec::new();
    let mut res = Resources::default();
    let text = scenes::text(&mut res);
    let images = scenes::images(&mut res);
    let colour = scenes::colour_glyphs(&mut res);
    let list: [(&str, (u32, u32), Picture); 13] = [
        ("rects", (128, 96), scenes::rects()),
        ("rrects", (128, 96), scenes::rrects()),
        ("paths", (128, 112), scenes::paths()),
        ("strokes", (128, 128), scenes::strokes()),
        ("clips", (128, 120), scenes::clips()),
        ("transforms", (128, 128), scenes::transforms()),
        ("blends", (128, 128), scenes::blends()),
        ("text", (256, 200), text.clone()),
        ("images", (128, 144), images.clone()),
        ("gradients", (128, 144), scenes::gradients()),
        ("layers", (128, 144), scenes::layers()),
        ("blend-modes", (254, 146), scenes::blend_modes()),
        ("colour-glyphs", (256, 240), colour),
    ];
    for (name, size, picture) in &list {
        let mut renderer = Renderer::new(&ctx, FORMAT);
        let ours = gpu(&ctx, &mut renderer, picture, &res, *size).await;
        scenes.push(compare(name, *size, &ours, &cpu(picture, &res, *size)));
    }
    // Kept pictures, as Twip draws them: each frame a picture of its own placing them, moved by whole pixels (drawn
    // again) and by a fraction of one (prepared in the frame).
    let parts = [text, images, scenes::layers(), scenes::blend_modes()];
    let size = (520, 380);
    let mut renderer = Renderer::new(&ctx, FORMAT);
    for (k, (dx, dy)) in [(0.0, 0.0), (0.0, 0.0), (0.0, -40.0), (13.5, -90.25), (13.0, -90.0)].into_iter().enumerate() {
        let mut rec = Recorder::new();
        for (i, p) in parts.iter().enumerate() {
            rec.picture(p, Affine::translate((dx + (i % 2) as f64 * 260.0, dy + (i / 2) as f64 * 210.0)));
        }
        let frame = rec.finish();
        let ours = gpu(&ctx, &mut renderer, &frame, &res, size).await;
        scenes.push(compare(&format!("kept-{k}"), size, &ours, &cpu(&frame, &res, size)));
    }
    let kept = renderer.kept_counts();
    let mut out = format!("{{\"adapter\":{:?},\"backend\":\"{:?}\",\"kept\":[{},{}],\"scenes\":[", info.name, info.backend, kept.0, kept.1);
    for (i, s) in scenes.iter().enumerate() {
        let _ = write!(out, "{}{s}", if i > 0 { "," } else { "" });
    }
    out.push_str("]}");
    out
}

/// `picture` drawn by `renderer` into a `w × h` RGBA8 target over white, read back.
async fn gpu(ctx: &Context, renderer: &mut Renderer, picture: &Picture, res: &Resources, (w, h): (u32, u32)) -> Vec<u8> {
    let (device, queue) = (&ctx.device, &ctx.queue);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    let Ok(prepared) = renderer.prepare(ctx, res, &Frame { clear: Some(Color::WHITE), draws: &draws }, (w, h)) else { return Vec::new() };
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
    if !mapped(&slice).await {
        return Vec::new();
    }
    let Ok(data) = slice.get_mapped_range() else { return Vec::new() };
    data.chunks(padded as usize).flat_map(|r| &r[..row as usize]).copied().collect()
}

/// Maps `slice` for reading: `true` once it is mapped (the browser resolves the mapping as the GPU finishes).
async fn mapped(slice: &wgpu::BufferSlice<'_>) -> bool {
    let state: Rc<RefCell<(Option<bool>, Option<Waker>)>> = Rc::default();
    let done = Rc::clone(&state);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let mut s = done.borrow_mut();
        s.0 = Some(r.is_ok());
        if let Some(w) = s.1.take() {
            w.wake();
        }
    });
    std::future::poll_fn(|cx| {
        let mut s = state.borrow_mut();
        match s.0 {
            Some(ok) => Poll::Ready(ok),
            None => {
                s.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    })
    .await
}

fn cpu(picture: &Picture, res: &Resources, (w, h): (u32, u32)) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    if let Ok(mut target) = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8) {
        let draws = [Draw { picture, transform: Affine::IDENTITY }];
        CpuRenderer::new().render(&mut target, res, &Frame { clear: Some(Color::WHITE), draws: &draws });
    }
    px
}

/// The comparison of the GPU's pixels with the CPU's, as JSON: how many differ by more than 2, how many may, and the
/// largest difference. A readback that failed differs everywhere.
fn compare(name: &str, (w, h): (u32, u32), ours: &[u8], theirs: &[u8]) -> String {
    let (mut over, mut worst) = (0usize, 0u8);
    if ours.len() == theirs.len() {
        for (a, b) in ours.chunks(4).zip(theirs.chunks(4)) {
            let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            worst = worst.max(d);
            over += usize::from(d > 2);
        }
    } else {
        (over, worst) = ((w * h) as usize, 255);
    }
    format!("{{\"name\":{name:?},\"over\":{over},\"allowed\":{},\"worst\":{worst}}}", (w * h) as usize / 5000)
}
