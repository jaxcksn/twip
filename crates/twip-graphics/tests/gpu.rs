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
use twip_graphics::scene::{Picture, Recorder};

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

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// `picture` under `transform` drawn by `renderer` into a `w × h` RGBA8 target over white, read back.
fn gpu(ctx: &Context, renderer: &mut Renderer, picture: &Picture, resources: &Resources, (w, h): (u32, u32), transform: Affine) -> Vec<u8> {
    let (device, queue) = (&ctx.device, &ctx.queue);
    let format = FORMAT;
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
    let draws = [Draw { picture, transform }];
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

fn cpu(picture: &Picture, resources: &Resources, (w, h): (u32, u32), transform: Affine) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("a buffer that fits");
    let draws = [Draw { picture, transform }];
    CpuRenderer::new().render(&mut target, resources, &Frame { clear: Some(Color::WHITE), draws: &draws });
    px
}

/// Draws `picture` on both backends and checks they agree.
fn same(name: &str, size: (u32, u32), picture: &Picture, resources: &Resources) {
    let Some(ctx) = context() else { return };
    let mut renderer = Renderer::new(ctx, FORMAT);
    let ours = gpu(ctx, &mut renderer, picture, resources, size, Affine::IDENTITY);
    compare(name, size, &ours, &cpu(picture, resources, size, Affine::IDENTITY));
}

/// Checks the GPU's pixels against the CPU's (see the module's tolerance).
fn compare(name: &str, size: (u32, u32), ours: &[u8], theirs: &[u8]) {
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
    // TWIP_GRAPHICS_DIFF=1 writes the images for a comparison that passes too.
    if over > allowed || std::env::var_os("TWIP_GRAPHICS_DIFF").is_some() {
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("out");
        std::fs::create_dir_all(&out).expect("the output folder");
        let write = |file: &str, px: &[u8]| {
            let f = std::io::BufWriter::new(std::fs::File::create(out.join(file)).expect("a file"));
            let mut e = png::Encoder::new(f, size.0, size.1);
            e.set_color(png::ColorType::Rgba);
            e.write_header().expect("a header").write_image_data(px).expect("pixels");
        };
        write(&format!("{name}.gpu.png"), ours);
        write(&format!("{name}.gpu-diff.png"), &diff);
    }
    if over > allowed {
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

/// Kept pictures as Twip draws them: each frame a picture of its own placing them. Moved by whole pixels they are
/// drawn again without being prepared; moved by a fraction of one, they are prepared in the frame. Every frame matches
/// the CPU's.
#[test]
fn kept_pictures_draw_where_they_move() {
    let Some(ctx) = context() else { return };
    let mut resources = Resources::default();
    let text = common::scenes::text(&mut resources);
    let images = common::scenes::images(&mut resources);
    let parts = [text, images, common::scenes::layers(), common::scenes::blend_modes(), common::scenes::paths(), common::scenes::gradients()];
    let size = (780, 380);
    let frame = |dx: f64, dy: f64| {
        let mut rec = Recorder::new();
        for (i, p) in parts.iter().enumerate() {
            rec.picture(p, Affine::translate((dx + (i % 3) as f64 * 260.0, dy + (i / 3) as f64 * 210.0)));
        }
        rec.finish()
    };
    let mut renderer = Renderer::new(ctx, FORMAT);
    let moves = [(0.0, 0.0), (0.0, 0.0), (0.0, -40.0), (13.0, -90.0), (13.5, -90.25), (13.0, -90.0)];
    for (k, &(dx, dy)) in moves.iter().enumerate() {
        let picture = frame(dx, dy);
        let ours = gpu(ctx, &mut renderer, &picture, &resources, size, Affine::IDENTITY);
        compare(&format!("kept-{k}"), size, &ours, &cpu(&picture, &resources, size, Affine::IDENTITY));
    }
    // The first frame draws the parts itself; the second prepares them to be kept; the third, fourth and last draw them
    // again; the fifth, a fraction of a pixel over, draws them itself.
    assert_eq!(renderer.kept_counts(), (6, 18));
}

/// A page of line pictures, kept: an edit makes a new page with one new line, and the page is prepared again from its
/// unchanged lines' kept preparations and the new line's.
#[test]
fn kept_pages_reuse_their_unchanged_lines() {
    use twip_graphics::kurbo::Shape;
    let Some(ctx) = context() else { return };
    let resources = Resources::default();
    let line = |k: u32| {
        let mut rec = Recorder::new();
        for j in 0..6 {
            let x = 4.0 + f64::from(j) * 30.0 + f64::from(k % 5) * 1.25;
            rec.fill_rect(common::scenes::rect(x, 3.0, x + 22.0, 13.0), Color::rgb(0x20_2020 + 0x10_1408 * (k % 4)));
        }
        rec.fill_path(kurbo::Circle::new((200.0 + f64::from(k) * 3.5, 8.0), 6.0).to_path(0.01).elements(), common::scenes::BLUE);
        rec.finish()
    };
    let page = |lines: &[Picture]| {
        let mut rec = Recorder::new();
        for (i, l) in lines.iter().enumerate() {
            rec.picture(l, Affine::translate((0.0, i as f64 * 18.0)));
        }
        rec.finish()
    };
    let mut lines: Vec<Picture> = (0..12).map(line).collect();
    let first = page(&lines);
    lines[5] = line(40);
    let edited = page(&lines);
    let size = (300, 240);
    let mut renderer = Renderer::new(ctx, FORMAT);
    let frames = [(&first, 10.0), (&first, 10.0), (&first, -7.0), (&edited, -7.0), (&edited, -7.0), (&edited, 23.0)];
    let mut counts = Vec::new();
    for (k, &(p, dy)) in frames.iter().enumerate() {
        let mut rec = Recorder::new();
        rec.picture(p, Affine::translate((6.0, dy)));
        let frame = rec.finish();
        let ours = gpu(ctx, &mut renderer, &frame, &resources, size, Affine::IDENTITY);
        compare(&format!("kept-lines-{k}"), size, &ours, &cpu(&frame, &resources, size, Affine::IDENTITY));
        counts.push(renderer.kept_counts());
    }
    // 0: the page and its lines drawn in the frame. 1: the page kept, its 12 lines kept with it. 2: the page drawn
    // again. 3: the edited page drawn in the frame, from 11 kept lines (the new one drawn in the frame). 4: the edited
    // page kept: 11 lines drawn again, the new one kept. 5: the edited page drawn again.
    assert_eq!(counts, [(0, 0), (13, 0), (13, 1), (13, 12), (15, 23), (15, 24)]);
}

/// A lost device (destroyed here, as a driver reset loses it) is reported, by the context and by the next prepare,
/// without a panic; the pictures live on the CPU, so a new device draws them.
#[test]
fn a_lost_device_is_reported_and_a_new_one_draws() {
    if context().is_none() {
        return;
    }
    let scene = common::scenes::layers();
    let (resources, size) = (Resources::default(), (128, 144));
    let lost = block_on(Context::new(None)).expect("a device of its own");
    let mut renderer = Renderer::new(&lost, FORMAT);
    gpu(&lost, &mut renderer, &scene, &resources, size, Affine::IDENTITY);
    // Lost between a frame's preparation and its drawing: drawing it and submitting it don't panic.
    let draws = [Draw { picture: &scene, transform: Affine::IDENTITY }];
    let frame = Frame { clear: Some(Color::WHITE), draws: &draws };
    let prepared = renderer.prepare(&lost, &resources, &frame, size).expect("not lost yet");
    let target = lost.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    lost.device.destroy();
    let mut encoder = lost.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    renderer.render(&mut encoder, &target.create_view(&wgpu::TextureViewDescriptor::default()), &prepared);
    lost.queue.submit([encoder.finish()]);
    let _ = lost.device.poll(wgpu::PollType::wait_indefinitely());
    assert!(lost.is_lost(), "the context reports the loss");
    assert_eq!(renderer.prepare(&lost, &resources, &frame, size).err(), Some(twip_graphics::gpu::Error::Lost));
    let fresh = block_on(Context::new(None)).expect("a new device");
    let mut renderer = Renderer::new(&fresh, FORMAT);
    let ours = gpu(&fresh, &mut renderer, &scene, &resources, size, Affine::IDENTITY);
    compare("after-loss", size, &ours, &cpu(&scene, &resources, size, Affine::IDENTITY));
}

/// Images past the budget go when a frame doesn't draw them, and come back when one does, kept pictures' included.
#[test]
fn evicted_images_are_decoded_again() {
    let Some(ctx) = context() else { return };
    let mut resources = Resources::default();
    let images = common::scenes::images(&mut resources);
    let rects = common::scenes::rects();
    let size = (128, 144);
    let mut renderer = Renderer::new(ctx, FORMAT);
    renderer.set_image_budget(1);
    let placed = |p: &Picture| {
        let mut rec = Recorder::new();
        rec.picture(p, Affine::IDENTITY);
        rec.finish()
    };
    // Drawn twice (kept the second time), then not at all (its images evicted), then again from its kept preparation.
    for (k, p) in [&images, &images, &rects, &images].into_iter().enumerate() {
        let frame = placed(p);
        let ours = gpu(ctx, &mut renderer, &frame, &resources, size, Affine::IDENTITY);
        compare(&format!("evicted-{k}"), size, &ours, &cpu(&frame, &resources, size, Affine::IDENTITY));
        if k == 2 {
            assert_eq!(renderer.images_held(), 0, "a frame drawing no image evicts them all");
        }
    }
    assert_eq!(renderer.kept_counts().1, 1, "the last frame drew the kept picture");
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
