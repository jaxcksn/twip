//! The developer tools: `twip-tools <command> ...` (golden, dump, dumpdiff, roundtrip, render, ...). Native only; the
//! browser module is `twip-wasm`. Speed is measured by the engine's Divan benchmarks (`cargo bench -p twip`).

#[cfg(target_os = "emscripten")]
fn main() {}

#[cfg(not(target_os = "emscripten"))]
mod cli;
#[cfg(not(target_os = "emscripten"))]
mod golden;

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
            let fonts = twip::internal::font::Fonts::system();
            let result = cli::dump::run(std::path::Path::new(out), paths, |doc| {
                twip::internal::layout::dump::page_text(&twip::internal::layout::layout(doc, &fonts), doc)
            });
            return exit_on_error(result.map(|n| println!("dumped {n} documents into {out}")));
        }
        Some("dumpdiff") => {
            // twip dumpdiff <a dir> <b dir> [--features f1,f2,...]
            let a: Vec<String> = args.skip(1).collect();
            let features = match a.iter().position(|x| x == "--features") {
                Some(i) => match twip::internal::features::parse(a.get(i + 1).map_or("", String::as_str)) {
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
        Some("roundtrip") => {
            // twip roundtrip [--edits all] [--keep <dir>] <docx or folders...> (cli/roundtrip.rs).
            let a: Vec<String> = args.skip(1).collect();
            return exit_on_error(cli::roundtrip::run(&a));
        }
        Some("units") => {
            // twip units <docx or folders...>: atomic units over the limit.
            let a: Vec<String> = args.skip(1).collect();
            return exit_on_error(cli::units::run(&a));
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
                match twip::internal::features::parse(&list) {
                    Ok(f) => Some(f),
                    Err(e) => return exit_on_error(Err(e)),
                }
            }
            None => None,
        };
        let fonts = twip::internal::font::Fonts::system();
        golden::run(&a, &fonts, std::path::Path::new("golden-report.txt"), features.as_ref());
        return;
    }
    if args.peek().map(String::as_str) == Some("diff") {
        // twip diff <fixture.docx> [page row]
        let a: Vec<String> = args.skip(1).collect();
        let want = (a.len() >= 3).then(|| (a[1].parse().unwrap_or(1), a[2].parse().unwrap_or(1)));
        golden::diff(std::path::Path::new(&a[0]), &twip::internal::font::Fonts::system(), want);
        return;
    }
    if args.peek().map(String::as_str) == Some("rows") {
        // twip rows <fixture.docx> <page> [from-row]
        let a: Vec<String> = args.skip(1).collect();
        let page = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        let from = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        golden::rows_of(std::path::Path::new(&a[0]), &twip::internal::font::Fonts::system(), page, from);
        return;
    }
    if args.peek().map(String::as_str) == Some("pairs") {
        // twip pairs <fixture.docx> <out.txt>: page row k n x_ours x_word per matched character
        let a: Vec<String> = args.skip(1).collect();
        golden::pairs(std::path::Path::new(&a[0]), &twip::internal::font::Fonts::system(), std::path::Path::new(&a[1]));
        return;
    }
    if args.peek().map(String::as_str) == Some("metrics") {
        // twip metrics <family> [bold] [italic]: the face's vertical metrics in font units
        let a: Vec<String> = args.skip(1).collect();
        let fonts = twip::internal::font::Fonts::system();
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
        use twip::api::input::{Key, Modifiers};
        let mut engine = twip::api::engine::Engine::new(twip::api::fonts::FontLibrary::system());
        let id = engine.create_session(Default::default());
        let e = engine.session(id).expect("a session");
        println!("warnings {:?}", e.open(&std::fs::read(&a[0]).expect("read document")).expect("open document"));
        let zoom = a.get(2).and_then(|z| z.parse::<f64>().ok()).unwrap_or(1.0);
        e.run(twip::api::command::Command::SetZoom(twip::api::command::Zoom::Percent(zoom * 100.0))).expect("zoom");
        // Every page, not only the first ones laid out at open: counting lines lays out the rest.
        e.word_count();
        if let Some(text) = a.get(3) {
            e.key(Key::Home, Modifiers { ctrl: true, ..Modifiers::default() });
            e.text(text);
            let f = e.host().frame_info();
            println!("caret {:?}", [f.caret_x, f.caret_y, f.caret_height]);
        }
        let f = e.host().frame_info();
        let (w, h) = (f.content_width, f.content_height.min(8000.0));
        e.host().set_size(w, h, 1.0);
        let info = ImageInfo::new((w.ceil() as i32, (h.ceil() as i32).min(8000)), ColorType::RGBA8888, AlphaType::Premul, None);
        let image = if gpu {
            let pixels = vulkan::render(e, &info).unwrap_or_else(|why| panic!("Vulkan: {why}"));
            skia_safe::images::raster_from_data(&info, skia_safe::Data::new_copy(&pixels), info.min_row_bytes()).expect("image from pixels")
        } else {
            let mut surface = surfaces::raster(&info, None, None).expect("raster surface");
            e.host().draw(surface.canvas(), w, h);
            surface.image_snapshot()
        };
        let png = image.encode(None, EncodedImageFormat::PNG, None).expect("PNG encode");
        std::fs::write(&a[1], png.as_bytes()).expect("write PNG");
        println!("{}: wrote {}", if gpu { "Graphite (Vulkan)" } else { "CPU raster" }, a[1]);
        return;
    }
    exit_on_error(Err("usage: twip-tools <dump|dumpdiff|roundtrip|units|golden|diff|rows|pairs|metrics|render> ...".into()));
}

#[cfg(not(target_os = "emscripten"))]
mod vulkan {
    use ash::vk::{self, Handle};
    use skia_safe::ImageInfo;
    use skia_safe::gpu::{self, graphite};

    /// Renders a frame offscreen with Graphite on the first Vulkan device and reads it back.
    pub fn render(session: &mut twip::api::session::Session, info: &ImageInfo) -> Result<Vec<u8>, String> {
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
            let pixels = draw(session, info, &entry, &instance, pdevice, &device, family);
            unsafe { device.destroy_device(None) };
            pixels
        })();
        unsafe { instance.destroy_instance(None) };
        result
    }

    fn draw(
        session: &mut twip::api::session::Session,
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
        session.host().draw(surface.canvas(), f64::from(info.width()), f64::from(info.height()));
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
