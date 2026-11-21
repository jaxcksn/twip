//! The PDF writer against the CPU backend: each scene written as a one-page PDF, the page rendered by hayro (a PDF
//! rasteriser) and compared with the CPU backend's drawing, both at 2 pixels per point over white. Two rasterisers'
//! anti-aliasing differ, so a pixel may differ by up to 64 in a channel; beyond that, at most 1% of the pixels.
//! hayro doesn't composite PDF blend modes, so scenes that use them aren't compared. hayro's hinting is off: the
//! embedded subsets carry no hinting programs, so it would auto-hint them, which the CPU backend doesn't. A failure writes
//! `tests/out/<name>.pdf`, `<name>.pdf.png` and `<name>.pdf-diff.png`.

#![cfg(feature = "pdf")]

use std::path::PathBuf;
use std::sync::Arc;

use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};
use twip_graphics::kurbo::{self, Affine, Shape};
use twip_graphics::paint::{BlendMode, Color, Paint};
use twip_graphics::pdf::{ArtifactKind, Element, Heading, Info, Kind, Mark, Node, Page, write};
use twip_graphics::scene::{Filter, Layer, Picture, Recorder};

mod common;

use common::scenes::{self, rect};

const SCALE: f64 = 2.0;

fn page(picture: &Picture, (w, h): (u32, u32)) -> Page {
    Page {
        width: f64::from(w),
        height: f64::from(h),
        parts: vec![(Mark::Artifact(ArtifactKind::Other), picture.clone())],
        links: Vec::new(),
        destinations: Vec::new(),
    }
}

/// The PDF's first page rendered by hayro at [`SCALE`] over white: premultiplied RGBA.
fn rendered(pdf: Vec<u8>) -> (Vec<u8>, (u32, u32)) {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::vello_cpu::color::palette::css::WHITE;
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(pdf)).expect("hayro reads the PDF");
    let pages = pdf.pages();
    let page = pages.first().expect("a page");
    let settings = hayro::PixmapSettings { x_scale: SCALE as f32, y_scale: SCALE as f32, bg_color: WHITE };
    let pixmap = hayro::render(
        page,
        &hayro::RenderCache::new(),
        &InterpreterSettings::default(),
        &hayro::RenderSettings { hinting: false, ..hayro::RenderSettings::default() },
        &settings,
    );
    let size = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    (pixmap.data_as_u8_slice().to_vec(), size)
}

fn cpu(picture: &Picture, resources: &Resources, (w, h): (u32, u32)) -> Vec<u8> {
    let (w, h) = ((f64::from(w) * SCALE) as u32, (f64::from(h) * SCALE) as u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("a buffer that fits");
    let draws = [Draw { picture, transform: Affine::scale(SCALE) }];
    Renderer::new().render(&mut target, resources, &Frame { clear: Some(Color::WHITE), draws: &draws });
    px
}

/// Writes `picture` as a PDF page of `size` points, renders it, and checks it against the CPU backend.
fn same(name: &str, size: (u32, u32), picture: &Picture, resources: &Resources) {
    same_from(name, size, picture, resources, 0, 1);
}

/// [`same`], comparing the rows from `top` (points) down, with `percent` of the pixels allowed past 64.
fn same_from(name: &str, size: (u32, u32), picture: &Picture, resources: &Resources, top: u32, percent: usize) {
    let written = write(resources, &[page(picture, size)], &[], &[], &Info::default()).expect("writes");
    let (ours, got) = rendered(written.bytes.clone());
    let theirs = cpu(picture, resources, size);
    assert_eq!(got, ((f64::from(size.0) * SCALE) as u32, (f64::from(size.1) * SCALE) as u32), "{name}: the page's size");
    let skip = (f64::from(top) * SCALE) as usize * got.0 as usize * 4;
    let (ours, theirs) = (ours[skip..].to_vec(), theirs[skip..].to_vec());
    let got = (got.0, got.1 - (f64::from(top) * SCALE) as u32);
    let mut over = 0usize;
    let mut worst = 0u8;
    let diff: Vec<u8> = ours
        .chunks(4)
        .zip(theirs.chunks(4))
        .flat_map(|(a, b)| {
            let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            worst = worst.max(d);
            over += usize::from(d > 64);
            if d > 64 { [255, 0, 0, 255] } else { [b[0] / 2 + 127, b[1] / 2 + 127, b[2] / 2 + 127, 255] }
        })
        .collect();
    let pixels = (got.0 * got.1) as usize;
    eprintln!("{name}: {over} pixels over 64 (of {pixels}), worst {worst}, {} bytes", written.bytes.len());
    if over * 100 > pixels * percent || std::env::var_os("TWIP_GRAPHICS_DIFF").is_some() {
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("out");
        std::fs::create_dir_all(&out).expect("the output folder");
        std::fs::write(out.join(format!("{name}.pdf")), &written.bytes).expect("the PDF");
        let png = |file: String, px: &[u8]| {
            let f = std::io::BufWriter::new(std::fs::File::create(out.join(file)).expect("a file"));
            let mut e = png::Encoder::new(f, got.0, got.1);
            e.set_color(png::ColorType::Rgba);
            e.write_header().expect("a header").write_image_data(px).expect("pixels");
        };
        png(format!("{name}.pdf.png"), &ours);
        png(format!("{name}.pdf-diff.png"), &diff);
    }
    assert!(over * 100 <= pixels * percent, "{name}: {over} pixels differ by more than 64");
}

#[test]
fn rects() {
    same("rects", (128, 96), &scenes::rects(), &Resources::default());
}

#[test]
fn rrects() {
    same("rrects", (128, 96), &scenes::rrects(), &Resources::default());
}

#[test]
fn paths() {
    same("paths", (128, 112), &scenes::paths(), &Resources::default());
}

#[test]
fn fill_rules() {
    same("fill-rules", (128, 168), &scenes::fill_rules(), &Resources::default());
}

#[test]
fn strokes() {
    same("strokes", (128, 128), &scenes::strokes(), &Resources::default());
}

#[test]
fn clips() {
    same("clips", (128, 120), &scenes::clips(), &Resources::default());
}

/// Raster operations read the destination, which PDF can't: the writer draws them as a raster of the group.
#[test]
fn raster_ops() {
    same("raster-ops", (162, 162), &scenes::raster_ops(), &Resources::default());
}

#[test]
fn transforms() {
    same("transforms", (128, 128), &scenes::transforms(), &Resources::default());
}

#[test]
fn text() {
    let mut res = Resources::default();
    let picture = scenes::text(&mut res);
    same("text", (256, 200), &picture, &res);
}

#[test]
fn stroked_text() {
    let mut res = Resources::default();
    let picture = scenes::stroked_text(&mut res);
    same("stroked-text", (256, 200), &picture, &res);
}

#[test]
fn text_effects() {
    let mut res = Resources::default();
    let picture = scenes::text_effects(&mut res);
    same("text-effects", (256, 200), &picture, &res);
}

#[test]
fn turned_text() {
    let mut res = Resources::default();
    let picture = scenes::turned_text(&mut res);
    // 2%: hayro sets the path glyphs a device pixel left of the CPU, and the quarter-turned runs apart (hinted or not).
    same_from("turned-text", (560, 400), &picture, &res, 0, 2);
}

#[test]
fn images() {
    let mut res = Resources::default();
    let picture = scenes::images(&mut res);
    same("images", (128, 144), &picture, &res);
}

#[test]
fn pixel_tiles() {
    let mut res = Resources::default();
    let picture = scenes::pixel_tiles(&mut res);
    // Tiles drawn finer than a pixel a texel are placed by the viewer, snapped or not: up to a tenth of them differs.
    same_from("pixel-tiles", (128, 96), &picture, &res, 0, 10);
}

#[test]
fn gradients() {
    same("gradients", (128, 144), &scenes::gradients(), &Resources::default());
}

#[test]
fn layers() {
    same("layers", (128, 144), &scenes::layers(), &Resources::default());
}

/// The CFF outlines' row only, allowing 2%: the CPU backend's colour glyphs are drawn into the PDF as images by the
/// same backend, but hayro samples images differently; and the CFF glyphs are Type 3 outlines, which hayro fills as
/// paths, without the mask gamma the CPU backend gives glyph coverage (as Skia does), so their stems' edges differ
/// more than text's.
#[test]
fn colour_glyphs() {
    let mut res = Resources::default();
    let picture = scenes::colour_glyphs(&mut res);
    same_from("colour-glyphs", (256, 240), &picture, &res, 205, 2);
}

/// What Twip composites with Porter-Duff modes, which the writer turns into soft masks: a compound line (bands
/// stroked, the gap cleared, the line's paint over them source-in), an inner shadow (a blurred frame with the shape
/// cleared, kept destination-in to the shape), and a soft edge (the shape kept destination-in to its blurred
/// silhouette, its outline cleared).
fn masks() -> Picture {
    let mut rec = Recorder::new();
    let blended = |c: Color, blend: BlendMode| Paint { blend, ..Paint::from(c) };
    // Compound line.
    let line = kurbo::Rect::new(10.0, 10.0, 70.0, 50.0).to_path(0.01);
    rec.push_layer(&Layer::default());
    rec.push_layer(&Layer::default());
    rec.stroke_path(line.elements(), &kurbo::Stroke::new(10.0), Color::BLACK);
    rec.stroke_path(line.elements(), &kurbo::Stroke::new(4.0), blended(Color::BLACK, BlendMode::Clear));
    rec.pop();
    rec.push_layer(&Layer { blend: BlendMode::SrcIn, ..Layer::default() });
    rec.fill_rect(rect(0.0, 0.0, 80.0, 60.0), Color::rgb(0x2060C0));
    rec.pop();
    rec.pop();
    // Inner shadow.
    let shape = kurbo::Circle::new((120.0, 32.0), 24.0).to_path(0.01);
    rec.fill_path(shape.elements(), Color::rgb(0xF0D080));
    rec.push_layer(&Layer::default());
    rec.push_layer(&Layer { filters: &[Filter::Blur { sigma: 3.0 }], ..Layer::default() });
    rec.push_transform(Affine::translate((4.0, 4.0)));
    rec.fill_rect(rect(80.0, -10.0, 160.0, 70.0), Color::rgba(0, 0, 0, 160));
    rec.fill_path(shape.elements(), blended(Color::BLACK, BlendMode::Clear));
    rec.pop();
    rec.pop();
    rec.push_layer(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
    rec.fill_path(shape.elements(), Color::BLACK);
    rec.pop();
    rec.pop();
    // Soft edge.
    let soft = kurbo::Circle::new((60.0, 100.0), 26.0).to_path(0.01);
    rec.push_layer(&Layer::default());
    rec.fill_path(soft.elements(), Color::rgb(0x40A040));
    let area = kurbo::Rect::new(20.0, 60.0, 100.0, 140.0);
    rec.push_layer(&Layer { blend: BlendMode::DstIn, filters: &[Filter::Blur { sigma: 2.0 }], bounds: Some(area), ..Layer::default() });
    rec.fill_path(soft.elements(), Color::BLACK);
    rec.stroke_path(soft.elements(), &kurbo::Stroke::new(6.0), blended(Color::BLACK, BlendMode::Clear));
    rec.pop();
    rec.pop();
    rec.finish()
}

#[test]
fn porter_duff_masks() {
    same("masks", (160, 150), &masks(), &Resources::default());
}

/// Tags, bookmarks and metadata are written: a heading and a paragraph as content, a rule as an artifact.
#[test]
fn structure_outline_and_metadata() {
    let mut rec = Recorder::new();
    rec.fill_rect(rect(10.0, 10.0, 90.0, 20.0), Color::BLACK);
    let heading = rec.finish();
    let mut rec = Recorder::new();
    rec.fill_rect(rect(10.0, 30.0, 90.0, 40.0), Color::BLACK);
    let body = rec.finish();
    let mut rec = Recorder::new();
    rec.fill_rect(rect(10.0, 45.0, 90.0, 46.0), Color::BLACK);
    let rule = rec.finish();
    let parts = vec![(Mark::Content(1), heading), (Mark::Content(2), body), (Mark::Artifact(ArtifactKind::Layout), rule)];
    let structure = [Element::new(Kind::Heading(1), vec![Node::Content(1)]), Element::new(Kind::Paragraph, vec![Node::Content(2)])];
    let outline = [Heading { level: 1, content: 1, title: "Title".into() }];
    let info = Info {
        title: "A test".into(),
        language: Some("en-US".into()),
        creator: "Twip".into(),
        producer: "Twip".into(),
        archival: true,
        created: Some(twip_graphics::pdf::Date { year: 2026, month: 11, day: 12, hour: 9, minute: 30, second: 0 }),
        ..Info::default()
    };
    let written = write(
        &Resources::default(),
        &[Page { width: 100.0, height: 60.0, parts, links: Vec::new(), destinations: Vec::new() }],
        &structure,
        &outline,
        &info,
    )
    .expect("writes");
    assert!(written.archival, "PDF/A-2b is claimed");
    let text = String::from_utf8_lossy(&written.bytes);
    // The heading and the paragraph are marked content 0 and 1; the rule, an artifact, is no structure's.
    for key in ["/StructTreeRoot", "/MarkInfo", "/Outlines", "/S/H1/P", "/MCID 0", "/S/P/P", "/MCID 1", "/S/Document", "pdfaid:part", "(A test)"] {
        assert!(text.contains(key), "{key} is written");
    }
    assert!(!text.contains("/MCID 2"), "the artifact isn't tagged");
}

/// Links, notes, header cells and header artifacts: a link's text and its annotation in one element, a named
/// destination, identifiers in the tree, table attributes, and PDF/UA's identifier.
#[test]
fn links_notes_and_table_cells_are_tagged() {
    use twip_graphics::pdf::{Cell, Destination, Link, LinkTarget, Numbering, Scope};
    let square = |y: f64| {
        let mut rec = Recorder::new();
        rec.fill_rect(rect(10.0, y, 90.0, y + 8.0), Color::BLACK);
        rec.finish()
    };
    let parts = vec![
        (Mark::Artifact(ArtifactKind::Header), square(2.0)),
        (Mark::Content(1), square(12.0)),
        (Mark::Content(2), square(22.0)),
        (Mark::Content(3), square(32.0)),
        (Mark::Content(4), square(42.0)),
    ];
    let links = vec![
        Link {
            link: 7,
            rect: kurbo::Rect::new(10.0, 12.0, 50.0, 20.0),
            target: LinkTarget::Uri("https://example.com/ä".into()),
            description: "the report".into(),
        },
        Link {
            link: 8,
            rect: kurbo::Rect::new(10.0, 22.0, 50.0, 30.0),
            target: LinkTarget::Named("results".into()),
            description: "the table".into(),
        },
    ];
    let destinations = vec![Destination { name: "results".into(), at: kurbo::Point::new(10.0, 32.0) }];
    let cell = |scope, headers: &[&str], kids| {
        Element::new(Kind::Cell(Cell { scope, headers: headers.iter().map(|h| h.to_string()).collect(), row_span: 1, col_span: 1 }), kids)
    };
    let mut th = cell(Some(Scope::Column), &[], vec![Node::Content(3)]);
    th.id = Some("t1-h1".into());
    let note = Element {
        kind: Kind::Note,
        id: Some("note-1".into()),
        lang: None,
        kids: vec![Node::Element(Element::new(Kind::Paragraph, vec![Node::Content(4)]))],
    };
    let structure = [
        Element::new(
            Kind::Paragraph,
            vec![
                Node::Element(Element::new(Kind::Link, vec![Node::Content(1), Node::Annotation(7)])),
                Node::Element(Element::new(Kind::Link, vec![Node::Content(2), Node::Annotation(8)])),
                Node::Element(note),
            ],
        ),
        Element::new(
            Kind::List(Numbering::Decimal),
            vec![Node::Element(Element::new(
                Kind::ListItem,
                vec![
                    Node::Element(Element::new(Kind::Label, vec![Node::Content(5)])),
                    Node::Element(Element::new(
                        Kind::ListBody,
                        vec![Node::Element(Element { kind: Kind::Span, id: None, lang: Some("fr-FR".into()), kids: vec![Node::Content(6)] })],
                    )),
                ],
            ))],
        ),
        Element::new(
            Kind::Table(Some("Quarterly results".into())),
            vec![
                Node::Element(Element::new(Kind::Row, vec![Node::Element(th)])),
                Node::Element(Element::new(Kind::Row, vec![Node::Element(cell(None, &["t1-h1"], Vec::new()))])),
            ],
        ),
    ];
    let info = Info { title: "Links".into(), language: Some("en-US".into()), archival: true, ..Info::default() };
    let page = Page { width: 100.0, height: 60.0, parts, links, destinations };
    let written = write(&Resources::default(), &[page], &structure, &[], &info).expect("writes");
    let text = String::from_utf8_lossy(&written.bytes);
    for key in [
        "/S/Link",
        "/Type/OBJR",
        "/StructParent 1",
        "/Subtype/Link",
        "/Contents(the report)",
        "/URI(https://example.com/%C3%A4)",
        "/D(results)",
        "/Dests",
        "/S/Note",
        "/ID(note-1)",
        "/IDTree",
        "/Scope/Column",
        "/Headers[(t1-h1)]",
        "/Summary(Quarterly results)",
        "/Tabs/S",
        "<pdfuaid:part>1</pdfuaid:part>",
        "pdfaExtension:schemas",
        "/ParentTreeNextKey 3",
        "/S/Lbl",
        "/ListNumbering/Decimal",
        "/S/Span",
        "/Lang(fr-FR)",
    ] {
        assert!(text.contains(key), "{key} is written");
    }
    // Without PDF/A the metadata still names PDF/UA, without PDF/A's schemas.
    let info = Info { archival: false, ..info };
    let page = Page { width: 100.0, height: 60.0, parts: Vec::new(), links: Vec::new(), destinations: Vec::new() };
    let text = String::from_utf8_lossy(&write(&Resources::default(), &[page], &[], &[], &info).expect("writes").bytes).into_owned();
    assert!(text.contains("<pdfuaid:part>1</pdfuaid:part>") && !text.contains("<pdfaid:part>") && !text.contains("<pdfaExtension:schemas>"));
}

/// A run that draws .notdef shows its other glyphs as text and its .notdef glyph as an outline, all in the run's
/// ActualText: no text operator shows glyph 0.
#[test]
fn notdef_glyphs_are_outlines() {
    use twip_graphics::scene::{GlyphRun, Synthesis};
    let mut res = Resources::default();
    let mono = scenes::Mono::new();
    let font = res.fonts.add(mono.data.clone(), 0, &[]).expect("the test face registers");
    let text = "A\u{E000}B";
    let glyphs = mono.set(text, 20.0, 4.0);
    assert_eq!(glyphs[1].0, 0, "the private use character has no glyph");
    let mut rec = Recorder::new();
    let run = GlyphRun { font, size: 20.0, baseline: 30.0, glyphs: &glyphs, synthesis: Synthesis::default() };
    rec.glyphs_text(&res.fonts, &run, text, &[0, 1, 4], Color::BLACK);
    let picture = rec.finish();
    same("notdef", (80, 40), &picture, &res);
    let pdf = write(&res, &[page(&picture, (80, 40))], &[], &[], &Info::default()).expect("writes").bytes;
    // The page's content: the stream with the run's ActualText.
    let mut content = Vec::new();
    let mut at = 0;
    while let Some(k) = pdf[at..].windows(7).position(|w| w == b"stream\n") {
        let start = at + k + 7;
        let end = start + pdf[start..].windows(9).position(|w| w == b"endstream").expect("an end");
        let s = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).unwrap_or_default();
        if s.windows(11).any(|w| w == b"/ActualText") {
            content = s;
        }
        at = end + 9;
    }
    assert!(!content.windows(4).any(|w| w == b"(\0\0)"), "glyph 0 isn't shown");
    let content = String::from_utf8_lossy(&content);
    assert_eq!(content.matches("/ActualText").count(), 1, "{content}");
    let (span, text, fill, end) = (content.find("/ActualText"), content.find(" TJ"), content.find("\nf\n"), content.rfind("EMC"));
    assert!(span < text && text < fill && fill < end, "the text, then the .notdef outline, in the span: {content}");
}
