//! The developer tools: `twip-tools <command> ...` (golden, dump, dumpdiff, roundtrip, render, pdf, presets, chrome-fonts, autocorrect-list, ...). Native only; the
//! browser module is `twip-wasm`. Speed is measured by the engine's Divan benchmarks (`cargo bench -p twip`).

#[cfg(target_family = "wasm")]
fn main() {}

#[cfg(not(target_family = "wasm"))]
mod cli;
#[cfg(not(target_family = "wasm"))]
mod golden;

/// Prints a command's error and exits with status 1.
#[cfg(not(target_family = "wasm"))]
fn exit_on_error(result: Result<(), String>) {
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let mut args = std::env::args().skip(1).peekable();
    match args.peek().map(String::as_str) {
        Some("dump") => {
            // twip dump [--hyphenation <lang>=<patterns file>]... <out dir> <docx or folders...>: every page's items as
            // text, to compare engine builds.
            let mut a: Vec<String> = args.skip(1).collect();
            let hyphenation = match cli::hyphenation(&mut a) {
                Ok(h) => h,
                Err(e) => return exit_on_error(Err(e)),
            };
            let Some((out, paths)) = a.split_first() else {
                return exit_on_error(Err("usage: dump [--hyphenation <lang>=<patterns file>]... <out dir> <docx or folders...>".into()));
            };
            let fonts = cli::system_fonts();
            let result = cli::dump::run(std::path::Path::new(out), paths, |doc| {
                let mut doc = doc.clone();
                doc.review.view = twip::internal::docx::review::MarkupView::NoMarkup;
                doc.hyphenation = hyphenation.clone();
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
        // twip golden [--features f1,...] [--hyphenation <lang>=<patterns file>]... <fixture folders or .docx...>:
        // compare layouts with reference (golden) geometry; --features keeps documents within a feature set.
        let mut a: Vec<String> = args.skip(1).collect();
        let hyphenation = match cli::hyphenation(&mut a) {
            Ok(h) => h,
            Err(e) => return exit_on_error(Err(e)),
        };
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
        let fonts = cli::system_fonts();
        golden::run(&a, &fonts, hyphenation.as_ref(), std::path::Path::new("golden-report.txt"), features.as_ref());
        return;
    }
    if args.peek().map(String::as_str) == Some("diff") {
        // twip diff <fixture.docx> [page row]
        let a: Vec<String> = args.skip(1).collect();
        let want = (a.len() >= 3).then(|| (a[1].parse().unwrap_or(1), a[2].parse().unwrap_or(1)));
        golden::diff(std::path::Path::new(&a[0]), &cli::system_fonts(), want);
        return;
    }
    if args.peek().map(String::as_str) == Some("rows") {
        // twip rows <fixture.docx> <page> [from-row]
        let a: Vec<String> = args.skip(1).collect();
        let page = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
        let from = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        golden::rows_of(std::path::Path::new(&a[0]), &cli::system_fonts(), page, from);
        return;
    }
    if args.peek().map(String::as_str) == Some("pairs") {
        // twip pairs <fixture.docx> <out.txt>: page row k n x_ours x_word per matched character
        let a: Vec<String> = args.skip(1).collect();
        golden::pairs(std::path::Path::new(&a[0]), &cli::system_fonts(), std::path::Path::new(&a[1]));
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
        // twip render <doc.docx> <out.png> [zoom] [text] [--gpu]: the editor's view of a document (a CPU frame, or the
        // window's frame drawn on the GPU), with `text` typed at the start of the first paragraph.
        // `--appearance=dark|darkWithLightPage|light` and `--theme=dark|light` set the view options of the same names.
        let a: Vec<String> = args.skip(1).collect();
        let gpu = a.iter().any(|x| x == "--gpu");
        let flag = |name: &str| a.iter().find_map(|x| x.strip_prefix(&format!("--{name}="))).map(|v| format!("\"{v}\""));
        let options = twip::api::command::ViewOptions {
            appearance: flag("appearance").map(|v| serde_json::from_str(&v).expect("an appearance: light, dark or darkWithLightPage")),
            theme: flag("theme").map(|v| serde_json::from_str(&v).expect("a theme: light or dark")),
            ..Default::default()
        };
        let a: Vec<String> = a.into_iter().filter(|x| !x.starts_with("--")).collect();
        use twip::api::input::{Key, Modifiers};
        let mut engine = twip::api::engine::Engine::new(twip::api::fonts::FontLibrary::system());
        engine.fonts().set_character_fallback(cli::CHARACTER_FALLBACK.map(String::from).to_vec());
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
        let (pw, ph) = (w.ceil() as u32, (h.ceil() as u32).min(8000));
        let mut pixels = vec![0u8; pw as usize * ph as usize * 4];
        if gpu {
            e.host().frame_gpu_rgba(&mut pixels, pw, ph).expect("a GPU frame");
        } else {
            e.host().frame_rgba(&mut pixels, pw, ph).expect("a CPU frame");
        }
        // The frame is opaque (the desk and the pages), so its premultiplied pixels are straight RGBA.
        let file = std::io::BufWriter::new(std::fs::File::create(&a[1]).expect("create the PNG"));
        let mut encoder = png::Encoder::new(file, pw, ph);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().and_then(|mut w| w.write_image_data(&pixels)).expect("write PNG");
        println!("{}: wrote {}", if gpu { "GPU" } else { "CPU raster" }, a[1]);
        return;
    }
    exit_on_error(Err("usage: twip-tools <dump|dumpdiff|roundtrip|units|golden|diff|rows|pairs|metrics|render> ...".into()));
}
