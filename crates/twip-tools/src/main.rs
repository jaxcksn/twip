//! The developer tools: `twip-tools <command> ...` (golden, dump, dumpdiff, roundtrip, render, oracle, pdf, presets, chrome-fonts, autocorrect-list, ...). Native only; the
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
    use skia_safe::{AlphaType, ColorType, EncodedImageFormat, ImageInfo};

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
                let mut doc = doc.clone();
                doc.review.view = twip::internal::docx::review::MarkupView::NoMarkup;
                twip::internal::layout::dump::page_text(&twip::internal::layout::layout(&doc, &fonts), &doc)
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
        Some("pdf") => {
            // twip pdf <docx> <out.pdf> [--password <text>] [--keywords <a,b,...>]: the document's PDF export
            // (cli/pdf.rs).
            let a: Vec<String> = args.skip(1).collect();
            let (Some(docx), Some(out)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: pdf <docx> <out.pdf> [--password <text>] [--keywords <a,b,...>]".into()));
            };
            let flag = |name: &str| a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).cloned();
            let options = twip::api::session::PdfOptions {
                open_password: flag("--password"),
                keywords: flag("--keywords").map(|k| k.split(',').map(str::to_string).collect()).unwrap_or_default(),
            };
            return exit_on_error(cli::pdf::run(std::path::Path::new(docx), std::path::Path::new(out), &options));
        }
        Some("oracle") => {
            // twip oracle <docx> <out dir> [scale]: every page drawn by the Skia renderer and by twip-graphics at
            // `scale` px per point (default 1), side by side with a diff, and how far they differ (cli/oracle.rs).
            let a: Vec<String> = args.skip(1).collect();
            let (Some(docx), Some(out)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: oracle <docx> <out dir> [scale]".into()));
            };
            let scale = a.get(2).and_then(|s| s.parse::<f32>().ok()).filter(|s| *s > 0.0).unwrap_or(1.0);
            let result = cli::oracle::run(std::path::Path::new(docx), std::path::Path::new(out), scale);
            return exit_on_error(result.map(|pages| {
                let pct = |n: usize, of: usize| 100.0 * n as f64 / of.max(1) as f64;
                for (i, p) in pages.iter().enumerate() {
                    println!(
                        "page {:3}: {:7} ink px, {:6.2}% differ by > 16, {:6.2}% by > 64, largest {}; {} image px, {:.2}% by > 64",
                        i + 1,
                        p.ink,
                        pct(p.over16, p.ink),
                        pct(p.over64, p.ink),
                        p.max,
                        p.image,
                        pct(p.image_over64, p.image)
                    );
                }
                let (ink, o16, o64) = pages.iter().fold((0, 0, 0), |(a, b, c), p| (a + p.ink, b + p.over16, c + p.over64));
                println!("all {} pages: {:.2}% of ink px differ by > 16, {:.2}% by > 64 (into {out})", pages.len(), pct(o16, ink), pct(o64, ink));
            }));
        }
        Some("autocorrect-list") => {
            // twip autocorrect-list <dir> <out dir>: the shipped AutoCorrect lists (cli/autocorrect.rs).
            let a: Vec<String> = args.skip(1).collect();
            let (Some(xml), Some(out)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: autocorrect-list <dir> <out dir>".into()));
            };
            return exit_on_error(cli::autocorrect::run(std::path::Path::new(xml), std::path::Path::new(out)));
        }
        Some("chrome-fonts") => {
            // twip chrome-fonts <dir> <out dir>: the bundled chrome typeface subsets (cli/fonts.rs).
            let a: Vec<String> = args.skip(1).collect();
            let (Some(dir), Some(out)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: chrome-fonts <dir> <out dir>".into()));
            };
            return exit_on_error(cli::fonts::run(std::path::Path::new(dir), std::path::Path::new(out)));
        }
        Some("presets") => {
            // twip presets <presetShapeDefinitions.xml> <out.rs>: the engine's preset geometry table (cli/presets.rs).
            let a: Vec<String> = args.skip(1).collect();
            let (Some(xml), Some(out)) = (a.first(), a.get(1)) else {
                return exit_on_error(Err("usage: presets <presetShapeDefinitions.xml> <out.rs>".into()));
            };
            return exit_on_error(cli::presets::run(std::path::Path::new(xml), std::path::Path::new(out)));
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
        let f = fonts.face(&a[0], None, a.iter().any(|x| x == "bold"), a.iter().any(|x| x == "italic")).expect("no such font");
        println!(
            "{} upem {} ascent {} descent {} leading {} sup size {} offset {} sub size {} offset {}",
            f.family, f.upem, f.ascent, f.descent, f.leading, f.sup_size, f.sup_offset, f.sub_size, f.sub_offset
        );
        return;
    }
    if args.peek().map(String::as_str) == Some("render") {
        // twip render <doc.docx> <out.png> [zoom] [text] [--gpu | --skia | --graphics]: the editor's view of a document
        // (a CPU frame, drawn by the build's CPU screen or the one named; or the window's frame, drawn by twip-graphics
        // on the GPU), with `text` typed at the start of the first paragraph.
        // `--appearance=dark|darkWithLightPage|light` and `--theme=dark|light` set the view options of the same names.
        let a: Vec<String> = args.skip(1).collect();
        let gpu = a.iter().any(|x| x == "--gpu");
        let flag = |name: &str| a.iter().find_map(|x| x.strip_prefix(&format!("--{name}="))).map(|v| format!("\"{v}\""));
        let options = twip::api::command::ViewOptions {
            appearance: flag("appearance").map(|v| serde_json::from_str(&v).expect("an appearance: light, dark or darkWithLightPage")),
            theme: flag("theme").map(|v| serde_json::from_str(&v).expect("a theme: light or dark")),
            ..Default::default()
        };
        let screen = if a.iter().any(|x| x == "--skia") { Some(false) } else { a.iter().any(|x| x == "--graphics").then_some(true) };
        let a: Vec<String> = a.into_iter().filter(|x| !x.starts_with("--")).collect();
        use twip::api::input::{Key, Modifiers};
        let mut engine = twip::api::engine::Engine::new(twip::api::fonts::FontLibrary::system());
        let id = engine.create_session(Default::default());
        let e = engine.session(id).expect("a session");
        println!("warnings {:?}", e.open(&std::fs::read(&a[0]).expect("read document")).expect("open document"));
        e.run(twip::api::command::Command::SetViewOptions(options)).expect("view options");
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
        if let Some(on) = screen {
            e.host().set_graphics_screen(on);
        }
        let (pw, ph) = (info.width() as u32, info.height() as u32);
        let mut pixels = vec![0u8; pw as usize * ph as usize * 4];
        if gpu {
            e.host().frame_gpu_rgba(&mut pixels, pw, ph).expect("a GPU frame");
        } else {
            e.host().frame_rgba(&mut pixels, pw, ph).expect("a CPU frame");
        }
        let image = skia_safe::images::raster_from_data(&info, skia_safe::Data::new_copy(&pixels), info.min_row_bytes()).expect("image from pixels");
        let png = image.encode(None, EncodedImageFormat::PNG, None).expect("PNG encode");
        std::fs::write(&a[1], png.as_bytes()).expect("write PNG");
        println!("{}: wrote {}", if gpu { "GPU" } else { "CPU raster" }, a[1]);
        return;
    }
    exit_on_error(Err("usage: twip-tools <dump|dumpdiff|roundtrip|units|golden|diff|rows|pairs|metrics|render|oracle> ...".into()));
}
