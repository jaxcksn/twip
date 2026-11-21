//! Render output snapshots

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use twip_graphics::cpu::{Format, Target};
use twip_graphics::kurbo::Affine;

use super::Graphics;
use crate::layout::tests::output::{bundled_fonts, corpus, crate_dir, entries, escape, root};
use crate::render::Renderer;

/// Device pixels per point: 96 dpi.
const PX_PER_PT: f64 = 96.0 / 72.0;
/// A pixel fails when a channel differs by more than this.
const CHANNEL: u8 = 2;
/// A page fails when more than one pixel in this many fails.
const PER_FAILED: usize = 5000;

/// Straight RGBA8 pixels, rows packed.
#[derive(Clone, PartialEq)]
pub(crate) struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    fn decode(png: &[u8]) -> Option<Rgba> {
        let mut d = zune_png::PngDecoder::new(zune_png::zune_core::bytestream::ZCursor::new(png));
        let px = d.decode_raw().ok()?;
        let (w, h) = d.dimensions()?;
        (px.len() == w * h * 4).then_some(Rgba { w: w as u32, h: h as u32, px })
    }

    fn png(&self) -> Vec<u8> {
        crate::render::svg::png(self.w, self.h, &self.px)
    }
}

/// Whether a pixel fails: a channel differs by more than [`CHANNEL`].
fn fails(a: &[u8; 4], b: &[u8; 4]) -> bool {
    a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > CHANNEL)
}

/// How `got` differs from `want` beyond the tolerance (a size change, or more than 1 in [`PER_FAILED`] pixels
/// failing), `None` within it.
pub(crate) fn differs(want: &Rgba, got: &Rgba) -> Option<String> {
    if (want.w, want.h) != (got.w, got.h) {
        return Some(format!("size {}x{} -> {}x{}", want.w, want.h, got.w, got.h));
    }
    let failed = want.px.as_chunks::<4>().0.iter().zip(got.px.as_chunks::<4>().0).filter(|&(a, b)| fails(a, b)).count();
    let allowed = (want.w * want.h) as usize / PER_FAILED;
    (failed > allowed).then(|| format!("{failed} pixels differ by more than {CHANNEL} (at most {allowed} may)"))
}

/// Red where a pixel fails, `want` faded elsewhere. Both the same size.
fn diff_image(want: &Rgba, got: &Rgba) -> Rgba {
    let px = want
        .px
        .as_chunks::<4>()
        .0
        .iter()
        .zip(got.px.as_chunks::<4>().0)
        .flat_map(|(a, b)| if fails(a, b) { [255, 0, 0, 255] } else { [a[0] / 4 + 191, a[1] / 4 + 191, a[2] / 4 + 191, 255] })
        .collect();
    Rgba { w: want.w, h: want.h, px }
}

/// The pages a snapshot keeps of a document: every page of those under `fixtures/render/`, else the first two.
fn kept(name: &str, pages: usize) -> usize {
    if name.starts_with("render__") { pages } else { pages.min(2) }
}

/// One page drawn on the CPU at 96 dpi, light appearance, page only, on the document's page colour.
pub(crate) fn draw(g: &mut Graphics, r: &Renderer, page: &crate::layout::fragment::PageFragment, images: &[Vec<u8>]) -> Rgba {
    let picture = g.page_picture(r, page, images);
    let (w, h) = ((page.w * PX_PER_PT).round().max(1.0) as u32, (page.h * PX_PER_PT).round().max(1.0) as u32);
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("a page-sized target");
    // Over the opaque page colour, so the pixels are opaque: premultiplied is straight.
    g.render_picture(&mut target, &picture, Affine::scale(PX_PER_PT), r.ink_for(false, r.page_colour.unwrap_or(0xFF_FFFF)));
    Rgba { w, h, px }
}

/// Every corpus document's kept pages, by snapshot name (`<fixture>__p<N>.png`).
pub(crate) fn rendered() -> BTreeMap<String, Rgba> {
    let fonts = bundled_fonts();
    let mut out = BTreeMap::new();
    for (name, d) in corpus() {
        let snap = crate::layout::layout(&d, &fonts);
        let r = Renderer { page_colour: d.page_colour, ..Renderer::default() };
        let mut g = Graphics::default();
        g.set_fonts(&fonts);
        for (i, page) in snap.pages().iter().take(kept(&name, snap.pages().len())).enumerate() {
            out.insert(format!("{name}__p{}.png", i + 1), draw(&mut g, &r, page, &d.images));
        }
    }
    out
}

/// `target/render-diff`, which CI uploads when the check fails.
fn diff_root() -> PathBuf {
    crate_dir().join("../../target/render-diff")
}

/// `target/render-diff-gpu`: the GPU comparison's failures (before: the CPU, after: the GPU).
#[cfg(twip_window)]
fn gpu_diff_root() -> PathBuf {
    crate_dir().join("../../target/render-diff-gpu")
}

/// The snapshot, this render and their difference as `target/render-diff/<page>.{before,after,diff}.png`, and the
/// report's entry for them.
fn write_diff(root: &std::path::Path, name: &str, want: Option<&Rgba>, got: &Rgba, why: &str) -> String {
    let stem = name.trim_end_matches(".png");
    let mut entry = format!("<h2>{}</h2>\n<p>{}</p>\n<div>", escape(name), escape(why));
    let mut put = |kind: &str, img: &Rgba| {
        let file = format!("{stem}.{kind}.png");
        std::fs::write(root.join(&file), img.png()).unwrap();
        write!(entry, "<figure><img src=\"{file}\"><figcaption>{kind}</figcaption></figure>").unwrap();
    };
    if let Some(want) = want {
        put("before", want);
    }
    put("after", got);
    if let Some(want) = want.filter(|w| (w.w, w.h) == (got.w, got.h)) {
        put("diff", &diff_image(want, got));
    }
    entry + "</div>\n"
}

/// Every corpus document's kept pages equal their committed snapshots in `tests/render`, within the tolerance.
/// `TWIP_WRITE_RENDER=1` rewrites them.
#[test]
fn render_output_matches_snapshots() {
    let (dir, write) = (root().join("render"), std::env::var_os("TWIP_WRITE_RENDER").is_some());
    let got = rendered();
    assert!(!got.is_empty(), "no fixtures read from {}", root().join("fixtures").display());
    if write {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, img) in &got {
            std::fs::write(dir.join(name), img.png()).unwrap();
        }
        return;
    }
    // The WebAssembly run reports the failures without drawing them.
    let report = !cfg!(target_family = "wasm");
    if report {
        let _ = std::fs::remove_dir_all(diff_root());
        std::fs::create_dir_all(diff_root()).unwrap();
    }
    let (mut failures, mut index) = (Vec::new(), String::new());
    for (name, img) in &got {
        let want = std::fs::read(dir.join(name)).ok().and_then(|b| Rgba::decode(&b));
        let why = match &want {
            Some(want) => differs(want, img),
            None => Some("no snapshot".into()),
        };
        if let Some(why) = why {
            if report {
                index += &write_diff(&diff_root(), name, want.as_ref(), img, &why);
            }
            failures.push(format!("{name}: {why}"));
        }
    }
    for e in entries(&dir) {
        let name = e.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if !got.contains_key(&name) {
            failures.push(format!("{name}: snapshot without a page"));
        }
    }
    if !index.is_empty() {
        let page = format!(
            "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Render changes</title>\n<style>body{{font-family:sans-serif}}div{{display:flex;gap:8px}}figure{{margin:0}}img{{border:1px solid #ccc;max-width:32vw}}</style>\n<h1>Render changes</h1>\n<p>Before: the snapshot. After: this render. Diff: red where a channel differs by more than {CHANNEL}.</p>\n{index}"
        );
        std::fs::write(diff_root().join("index.html"), page).unwrap();
    }
    assert!(
        failures.is_empty(),
        "render output changed for {} page(s); see {} (in CI: the render-diff artifact). If intended, run `TWIP_WRITE_RENDER=1 cargo nextest run -p twip --lib render_output` and commit tests/render:\n{}",
        failures.len(),
        diff_root().join("index.html").display(),
        failures.join("\n")
    );
}

/// The `fixtures/render/` pages and the showcase's first drawn on the GPU agree with the CPU's within the same
/// tolerance, and `text-glyphs`' pages exactly. Skips without a GPU adapter, unless `TWIP_GRAPHICS_GPU=1`.
#[cfg(twip_window)]
#[test]
fn gpu_pages_match_the_cpu() {
    use twip_graphics::cpu::{Draw, Frame};
    use twip_graphics::gpu::{Context, Renderer as Gpu, block_on, wgpu};
    let Ok(ctx) = block_on(Context::new(Context::instance(), None)) else {
        assert!(std::env::var_os("TWIP_GRAPHICS_GPU").is_none(), "TWIP_GRAPHICS_GPU is set and there is no GPU adapter");
        return;
    };
    let fonts = bundled_fonts();
    let (mut failures, mut drawn) = (Vec::new(), 0);
    for (name, d) in corpus() {
        let pages = match name.as_str() {
            "docx-feature-showcase" => 1,
            n if n.starts_with("render__") => usize::MAX,
            _ => continue,
        };
        let snap = crate::layout::layout(&d, &fonts);
        let r = Renderer { page_colour: d.page_colour, ..Renderer::default() };
        // One GPU renderer per document's resources.
        let (mut g, mut gpu) = (Graphics::default(), Gpu::new(&ctx, wgpu::TextureFormat::Rgba8Unorm));
        g.set_fonts(&fonts);
        for (i, page) in snap.pages().iter().take(pages).enumerate() {
            let cpu = draw(&mut g, &r, page, &d.images);
            let picture = g.page_picture(&r, page, &d.images);
            let draws = [Draw { picture: &picture, transform: Affine::scale(PX_PER_PT) }];
            let clear = Some(r.ink_for(false, r.page_colour.unwrap_or(0xFF_FFFF)));
            let px = gpu.draw_pixels(&ctx, g.resources(), &Frame { clear, draws: &draws }, (cpu.w, cpu.h)).expect("the device is not lost");
            let ours = Rgba { w: cpu.w, h: cpu.h, px };
            // Text only, drawn from the same glyph masks: byte for byte.
            let why = if name == "render__text-glyphs" {
                (cpu != ours).then(|| "text drawn otherwise than the CPU's glyph masks".to_owned())
            } else {
                differs(&cpu, &ours)
            };
            if let Some(why) = why {
                std::fs::create_dir_all(gpu_diff_root()).unwrap();
                write_diff(&gpu_diff_root(), &format!("{name}__p{}", i + 1), Some(&cpu), &ours, &why);
                failures.push(format!("{name} page {}: {why}", i + 1));
            }
            drawn += 1;
        }
    }
    assert!(drawn > 1, "no pages drawn");
    assert!(
        failures.is_empty(),
        "the GPU's pages differ from the CPU's (before: the CPU, after: the GPU, in {}):\n{}",
        gpu_diff_root().display(),
        failures.join("\n")
    );
}

/// A white page with a grey block, `w` x `h`.
fn page(w: u32, h: u32) -> Rgba {
    let mut px = vec![255u8; (w * h * 4) as usize];
    for y in 10..20 {
        for x in 10..20 {
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 3].fill(128);
        }
    }
    Rgba { w, h, px }
}

#[test]
fn differences_inside_the_tolerance_pass() {
    let want = page(200, 200);
    let mut got = want.clone();
    // Every channel of every pixel off by 2, and 8 pixels (of 40 000; 8 may) off by more.
    got.px.iter_mut().for_each(|c| *c = c.saturating_sub(2));
    for x in 0..8 {
        got.px[x * 4] = 0;
    }
    assert_eq!(differs(&want, &got), None);
}

#[test]
fn a_changed_block_fails() {
    let want = page(200, 200);
    let mut got = want.clone();
    for y in 100..116 {
        for x in 100..116 {
            let i = (y * 200 + x) * 4;
            got.px[i + 1] = 252;
        }
    }
    assert_eq!(differs(&want, &got).as_deref(), Some("256 pixels differ by more than 2 (at most 8 may)"));
    let diff = diff_image(&want, &got);
    assert_eq!(&diff.px[(100 * 200 + 100) * 4..][..4], &[255, 0, 0, 255]);
}

#[test]
fn a_size_change_fails() {
    assert_eq!(differs(&page(200, 200), &page(200, 201)).as_deref(), Some("size 200x200 -> 200x201"));
}

#[test]
fn snapshots_decode_as_written() {
    let want = page(30, 24);
    assert!(Rgba::decode(&want.png()) == Some(want));
    assert!(Rgba::decode(b"\x89PNG\r\n\x1a\nnot a png").is_none());
}
