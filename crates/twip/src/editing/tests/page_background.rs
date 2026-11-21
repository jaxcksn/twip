//! Watermarks and the page colour

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::fonts;
use crate::api::command::{PictureWatermark, TextWatermark, Watermark, WatermarkLayout};
use crate::api::drawing::{ColourTransform, ColourValue, FillValue, GradientShape, GradientStop, ThemeColour, TransformKind};
use crate::api::value::{Bytes, Color, Points};
use crate::docx::Inline;

/// Two sections: the first with no header, the second with its own (`Second`).
fn two_sections() -> Vec<u8> {
    two_sections_with("<w:r><w:t>Second</w:t></w:r>")
}

/// [`two_sections`], the second section's header paragraph holding `runs`.
fn two_sections_with(runs: &str) -> Vec<u8> {
    let blank = crate::docx::blank::blank_docx();
    let read = |name: &str| crate::docx::package::read(&blank, &[], name).expect("reads").expect(name);
    let rels = read("word/_rels/document.xml.rels").replace(
        "</Relationships>",
        r#"<Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
    );
    let types = read("[Content_Types].xml").replace(
        "</Types>",
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
    );
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office""#;
    let header = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {ns}><w:p>{runs}</w:p></w:hdr>"#);
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body><w:p><w:pPr><w:sectPr>{page}</w:sectPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/>{page}</w:sectPr></w:body></w:document>"#
    );
    crate::docx::write::repackage(
        &blank,
        &[
            ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
            ("[Content_Types].xml".into(), types.into_bytes()),
            ("word/header1.xml".into(), header.into_bytes()),
            ("word/document.xml".into(), document.into_bytes()),
        ],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages")
}

fn run(e: &mut Editor, c: Command) {
    let out = e.execute(c.clone(), fonts());
    assert!(matches!(out, Outcome::Changed(_)), "{c:?}: {out:?}");
}

fn refused(e: &mut Editor, c: Command) {
    let out = e.execute(c.clone(), fonts());
    assert!(matches!(out, Outcome::Refused(_)), "{c:?}: {out:?}");
}

/// Each header part's watermarks: its name and how many.
fn marks(e: &Editor) -> Vec<(String, usize)> {
    e.document()
        .stories
        .iter()
        .filter(|(n, _)| n.contains("header"))
        .map(|(n, b)| {
            let count = b
                .iter()
                .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
                .flat_map(|p| p.inlines.iter())
                .filter(|i| matches!(i, Inline::Float(f, _) if crate::docx::watermark::is(f)))
                .count();
            (n.clone(), count)
        })
        .collect()
}

fn part(saved: &[u8], name: &str) -> String {
    crate::docx::package::read(saved, &[], name).expect("reads").unwrap_or_default()
}

fn draft() -> Watermark {
    Watermark::Text(TextWatermark { text: "DRAFT".into(), font: Some("Arial".into()), ..TextWatermark::default() })
}

#[test]
fn a_watermark_goes_in_every_header_of_every_section_and_undoes_whole() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    run(&mut e, Command::SetWatermark(draft()));
    let d = e.document();
    assert!(d.sections[0].headers.iter().all(Option::is_some), "the first section gets each header it lacked");
    assert!(marks(&e).iter().all(|(_, n)| *n == 1), "{:?}", marks(&e));
    assert_eq!(marks(&e).len(), 4, "three new headers and the second section's own");
    let Some(Watermark::Text(t)) = e.page_formatting(0).watermark else { panic!("{:?}", e.page_formatting(0).watermark) };
    assert_eq!(
        (t.text.as_str(), t.font.as_deref(), t.size, t.layout, t.transparency, t.color),
        ("DRAFT", Some("Arial"), None, Some(WatermarkLayout::Diagonal), Some(50.0), Some(Color(0xC0C0C0)))
    );

    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut spids = Vec::new();
    for name in d.stories.keys().filter(|n| n.contains("header")) {
        let xml = part(&saved, name);
        roxmltree::Document::parse(&xml).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(xml.contains(r#"id="PowerPlusWaterMarkObject"#) && xml.contains(r##"type="#_x0000_t136""##), "{xml}");
        assert!(xml.contains("rotation:315") && xml.contains("mso-position-horizontal-relative:margin") && xml.contains(r#"fillcolor="silver""#));
        let k = xml.find("o:spid=\"_x0000_s").expect("a shape number") + 16;
        spids.push(xml[k..].split('"').next().unwrap_or_default().to_string());
    }
    spids.sort();
    spids.dedup();
    assert_eq!(spids.len(), 4, "each shape its own number: {spids:?}");
    assert!(part(&saved, "word/header1.xml").contains("Second"), "the second section's header keeps its text");

    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.page_formatting(1).watermark, e.page_formatting(1).watermark, "read back as written");
    let f = again
        .document()
        .stories
        .values()
        .flat_map(|b| b.iter())
        .find_map(|b| match b {
            crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| if let Inline::Float(f, _) = i { Some(f.clone()) } else { None }),
            _ => None,
        })
        .expect("a watermark float");
    assert!(f.behind && f.h_align.as_deref() == Some("center") && f.v_relative == "margin", "{f:?}");

    run(&mut e, Command::Undo);
    assert!(e.document().sections[0].headers.iter().all(Option::is_none), "the made headers go");
    assert_eq!(marks(&e), vec![("word/header1.xml".to_string(), 0)]);
    assert_eq!(e.page_formatting(1).watermark, None);
    run(&mut e, Command::Redo);
    assert_eq!(marks(&e).len(), 4);
}

#[test]
fn a_watermark_replaces_the_one_there_is_and_is_removed() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    run(&mut e, Command::SetWatermark(draft()));
    let png = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/watermark-picture.docx")).expect("fixture");
    let image = part_bytes(&png, "word/media/image1.png");
    run(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(image), scale: Some(50.0), washout: None })));
    assert!(marks(&e).iter().all(|(_, n)| *n == 1), "one watermark a header: {:?}", marks(&e));
    let Some(Watermark::Picture(p)) = e.page_formatting(0).watermark else { panic!("a picture") };
    assert_eq!((p.scale, p.washout, p.image.0.len()), (Some(50.0), Some(true), 0));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = part(&saved, "word/header1.xml");
    assert!(
        xml.contains("WordPictureWatermark") && xml.contains(r#"gain="19661f" blacklevel="22938f""#) && !xml.contains("PowerPlusWaterMarkObject"),
        "{xml}"
    );
    assert!(part(&saved, "word/_rels/header1.xml.rels").contains("rIdCeImg"), "the picture's relationship");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert!(matches!(again.page_formatting(0).watermark, Some(Watermark::Picture(PictureWatermark { washout: Some(true), .. }))));

    run(&mut e, Command::RemoveWatermark);
    assert!(marks(&e).iter().all(|(_, n)| *n == 0));
    assert_eq!(e.page_formatting(0).watermark, None);
    assert!(matches!(e.execute(Command::RemoveWatermark, fonts()), Outcome::Unchanged(_)));
}

fn part_bytes(package: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(package)).expect("a package");
    let mut f = z.by_name(name).expect("the part");
    let mut out = Vec::new();
    f.read_to_end(&mut out).expect("reads");
    out
}

#[test]
fn word_s_own_watermark_is_reported_and_removed() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/watermark-text.docx")).expect("fixture");
    let mut e = Editor::open(&bytes, fonts()).expect("opens");
    let Some(Watermark::Text(t)) = e.page_formatting(0).watermark else { panic!("DRAFT") };
    assert_eq!((t.text.as_str(), t.font.as_deref(), t.layout), ("DRAFT", Some("Calibri"), Some(WatermarkLayout::Diagonal)));
    let Some(Watermark::Text(h)) = e.page_formatting(1).watermark else { panic!("DRAFT") };
    assert_eq!(h.layout, Some(WatermarkLayout::Horizontal), "the second section's own");
    run(&mut e, Command::RemoveWatermark);
    let saved = crate::docx::write::save(e.document()).expect("saves");
    for n in ["word/header1.xml", "word/header2.xml"] {
        assert!(!part(&saved, n).contains("PowerPlusWaterMarkObject"), "{n}");
    }
}

#[test]
fn watermark_arguments_are_checked_and_its_text_escaped() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let text = |t: &str| Watermark::Text(TextWatermark { text: t.into(), ..TextWatermark::default() });
    refused(&mut e, Command::SetWatermark(text("")));
    refused(&mut e, Command::SetWatermark(text(&"x".repeat(1025))));
    refused(
        &mut e,
        Command::SetWatermark(Watermark::Text(TextWatermark { size: Some(Points(0.0)), ..TextWatermark { text: "A".into(), ..Default::default() } })),
    );
    refused(&mut e, Command::SetWatermark(Watermark::Text(TextWatermark { font: Some("f".repeat(65)), text: "A".into(), ..Default::default() })));
    refused(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(b"not a picture".to_vec()), ..Default::default() })));
    let png = part_bytes(
        &std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/watermark-picture.docx")).unwrap(),
        "word/media/image1.png",
    );
    refused(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(png), scale: Some(0.0), ..Default::default() })));
    assert!(e.document().sections[0].headers.iter().all(Option::is_none), "nothing made");

    // A font name the VML can't hold is refused; an apostrophe it can.
    for font in ["A\";x:y;'B", "A, B", " A", "'A'", "A\\B"] {
        refused(&mut e, Command::SetWatermark(Watermark::Text(TextWatermark { font: Some(font.into()), text: "A".into(), ..Default::default() })));
    }
    let hostile = Watermark::Text(TextWatermark {
        text: "<b>&\"quoted\"\n rIdCeImg5".into(),
        font: Some("O'Brien Sans".into()),
        size: Some(Points(1584.0)),
        layout: Some(WatermarkLayout::Horizontal),
        ..Default::default()
    });
    run(&mut e, Command::SetWatermark(hostile));
    let saved = crate::docx::write::save(e.document()).expect("saves, the text that looks like a picture's id being text");
    let xml = part(&saved, "word/header1.xml");
    roxmltree::Document::parse(&xml).expect("well formed");
    assert!(xml.contains("font-family:&quot;O'Brien Sans&quot;"), "{xml}");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let Some(Watermark::Text(t)) = again.page_formatting(0).watermark else { panic!("a watermark") };
    assert_eq!((t.text.as_str(), t.size, t.font.as_deref()), ("<b>&\"quoted\"  rIdCeImg5", Some(Points(1584.0)), Some("O'Brien Sans")));
    // VML shows no SVG: a picture watermark or page picture of one is refused.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#.to_vec();
    refused(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(svg.clone()), ..Default::default() })));
    refused(&mut e, Command::SetPageColor(FillValue::Picture { image: Bytes(svg), tile: Some(true) }));
}

#[test]
fn a_shape_named_as_a_watermark_without_its_content_is_not_one() {
    let fake = r#"<w:r><w:pict><v:rect id="PowerPlusWaterMarkObject1" style="position:absolute;width:10pt;height:10pt"/></w:pict></w:r>"#;
    let real = crate::docx::watermark::vml(
        &crate::docx::watermark::Watermark::Text(crate::docx::watermark::TextMark {
            text: "DRAFT".into(),
            family: "Arial".into(),
            size: None,
            colour: crate::docx::watermark::SILVER,
            transparency: 50.0,
            diagonal: true,
            bold: false,
            italic: false,
        }),
        "w:",
        [400.0, 100.0],
        2050,
        "",
        "",
    );
    let mut e = Editor::open(&two_sections_with(&format!("{fake}<w:r>{real}</w:r>")), fonts()).expect("opens");
    let Some(Watermark::Text(t)) = e.page_formatting(1).watermark else { panic!("the real one, past the fake") };
    assert_eq!(t.text, "DRAFT");
    let floats = |e: &Editor| {
        e.document()
            .stories
            .values()
            .flatten()
            .filter_map(|b| if let crate::docx::Block::Para(p) = b { Some(p) } else { None })
            .flat_map(|p| p.inlines.iter())
            .filter(|i| matches!(i, Inline::Float(..)))
            .count()
    };
    assert_eq!(floats(&e), 2);
    run(&mut e, Command::RemoveWatermark);
    assert_eq!(floats(&e), 1, "the fake stays");
    assert_eq!(e.page_formatting(1).watermark, None);
}

#[test]
fn a_page_gradient_is_one_its_vml_holds() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let stop = |position: f64, c: u32, alpha: Option<f64>| GradientStop {
        position,
        colour: ColourValue {
            transforms: alpha.map(|a| vec![ColourTransform { kind: TransformKind::Alpha, value: a }]).unwrap_or_default(),
            ..ColourValue::rgb(Color(c))
        },
    };
    let gradient = |kind, angle, stops| FillValue::Gradient { kind, angle, stops, focus: None };
    // A circle, and a translucent stop in a list of colours, aren't.
    refused(&mut e, Command::SetPageColor(gradient(GradientShape::Radial, None, vec![stop(0.0, 0xFF0000, None), stop(1.0, 0xFFFFFF, None)])));
    let clear = vec![stop(0.0, 0xFF0000, None), stop(0.5, 0x00FF00, Some(0.5)), stop(1.0, 0x0000FF, None)];
    refused(&mut e, Command::SetPageColor(gradient(GradientShape::Linear, Some(30.0), clear)));
    // A diagonal runs corner to corner, as VML reads it, to the angle's precision.
    let three = vec![stop(0.0, 0xFF0000, None), stop(0.5, 0x00FF00, None), stop(1.0, 0x0000FF, None)];
    run(&mut e, Command::SetPageColor(gradient(GradientShape::Linear, Some(30.25), three)));
    let Some(crate::drawing::paint::Fill::Gradient(g)) = &e.document().page_fill else { panic!("a gradient") };
    assert!(matches!(g.kind, crate::drawing::paint::GradientKind::Linear { scaled: true, .. }), "{:?}", g.kind);
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.document().page_fill, e.document().page_fill, "{}", part(&saved, "word/document.xml"));
    // One following the outline is rectangular on a page.
    run(&mut e, Command::SetPageColor(gradient(GradientShape::Path, None, vec![stop(0.0, 0xFF0000, None), stop(1.0, 0xFFFFFF, None)])));
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(again.document().page_fill, e.document().page_fill);
}

#[test]
fn a_page_colour_keeps_its_theme_colour_and_undoes() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let accent =
        ColourValue { transforms: vec![ColourTransform { kind: TransformKind::Tint, value: 0.6 }], ..ColourValue::theme(ThemeColour::Accent2) };
    run(&mut e, Command::SetPageColor(FillValue::Solid { colour: accent.clone() }));
    assert!(e.document().settings.display_background_shape && e.document().page_colour.is_some());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let main = part(&saved, "word/document.xml");
    assert!(main.contains(r#"w:themeColor="accent2" w:themeTint="99""#), "{main}");
    assert!(part(&saved, "word/settings.xml").contains("displayBackgroundShape"));
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let Some(FillValue::Solid { colour }) = again.page_formatting(0).background else { panic!("a colour") };
    assert_eq!(colour.theme, Some(ThemeColour::Accent2));
    assert!((colour.transforms[0].value - 0.6).abs() < 0.01, "{colour:?}");
    assert_eq!(again.document().page_colour, e.document().page_colour);

    run(&mut e, Command::Undo);
    assert_eq!((e.document().page_colour, e.page_formatting(0).background), (None, None));
    run(&mut e, Command::Redo);
    run(&mut e, Command::SetPageColor(FillValue::None));
    assert_eq!(e.page_formatting(0).background, None);
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(!part(&saved, "word/document.xml").contains("w:background"));
    let two =
        ColourValue { transforms: vec![ColourTransform { kind: TransformKind::Tint, value: 0.6 }; 2], ..ColourValue::theme(ThemeColour::Accent2) };
    refused(&mut e, Command::SetPageColor(FillValue::Solid { colour: two }));
    refused(&mut e, Command::SetPageColor(FillValue::FromStyle));
}

#[test]
fn a_page_pattern_is_written_with_its_tile_picture_and_read_back_as_its_pattern() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let pattern = FillValue::Pattern {
        preset: "wdUpDiag".into(),
        foreground: ColourValue::rgb(Color(0xFFE699)),
        background: ColourValue::rgb(Color(0x2E75B6)),
    };
    run(&mut e, Command::SetPageColor(pattern.clone()));
    assert_eq!(e.page_formatting(0).background, Some(pattern.clone()));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let main = part(&saved, "word/document.xml");
    assert!(
        main.contains(r##"o:title="Wide upward diagonal" color2="#ffe699" type="pattern""##) && main.contains(r##"fillcolor="#2e75b6""##),
        "{main}"
    );
    assert!(part(&saved, "word/_rels/document.xml.rels").contains("media/pattern1.png"));
    assert!(part(&saved, "[Content_Types].xml").contains("/word/media/pattern1.png"));
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.page_formatting(0).background, Some(pattern));
    refused(
        &mut e,
        Command::SetPageColor(FillValue::Pattern {
            preset: "nonsense".into(),
            foreground: ColourValue::rgb(Color(0)),
            background: ColourValue::rgb(Color(0)),
        }),
    );
}

#[test]
fn a_page_gradient_and_picture_are_written_as_word_writes_them() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let stops = vec![
        GradientStop { position: 0.0, colour: ColourValue::rgb(Color(0xFFFFFF)) },
        GradientStop { position: 1.0, colour: ColourValue::rgb(Color(0x2E75B6)) },
    ];
    run(&mut e, Command::SetPageColor(FillValue::Gradient { kind: GradientShape::Linear, angle: Some(90.0), stops, focus: None }));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/document.xml").contains(r#"type="gradient""#));
    assert_eq!(e.document().page_colour, Some(0xFFFFFF), "the page colour is the first colour");
    let png = part_bytes(
        &std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/watermark-picture.docx")).unwrap(),
        "word/media/image1.png",
    );
    run(&mut e, Command::SetPageColor(FillValue::Picture { image: Bytes(png), tile: Some(true) }));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/document.xml").contains(r#"type="tile""#));
    refused(&mut e, Command::SetPageColor(FillValue::Picture { image: Bytes(b"nope".to_vec()), tile: None }));
    let many = (0..65).map(|k| GradientStop { position: f64::from(k) / 64.0, colour: ColourValue::rgb(Color(0)) }).collect();
    refused(&mut e, Command::SetPageColor(FillValue::Gradient { kind: GradientShape::Linear, angle: None, stops: many, focus: None }));
}

#[test]
fn a_watermark_s_transparency_is_a_percent_read_back_exactly() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let w = Watermark::Text(TextWatermark { text: "COPY".into(), transparency: Some(30.0), ..TextWatermark::default() });
    run(&mut e, Command::SetWatermark(w));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/header1.xml").contains(r#"<v:fill opacity=".7"/>"#));
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let Some(Watermark::Text(t)) = again.page_formatting(0).watermark else { panic!("a watermark") };
    assert_eq!(t.transparency, Some(30.0));
    refused(&mut e, Command::SetWatermark(Watermark::Text(TextWatermark { text: "A".into(), transparency: Some(101.0), ..Default::default() })));
    run(&mut e, Command::SetWatermark(Watermark::Text(TextWatermark { text: "A".into(), transparency: Some(0.0), ..Default::default() })));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(!part(&saved, "word/header1.xml").contains("opacity"), "opaque");
}

#[test]
fn shape_numbers_past_the_largest_start_again_from_word_s_first() {
    let hostile = r#"<w:r><w:pict><v:rect id="r" o:spid="_x0000_s4294967295" style="width:10pt;height:10pt"/></w:pict></w:r><w:r><w:pict><v:rect o:spid="_x0000_s2049" style="width:10pt;height:10pt"/></w:pict></w:r>"#;
    let mut e = Editor::open(&two_sections_with(hostile), fonts()).expect("opens");
    run(&mut e, Command::SetWatermark(draft()));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let mut spids: Vec<String> = e
        .document()
        .stories
        .keys()
        .flat_map(|n| part(&saved, n).match_indices("o:spid=\"").map(|(k, _)| k).collect::<Vec<_>>().into_iter().map(move |k| (n.clone(), k)))
        .map(|(n, k)| part(&saved, &n)[k + 8..].split('"').next().unwrap_or_default().to_string())
        .collect();
    let n = spids.len();
    spids.sort();
    spids.dedup();
    assert_eq!((n, spids.len()), (6, 6), "every shape its own number: {spids:?}");
    assert!(spids.contains(&"_x0000_s2050".to_string()), "{spids:?}");
}

#[test]
fn an_auto_picture_watermark_reports_no_scale() {
    let png = part_bytes(
        &std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/watermark-picture.docx")).unwrap(),
        "word/media/image1.png",
    );
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    run(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(png), scale: None, washout: Some(false) })));
    let auto = |e: &Editor| match e.page_formatting(0).watermark {
        Some(Watermark::Picture(p)) => (p.scale, p.washout),
        w => panic!("{w:?}"),
    };
    assert_eq!(auto(&e), (None, Some(false)));
    let again = Editor::open(&crate::docx::write::save(e.document()).expect("saves"), fonts()).expect("reopens");
    assert_eq!(auto(&again), (None, Some(false)));
    run(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(Vec::new()), scale: Some(150.0), washout: None })));
    assert_eq!(auto(&e), (Some(150.0), Some(true)), "the picture kept, scaled");
}

#[test]
fn a_page_gradient_s_focus_rectangle_and_one_colour_round_trip() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let stops = vec![
        GradientStop { position: 0.0, colour: ColourValue::rgb(Color(0x2E75B6)) },
        GradientStop { position: 1.0, colour: ColourValue::rgb(Color(0xFFFFFF)) },
    ];
    let focus = Some([0.25, 0.1, 0.5, 0.3]);
    run(&mut e, Command::SetPageColor(FillValue::Gradient { kind: GradientShape::Rectangular, angle: None, stops, focus }));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/document.xml").contains(r#"focusposition="0.25,0.1" focussize="0.25,0.6""#));
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let Some(FillValue::Gradient { focus: read, .. }) = again.page_formatting(0).background else { panic!("a gradient") };
    assert_eq!(read, focus);

    let darker = ColourValue { transforms: vec![ColourTransform { kind: TransformKind::Shade, value: 0.6 }], ..ColourValue::rgb(Color(0x2E75B6)) };
    let one = vec![GradientStop { position: 0.0, colour: ColourValue::rgb(Color(0x2E75B6)) }, GradientStop { position: 1.0, colour: darker }];
    run(&mut e, Command::SetPageColor(FillValue::Gradient { kind: GradientShape::Linear, angle: Some(90.0), stops: one, focus: None }));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    assert!(part(&saved, "word/document.xml").contains(r#"color2="fill darken(153)""#), "{}", part(&saved, "word/document.xml"));
    let again = Editor::open(&saved, fonts()).expect("reopens");
    assert_eq!(again.page_formatting(0).background, e.page_formatting(0).background, "read as made");
}

#[test]
fn a_text_watermark_keeps_its_language_takes_the_body_font_and_a_one_point_size() {
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    let w = TextWatermark { text: "BROUILLON".into(), language: Some("fr-FR".into()), size: Some(Points(1.0)), ..TextWatermark::default() };
    run(&mut e, Command::SetWatermark(Watermark::Text(w.clone())));
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let xml = part(&saved, "word/header1.xml");
    assert!(xml.contains(r#"<w:noProof/><w:lang w:val="fr-FR"/>"#), "{xml}");
    let width: f64 = xml.split("width:").nth(1).and_then(|s| s.split("pt").next()).and_then(|s| s.parse().ok()).expect("a width");
    assert!(width < 40.0, "1 pt is not Auto: {width}");
    let again = Editor::open(&saved, fonts()).expect("reopens");
    let Some(Watermark::Text(t)) = again.page_formatting(0).watermark else { panic!("a watermark") };
    assert_eq!((t.language.as_deref(), t.font.as_deref()), (Some("fr-FR"), Some("Calibri")), "the theme's body font by default");
    refused(&mut e, Command::SetWatermark(Watermark::Text(TextWatermark { language: Some("fr FR".into()), ..w })));
}

/// A committed render snapshot, as two pictures that differ.
fn snapshot_png(page: u8) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/render/render__text-glyphs__p{page}.png")))
        .expect("a snapshot")
}

/// The pictures the sections' headers show, per header part.
fn shown(e: &Editor) -> Vec<(String, Vec<u8>)> {
    let d = e.document();
    d.stories
        .iter()
        .filter(|(n, _)| n.contains("header"))
        .filter_map(|(n, b)| {
            let f = b.iter().find_map(|b| match b {
                crate::docx::Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                    Inline::Float(f, _) if crate::docx::watermark::is(f) => Some(f.clone()),
                    _ => None,
                }),
                _ => None,
            })?;
            let crate::docx::watermark::Watermark::Picture(p) = crate::docx::watermark::of(&f)? else { return None };
            Some((n.clone(), d.images[p.image?].clone()))
        })
        .collect()
}

#[test]
fn a_picture_watermark_kept_through_a_scale_change_is_the_edited_sections() {
    let (a, b) = (snapshot_png(1), snapshot_png(2));
    let mut e = Editor::open(&two_sections(), fonts()).expect("opens");
    run(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(a.clone()), scale: Some(100.0), washout: None })));
    // The second section's header shows another picture.
    let saved = crate::docx::write::save(e.document()).expect("saves");
    let rels = part(&saved, "word/_rels/header1.xml.rels");
    let target = rels.split("Target=\"").nth(1).and_then(|t| t.split('"').next()).expect("a picture target").to_string();
    let swapped = crate::docx::write::repackage(
        &saved,
        &[
            ("word/_rels/header1.xml.rels".into(), rels.replace(&target, "media/second.png").into_bytes()),
            ("word/media/second.png".into(), b.clone()),
        ],
        zip::CompressionMethod::Deflated,
    )
    .expect("repackages");
    let mut e = Editor::open(&swapped, fonts()).expect("reopens");
    let pictures = shown(&e);
    assert!(pictures.iter().any(|(_, d)| *d == a) && pictures.iter().any(|(_, d)| *d == b), "two pictures");
    // Editing the first section's scale keeps its own picture, not the first one found in the document.
    let first = super::para_id(&e, 0);
    e.select_range(first, crate::layout::Unit(0), crate::layout::Unit(0), fonts());
    run(&mut e, Command::SetWatermark(Watermark::Picture(PictureWatermark { image: Bytes(Vec::new()), scale: Some(150.0), washout: None })));
    assert!(shown(&e).iter().all(|(_, d)| *d == a), "{:?}", shown(&e).iter().map(|(n, d)| (n, d.len())).collect::<Vec<_>>());
}

#[test]
fn a_picture_watermark_keeps_its_aspect_at_the_size_limit() {
    let [w, h] = super::super::background::picture_size((720.0, 72.0), Some(500.0), [468.0, 648.0]);
    assert_eq!(w, 1584.0);
    assert!((h - 158.4).abs() < 1e-9, "{h}");
    assert_eq!(super::super::background::picture_size((100.0, 50.0), Some(100.0), [468.0, 648.0]), [100.0, 50.0]);
}
