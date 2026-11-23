//! Editor frames drawn on the CPU: hidden white space, formatting marks and wrap points as painted. Each frame is
//! one CSS and device pixel per point, with a document point at its top.

use crate::editing::Editor;
use crate::editing::tests::fonts;
use crate::editing::view::{FormattingMark, MarkKind, MarkOptions};
use crate::render::graphics::Graphics;
use crate::render::{Frame, Renderer};

const W: usize = 700;
const H: usize = 900;

struct Drawn(Vec<u8>);

impl Drawn {
    /// The pixel at document point (`x`, `y`), `top` the frame's top.
    fn at(&self, top: f64, x: f64, y: f64) -> [u8; 4] {
        let (px, py) = (x.floor() as usize, (y - top).floor() as usize);
        let i = (py.min(H - 1) * W + px.min(W - 1)) * 4;
        [self.0[i], self.0[i + 1], self.0[i + 2], self.0[i + 3]]
    }
}

fn draw(e: &mut Editor, top: f64, marks: Option<Vec<FormattingMark>>) -> Drawn {
    let f = Frame { origin_x: 0.0, origin_y: -top, width: W as f64, height: H as f64, dpr: 1.0, px_per_pt: 1.0, caret: false };
    let visible = Renderer::visible(&e.stack(), &f);
    if visible.end > 0 {
        e.ensure_page(visible.end - 1, fonts());
    }
    let mut view = e.view(visible.clone());
    if let Some(m) = marks {
        view.formatting_marks = m;
    }
    let (r, mut g) = (Renderer::default(), Graphics::default());
    g.draw(&r, &view, &e.document().images, &f);
    let mut px = vec![0u8; W * H * 4];
    let mut target = twip_graphics::cpu::Target::new(&mut px, W as u32, H as u32, W * 4, twip_graphics::cpu::Format::Rgba8).expect("a target");
    g.present(&mut target);
    Drawn(px)
}

/// A header of tall black capitals over two pages: "One", a page break, "Two".
fn headed() -> Vec<u8> {
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
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {ns}><w:p><w:r><w:rPr><w:b/><w:sz w:val="40"/></w:rPr><w:t>HHHHHHHHHHHHHHHHHHHH</w:t></w:r></w:p></w:hdr>"#
    );
    let page = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body><w:p><w:r><w:t>One</w:t></w:r><w:r><w:br w:type="page"/></w:r><w:r><w:t>Two</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/>{page}</w:sectPr></w:body></w:document>"#
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

fn paper(p: [u8; 4]) -> bool {
    p[..3].iter().all(|&v| v >= 250)
}

#[test]
fn hidden_white_space_cuts_each_page_to_its_body_and_draws_the_desk_where_pages_meet() {
    let mut e = Editor::open(&headed(), fonts()).expect("opens");
    e.complete(fonts());
    let stack = e.stack();
    assert_eq!(stack.pages.len(), 2);
    let [x, y, w, _] = stack.pages[1];
    let top = y - 100.0;
    let shown = draw(&mut e, top, None);
    let header = (36..60).any(|dy| (0..200).any(|dx| !paper(shown.at(top, x + 72.0 + dx as f64, y + dy as f64))));
    assert!(header, "page 2's header draws with white space shown");
    e.set_white_space_hidden(true);
    let stack = e.stack();
    let [_, meet, ..] = stack.shown(1).expect("page 2 shows");
    let top = meet - 100.0;
    let hidden = draw(&mut e, top, None);
    let desk = hidden.at(top, x + w + 20.0, meet);
    assert!(!paper(desk), "the desk beside the pages: {desk:?}");
    // The row above the line is page 2's shadow.
    for dy in 2..60 {
        for dx in (0..(w - 144.0) as usize).step_by(2) {
            let p = hidden.at(top, x + 72.0 + dx as f64, meet - dy as f64);
            assert!(paper(p), "page 1's blank body end, {dy} pt above the meeting line, not page 2's header: {p:?}");
        }
    }
    for dx in (0..(w as usize)).step_by(7) {
        assert_eq!(hidden.at(top, x + dx as f64 + 1.0, meet), desk, "the meeting line at {dx}");
    }
}

/// Each mark of `kinds` on its page draws something inside its rect (a label or a glyph at its middle).
fn marks_draw(e: &mut Editor, kinds: &[fn(MarkKind) -> bool]) {
    e.set_mark_options(MarkOptions::ALL);
    e.complete(fonts());
    let stack = e.stack();
    let mut seen = vec![false; kinds.len()];
    for page in 0..stack.pages.len() {
        let marks = e.shown_marks(page..page + 1);
        if !marks.iter().any(|m| kinds.iter().any(|k| k(m.kind))) {
            continue;
        }
        let [px, py, ..] = stack.pages[page];
        let top = py;
        let (off, on) = (draw(e, top, None), draw(e, top, Some(marks.clone())));
        for m in marks.iter().filter(|m| m.page == page) {
            let Some(k) = kinds.iter().position(|k| k(m.kind)) else { continue };
            let [x0, t, x1, b] = m.at;
            let h = b - t;
            // Breaks: their label at the middle, where the dots leave a gap; the rest: their glyph after the start.
            let (a, z) = match m.kind {
                MarkKind::PageBreak | MarkKind::ColumnBreak | MarkKind::SectionBreak(_) => ((x0 + x1) / 2.0 - 3.0, (x0 + x1) / 2.0 + 3.0),
                _ => (x0 - 1.0, x1.max(x0 + h * 0.6) + 1.0),
            };
            let differs = (t as i64..=b as i64).any(|y| {
                ((a.floor() as i64)..=(z.ceil() as i64))
                    .any(|x| off.at(top, px + x as f64, py + y as f64) != on.at(top, px + x as f64, py + y as f64))
            });
            assert!(differs, "{:?} draws at {:?} on page {page}", m.kind, m.at);
            seen[k] = true;
        }
    }
    assert!(seen.iter().all(|s| *s), "every kind found: {seen:?}");
}

#[test]
fn edited_wrap_points_draw_a_red_dashed_polygon_with_black_vertices() {
    use crate::editing::selection::{ObjectRef, Selection};
    let star = crate::layout::tests::shapes::sp(0, 0, 100, 100, "star5", "", "FFFFFF");
    let body = format!("<w:p><w:r><w:t>Text</w:t></w:r>{}</w:p>", crate::layout::tests::shapes::anchor(100, 300, 100, 100, 1, false, 5, &star));
    let mut e = Editor::open(&crate::layout::tests::shapes::bytes(&body).expect("a package"), fonts()).expect("opens");
    e.complete(fonts());
    let f = e.page_geometries().iter().flat_map(|(_, g)| g.stories.iter().filter_map(|s| s.float)).next().expect("a float");
    e.set_caret_for_benchmark(Selection::Object(ObjectRef::Floating(f)));
    assert!(matches!(e.edit_wrap_points(), crate::editing::commands::Outcome::Moved));
    let w = e.wrap_points(0..1).expect("the polygon");
    let [px, py, ..] = e.stack().pages[0];
    let drawn = draw(&mut e, py, None);
    let [x0, y0] = w.points[0];
    let black = |p: [u8; 4]| p[..3].iter().all(|&v| v <= 20);
    let red = |p: [u8; 4]| p[0] >= 200 && p[1] <= 60 && p[2] <= 60;
    assert!(black(drawn.at(py, px + x0, py + y0)), "a vertex: {:?}", drawn.at(py, px + x0, py + y0));
    let [x1, y1] = w.points[1];
    let along: Vec<_> = (8..((y1 - y0) as i64 - 8)).map(|d| drawn.at(py, px + x0 + (x1 - x0) / 2.0, py + y0 + d as f64)).collect();
    assert!(along.iter().any(|p| red(*p)) && along.iter().any(|p| !red(*p)), "dashed red along the edge: {along:?}");
}

#[test]
fn break_labels_cell_and_row_marks_and_special_characters_draw_at_their_marks() {
    let mut e = crate::editing::tests::open("FX-SECTIONS.docx");
    marks_draw(&mut e, &[|k| matches!(k, MarkKind::SectionBreak(_))]);
    let mut e = crate::editing::tests::open("FX-TBL-BASIC.docx");
    marks_draw(&mut e, &[|k| k == MarkKind::CellEnd, |k| k == MarkKind::RowEnd]);
    let body = r#"<w:p><w:r><w:t>a&#160;b&#8209;c</w:t></w:r><w:r><w:br w:type="column"/></w:r><w:r><w:t>d</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:pPr><w:rPr><w:vanish/><w:specVanish/></w:rPr></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p><w:p><w:r><w:t>Body</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&crate::editing::tests::blank_body(body), fonts()).expect("opens");
    marks_draw(
        &mut e,
        &[
            |k| k == MarkKind::NonBreakingSpace,
            |k| k == MarkKind::NonBreakingHyphen,
            |k| k == MarkKind::ColumnBreak,
            |k| k == MarkKind::PageBreak,
            |k| k == MarkKind::HiddenParagraph,
        ],
    );
}
