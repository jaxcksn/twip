//! The developer tools: `twip-tools <command> ...` (dump, dumpdiff, bench, render, ...). Native only; the browser
//! module is `twip-wasm`.

#[cfg(target_os = "emscripten")]
fn main() {}

#[cfg(not(target_os = "emscripten"))]
mod cli;

/// Prints a command's error and exits with status 1.
#[cfg(not(target_os = "emscripten"))]
fn exit_on_error(result: Result<(), String>) {
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "emscripten"))]
fn main() {
    use skia_safe::{AlphaType, ColorType, EncodedImageFormat, ImageInfo, surfaces};

    let mut args = std::env::args().skip(1).peekable();
    match args.peek().map(String::as_str) {
        Some("dump") => {
            // twip dump <out dir> <docx or folders...>: every page's items as text, to compare engine builds.
            let a: Vec<String> = args.skip(1).collect();
            let Some((out, paths)) = a.split_first() else {
                return exit_on_error(Err("usage: dump <out dir> <docx or folders...>".into()));
            };
            let fonts = twip::font::Fonts::system();
            let result =
                cli::dump::run(std::path::Path::new(out), paths, |doc| twip::layout::dump::page_text(&twip::layout::layout(doc, &fonts), doc));
            return exit_on_error(result.map(|n| println!("dumped {n} documents into {out}")));
        }
        Some("dumpdiff") => {
            // twip dumpdiff <a dir> <b dir> [--features f1,f2,...]
            let a: Vec<String> = args.skip(1).collect();
            let features = match a.iter().position(|x| x == "--features") {
                Some(i) => match twip::features::parse(a.get(i + 1).map_or("", String::as_str)) {
                    Ok(f) => Some(f),
                    Err(e) => return exit_on_error(Err(e)),
                },
                None => None,
            };
            let (Some(x), Some(y)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: dumpdiff <a dir> <b dir> [--features f1,f2,...]".into()));
            };
            return exit_on_error(cli::dumpdiff::run(std::path::Path::new(x), std::path::Path::new(y), features.as_ref()));
        }
        Some("editlatency") => {
            // twip editlatency <doc.docx> [n]: the editor's per-input times (cli/editlatency.rs).
            let a: Vec<String> = args.skip(1).collect();
            if a.first().is_some_and(|x| x == "--merge") {
                let Some(doc) = a.get(1) else { return exit_on_error(Err("usage: editlatency --merge <doc.docx> [n]".into())) };
                return exit_on_error(cli::editlatency::run_merge(doc, a.get(2).and_then(|n| n.parse().ok()).unwrap_or(100)));
            }
            if a.first().is_some_and(|x| x == "--depth") {
                let Some(doc) = a.get(1) else { return exit_on_error(Err("usage: editlatency --depth <doc.docx> [n]".into())) };
                return exit_on_error(cli::editlatency::run_depth(doc, a.get(2).and_then(|n| n.parse().ok()).unwrap_or(300)));
            }
            if a.first().is_some_and(|x| x == "--tracking") {
                let Some(doc) = a.get(1) else { return exit_on_error(Err("usage: editlatency --tracking <doc.docx> [n]".into())) };
                return exit_on_error(cli::editlatency::run_tracking(doc, a.get(2).and_then(|n| n.parse().ok()).unwrap_or(100)));
            }
            let Some(doc) = a.first() else { return exit_on_error(Err("usage: editlatency [--depth|--merge|--tracking] <doc.docx> [n]".into())) };
            return exit_on_error(cli::editlatency::run(doc, a.get(1).and_then(|n| n.parse().ok()).unwrap_or(100)));
        }
        Some("roundtrip") => {
            // twip roundtrip [--edits all] [--keep <dir>] <docx or folders...> (cli/roundtrip.rs).
            let a: Vec<String> = args.skip(1).collect();
            return exit_on_error(cli::roundtrip::run(&a));
        }
        Some("replay") => {
            // twip replay <doc.docx> <log.jsonl> [--fonts <dir>] [--dump] [--state] (cli/replay.rs).
            let a: Vec<String> = args.skip(1).collect();
            return exit_on_error(cli::replay::run(&a));
        }
        Some("units") => {
            // twip units <docx or folders...>: atomic units over the limit.
            let a: Vec<String> = args.skip(1).collect();
            return exit_on_error(cli::units::run(&a));
        }
        Some("fullbench") => {
            let a: Vec<String> = args.skip(1).collect();
            let Some(doc) = a.first() else { return exit_on_error(Err("usage: fullbench <doc.docx> [runs]".into())) };
            return exit_on_error(cli::fullbench::run(doc, a.get(1).and_then(|n| n.parse().ok()).unwrap_or(5)));
        }
        Some("reviewbench") => {
            // twip reviewbench <doc.docx>: open, view switches, accept and reject all (cli/reviewbench.rs).
            let Some(doc) = args.nth(1) else { return exit_on_error(Err("usage: reviewbench <doc.docx>".into())) };
            return exit_on_error(cli::reviewbench::run(&doc));
        }
        Some("memprofile") => {
            let a: Vec<String> = args.skip(1).collect();
            let Some(doc) = a.first() else { return exit_on_error(Err("usage: memprofile <doc.docx> [--soak N]".into())) };
            let soak = a.iter().position(|x| x == "--soak").and_then(|i| a.get(i + 1)).and_then(|n| n.parse().ok());
            return exit_on_error(cli::memprofile::run(doc, soak));
        }
        _ => {}
    }
    if args.peek().map(String::as_str) == Some("golden") {
        // twip golden [--features f1,...] <fixture folders or .docx...>: compare layouts with reference (golden)
        // geometry; --features keeps documents within a feature set.
        let mut a: Vec<String> = args.skip(1).collect();
        let features = match a.iter().position(|x| x == "--features") {
            Some(i) => {
                let list = a.get(i + 1).cloned().unwrap_or_default();
                a.drain(i..(i + 2).min(a.len()));
                match twip::features::parse(&list) {
                    Ok(f) => Some(f),
                    Err(e) => return exit_on_error(Err(e)),
                }
            }
            None => None,
        };
        let fonts = twip::font::Fonts::system();
        twip::golden::run(&a, &fonts, std::path::Path::new("golden-report.txt"), features.as_ref());
        return;
    }
    if args.peek().map(String::as_str) == Some("bench") {
        // twip bench <doc.docx> [runs]: read, layout and keystroke times (ms) (cli/bench.rs).
        let a: Vec<String> = args.skip(1).collect();
        let Some(doc) = a.first() else { return exit_on_error(Err("usage: bench <doc.docx> [runs]".into())) };
        return exit_on_error(cli::bench::run(doc, a.get(1).and_then(|n| n.parse().ok()).unwrap_or(5)));
    }
    if args.peek().map(String::as_str) == Some("diff") {
        // twip diff <fixture.docx> [page row]
        let a: Vec<String> = args.skip(1).collect();
        let want = (a.len() >= 3).then(|| (a[1].parse().unwrap_or(1), a[2].parse().unwrap_or(1)));
        twip::golden::diff(std::path::Path::new(&a[0]), &twip::font::Fonts::system(), want);
        return;
    }
    if args.peek().map(String::as_str) == Some("rows") {
        // twip rows <fixture.docx> <page> [from-row]
        let a: Vec<String> = args.skip(1).collect();
        let page = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        let from = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        twip::golden::rows_of(std::path::Path::new(&a[0]), &twip::font::Fonts::system(), page, from);
        return;
    }
    if args.peek().map(String::as_str) == Some("pairs") {
        // twip pairs <fixture.docx> <out.txt>: page row k n x_ours x_word per matched character
        let a: Vec<String> = args.skip(1).collect();
        twip::golden::pairs(std::path::Path::new(&a[0]), &twip::font::Fonts::system(), std::path::Path::new(&a[1]));
        return;
    }
    if args.peek().map(String::as_str) == Some("metrics") {
        // twip metrics <family> [bold] [italic]: the face's vertical metrics in font units
        let a: Vec<String> = args.skip(1).collect();
        let fonts = twip::font::Fonts::system();
        let f = fonts.face(&a[0], a.iter().any(|x| x == "bold"), a.iter().any(|x| x == "italic")).expect("no such font");
        println!(
            "{} upem {} ascent {} descent {} leading {} sup size {} offset {} sub size {} offset {}",
            f.family, f.upem, f.ascent, f.descent, f.leading, f.sup_size, f.sup_offset, f.sub_size, f.sub_offset
        );
        return;
    }
    if args.peek().map(String::as_str) == Some("render") {
        // twip render <doc.docx> <out.png> [zoom] [text] [--vulkan]: the editor's view of a document (CPU raster, or
        // Graphite on the first Vulkan device), with `text` typed at the start of the first paragraph.
        let a: Vec<String> = args.skip(1).collect();
        let gpu = a.iter().any(|x| x == "--vulkan");
        let a: Vec<String> = a.into_iter().filter(|x| x != "--vulkan").collect();
        let mut e = twip::host::Engine::with_fonts(twip::font::Fonts::system());
        println!("{}", e.open(&std::fs::read(&a[0]).expect("read document")));
        e.set_zoom(a.get(2).and_then(|z| z.parse().ok()).unwrap_or(1.0));
        // Every page, not only the first ones laid out at open.
        while e.idle() {}
        if let Some(text) = a.get(3) {
            e.key("Home", true, false, false);
            e.text(text);
            println!("caret {:?}", e.caret());
        }
        let [w, h] = e.content_size();
        let info = ImageInfo::new((w.ceil() as i32, (h.ceil() as i32).min(8000)), ColorType::RGBA8888, AlphaType::Premul, None);
        let image = if gpu {
            let pixels = vulkan::render(&mut e, &info).unwrap_or_else(|why| panic!("Vulkan: {why}"));
            skia_safe::images::raster_from_data(&info, skia_safe::Data::new_copy(&pixels), info.min_row_bytes()).expect("image from pixels")
        } else {
            let mut surface = surfaces::raster(&info, None, None).expect("raster surface");
            e.draw(surface.canvas(), 0.0, 0.0, w, h, 1.0);
            surface.image_snapshot()
        };
        let png = image.encode(None, EncodedImageFormat::PNG, None).expect("PNG encode");
        std::fs::write(&a[1], png.as_bytes()).expect("write PNG");
        println!("{}: wrote {}", if gpu { "Graphite (Vulkan)" } else { "CPU raster" }, a[1]);
        return;
    }
    exit_on_error(Err(
        "usage: twip-tools <dump|dumpdiff|editlatency|roundtrip|replay|units|fullbench|memprofile|golden|bench|diff|rows|pairs|metrics|render> ..."
            .into(),
    ));
}

#[cfg(not(target_os = "emscripten"))]
mod vulkan {
    use ash::vk::{self, Handle};
    use skia_safe::ImageInfo;
    use skia_safe::gpu::{self, graphite};

    /// Renders a frame offscreen with Graphite on the first Vulkan device and reads it back.
    pub fn render(engine: &mut twip::host::Engine, info: &ImageInfo) -> Result<Vec<u8>, String> {
        let entry = unsafe { ash::Entry::load() }.map_err(|e| e.to_string())?;
        // Graphite needs the 1.3/1.4 core entry points the loader only exposes when asked for that version.
        let version = unsafe { entry.try_enumerate_instance_version() }.ok().flatten().unwrap_or(vk::API_VERSION_1_1);
        let app = vk::ApplicationInfo::default().api_version(version);
        let instance =
            unsafe { entry.create_instance(&vk::InstanceCreateInfo::default().application_info(&app), None) }.map_err(|e| e.to_string())?;
        let result = (|| {
            let pdevice = *unsafe { instance.enumerate_physical_devices() }.map_err(|e| e.to_string())?.first().ok_or("no Vulkan device")?;
            let family = unsafe { instance.get_physical_device_queue_family_properties(pdevice) }
                .iter()
                .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .ok_or("no graphics queue")? as u32;
            let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(family).queue_priorities(&[1.0])];
            let device = unsafe { instance.create_device(pdevice, &vk::DeviceCreateInfo::default().queue_create_infos(&queues), None) }
                .map_err(|e| e.to_string())?;
            let pixels = draw(engine, info, &entry, &instance, pdevice, &device, family);
            unsafe { device.destroy_device(None) };
            pixels
        })();
        unsafe { instance.destroy_instance(None) };
        result
    }

    fn draw(
        engine: &mut twip::host::Engine,
        info: &ImageInfo,
        entry: &ash::Entry,
        instance: &ash::Instance,
        pdevice: vk::PhysicalDevice,
        device: &ash::Device,
        family: u32,
    ) -> Result<Vec<u8>, String> {
        let queue = unsafe { device.get_device_queue(family, 0) };
        let resolve = |of: gpu::vk::GetProcOf| unsafe {
            match of {
                gpu::vk::GetProcOf::Instance(i, name) => entry.get_instance_proc_addr(vk::Instance::from_raw(i as _), name),
                gpu::vk::GetProcOf::Device(d, name) => instance.get_device_proc_addr(vk::Device::from_raw(d as _), name),
            }
            .map_or(std::ptr::null(), |f| f as *const _)
        };
        // SAFETY: the Vulkan handles outlive the context, which is dropped before this returns.
        let backend = unsafe {
            gpu::vk::BackendContext::new_builder(
                instance.handle().as_raw() as _,
                pdevice.as_raw() as _,
                device.handle().as_raw() as _,
                (queue.as_raw() as _, family as usize),
                &resolve,
                None,
            )
            .build()
        };
        let mut context = graphite::vk::context_factory::make_vulkan(&backend, None).ok_or("Graphite rejected the device")?;
        let mut recorder = context.make_recorder(None).ok_or("no recorder")?;
        let mut surface = graphite::surfaces::render_target(&mut recorder, info, gpu::Mipmapped::No, None, None).ok_or("no surface")?;
        engine.draw(surface.canvas(), 0.0, 0.0, f64::from(info.width()), f64::from(info.height()), 1.0);
        let mut recording = recorder.snap().ok_or("snap failed")?;
        context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording));
        context.submit_and_wait();
        let mut pixels = vec![0u8; info.compute_min_byte_size()];
        if !context.read_pixels(&mut surface, info, &mut pixels, info.min_row_bytes(), (0, 0)) {
            return Err("readback failed".into());
        }
        Ok(pixels)
    }
}
