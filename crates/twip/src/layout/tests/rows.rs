//! A row taller than two pages splits into parts that together show its content once, under a repeated header row
//! (TABLE-40 to TABLE-46).

use std::collections::HashMap;

use crate::docx::model::next_para_id;
use crate::docx::{Block, Inline};
use crate::layout::fragment::{Child, PageChild, PageFragment, RowFragment};
use crate::layout::ids::{ParaId, TableId};
use crate::layout::layout;
use crate::layout::traverse::semantic_chars;

use super::{fixture, fonts};

/// The table's row fragments on a page.
fn rows_of(page: &PageFragment, table: TableId) -> Vec<RowFragment> {
    fn walk(children: &[Child], table: TableId, out: &mut Vec<RowFragment>) {
        for c in children {
            match c {
                Child::Table(t) if t.table == table => out.extend(t.rows.iter().cloned()),
                Child::Box(b) => walk(&b.children, table, out),
                Child::Group { children, .. } => walk(children, table, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for c in page.children.iter() {
        if let PageChild::Body(b) = c {
            walk(&b.children, table, &mut out);
        }
    }
    out
}

#[test]
fn row_split_three_pages() {
    let Some(mut doc) = fixture(r"T1\T1a_tiny.docx") else { return };
    doc.sections.truncate(1);
    let Some(Block::Table(mut t)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else { panic!("T1a has a table") };
    if t.rows.len() < 2 {
        let r = t.rows[0].clone();
        t.rows.push(r);
    }
    t.rows.truncate(2);
    t.rows[0].header = true;
    t.rows[1].header = false;
    t.rows[1].cant_split = false;
    t.rows[1].height = None;
    let Some(Block::Para(template)) = t.rows[1].cells[0].blocks.iter().find(|b| matches!(b, Block::Para(_))).cloned() else {
        panic!("a cell paragraph")
    };
    let run = template.mark.clone();
    let mut tall = Vec::new();
    t.rows[1].cells[0].blocks = (0..150)
        .map(|i| {
            let mut p = template.clone();
            p.inlines = vec![Inline::Text(format!("Line {i} of the tall cell."), run.clone())];
            p.id = next_para_id();
            p.src = None;
            tall.push(ParaId(p.id));
            Block::Para(p)
        })
        .collect();
    let table = TableId(t.id);
    doc.sections[0].blocks = vec![Block::Table(t)];
    let s = layout(&doc, &fonts());
    let rows: Vec<Vec<RowFragment>> = s.pages().iter().map(|p| rows_of(p, table)).collect();
    let parts = rows.iter().filter(|r| r.iter().any(|f| f.row == 1)).count();
    assert!(parts >= 3, "the tall row is split over at least three pages (got {parts})");
    for (i, page_rows) in rows.iter().enumerate().skip(1).filter(|(_, r)| !r.is_empty()) {
        assert!(page_rows.iter().any(|f| f.row == 0), "the header row repeats on page {}", i + 1);
    }
    for f in rows.iter().flatten() {
        assert!(f.rect[2] > f.rect[0] && f.rect[3] > f.rect[1], "row {} part has a rect", f.row);
        assert!(!f.edges.is_empty(), "row {} part has edges", f.row);
    }
    let mut seen: HashMap<(ParaId, u32), usize> = HashMap::new();
    for p in s.pages() {
        let mut chars = Vec::new();
        semantic_chars(p, &mut chars);
        for (para, unit, _) in chars.into_iter().filter(|c| tall.contains(&c.0)) {
            *seen.entry((para, unit.0)).or_default() += 1;
        }
    }
    let paras: std::collections::BTreeSet<ParaId> = seen.keys().map(|k| k.0).collect();
    assert_eq!(paras.len(), tall.len(), "every tall-cell paragraph is shown");
    assert!(seen.values().all(|&n| n == 1), "every character of the tall cell is shown exactly once");
}

#[test]
fn a_cell_float_counts_in_the_centred_content_unless_the_flag_says_not_to_align() {
    // TABLE-30: two centred cells in a 60pt row, one anchoring a 40pt layoutInCell float at its paragraph's top
    // (compat 12). With doNotVertAlignCellWithSp that cell keeps its text at the top; without it the float's bottom
    // is its content height, so its text sits (60 − 40)/2 = 10pt down.
    let tc = |inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vAlign w:val="center"/></w:tcPr><w:p>{inner}</w:p></w:tc>"#);
    let float = r#"<w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="254000" cy="508000"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let ys = |b: &str, flag: bool| {
        let body = format!(
            r#"<w:tbl><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="1200"/></w:trPr>{}{}</w:tr></w:tbl><w:p/>"#,
            tc("<w:r><w:t>a</w:t></w:r>"),
            tc(&format!("{b}<w:r><w:t>b</w:t></w:r>"))
        );
        let mut doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        if flag {
            doc.settings.compat.insert("doNotVertAlignCellWithSp".into());
        }
        let chars = &glyphs(&doc)[0];
        let y = |c: char| chars.iter().find(|x| x.0 == c).map(|x| x.1).expect("drawn");
        (y('a'), y('b'))
    };
    let (a, b) = ys(float, true);
    let (a0, b0) = ys("", true);
    assert!((a - a0).abs() < 1e-6 && (b0 - a0).abs() < 1e-6, "without the float both are centred: a {a0} b {b0}");
    assert!(a - b > 10.0, "the float's cell is at the top: a {a} b {b}");
    let (_, counted) = ys(float, false);
    assert!((counted - b - 10.0).abs() < 1e-6, "the float counts in the content height: {counted} vs top {b}");
}

#[test]
fn a_right_to_left_table_draws_its_first_cell_on_the_right() {
    // TABLE-23: w:bidiVisual, two 100pt cells, jc absent: right-aligned, the first cell rightmost, each 100pt apart.
    let body = r#"<w:tbl><w:tblPr><w:bidiVisual/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    let s = layout(&doc, &fonts());
    let mut xs = Vec::new();
    crate::layout::traverse::paint(&s.pages()[0], &mut |p| {
        if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
            xs.extend(r.chars.iter().map(|&(c, x, _)| (c, x)));
        }
    });
    let x = |c: char| xs.iter().find(|g| g.0 == c).map(|g| g.1).expect("drawn");
    assert!((x('a') - x('b') - 100.0).abs() < 0.1, "a {} is 100pt right of b {}", x('a'), x('b'));
    let page_w = doc.sections[0].page_w - doc.sections[0].right;
    assert!(page_w - x('a') < 100.0, "the first cell ends at the right margin: a {} margin {page_w}", x('a'));
}

#[test]
fn a_column_a_row_does_not_reach_keeps_its_grid_width() {
    // TABLE-15: grid 1000 × 3; the first row skips column 1 and ends after column 2, so column 3 is an after region
    // of its grid width; the second row skips columns 1-2; the third has three 300-twip cells of text. Column 3 stays
    // 50pt wide instead of shrinking to its text.
    let cell = |w: &str, t: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:trPr><w:gridBefore w:val="1"/><w:wBefore w:w="800" w:type="dxa"/></w:trPr>{}</w:tr><w:tr><w:trPr><w:gridBefore w:val="2"/><w:wBefore w:w="2000" w:type="dxa"/></w:trPr><w:tc><w:p><w:r><w:t>T1R2C1</w:t></w:r></w:p></w:tc></w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>"#,
        cell("2000", "T1R1C1"),
        cell("300", "T1R3C1"),
        cell("300", "T1R3C2"),
        cell("300", "T1R3C3")
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let Some(Block::Table(t)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("a table") };
    let s = layout(&doc, &fonts());
    let rows = rows_of(&s.pages()[0], TableId(t.id));
    let last = rows.last().expect("rows");
    let ex: Vec<f64> = last.edges.iter().map(|e| e.0).collect();
    let w = ex[ex.len() - 1] - ex[ex.len() - 2];
    assert!((w - 50.0).abs() < 0.1, "column 3 is 50pt: {w} ({ex:?})");
}

#[test]
fn a_right_to_left_paragraph_swaps_left_and_right() {
    // JUST-4: w:bidi with jc absent aligns right; with jc right (end), left.
    let x = |ppr: &str| {
        let body = format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>a</w:t></w:r></w:p>"#);
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        let s = layout(&doc, &fonts());
        let mut x = None;
        crate::layout::traverse::paint(&s.pages()[0], &mut |p| {
            if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
                x = x.or(r.chars.iter().find(|g| g.0 == 'a').map(|g| g.1));
            }
        });
        (x.expect("drawn"), doc.sections[0].page_w - doc.sections[0].right)
    };
    let (rtl, margin) = x("<w:bidi/>");
    assert!(margin - rtl < 10.0, "right-aligned: a {rtl} margin {margin}");
    assert_eq!(x(r#"<w:bidi/><w:jc w:val="right"/>"#).0, x("").0, "jc right is left-aligned");
}

/// `src` with its body's first empty paragraph replaced by `body`.
pub(super) fn with_body(src: &[u8], body: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = String::from_utf8(data).unwrap().replacen("<w:p/>", body, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn a_row_whose_extra_space_below_overhangs_moves_whole() {
    // TABLE-44: a one-line row under double spacing, 20pt above the body bottom: its text (13.43pt) fits, its pitch
    // (26.86pt) doesn't, so the cell keeps nothing and the row starts the next page (TABLE-42).
    let body = r#"<w:p><w:pPr><w:spacing w:after="0" w:line="12560" w:lineRule="exact"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0" w:line="480" w:lineRule="auto"/></w:pPr><w:r><w:t>z</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    let page = glyphs(&doc).iter().position(|p| p.iter().any(|c| c.0 == 'z'));
    assert_eq!(page, Some(1), "the row is on the second page");
}

#[test]
fn a_cell_line_whose_extra_space_below_overhangs_goes_on() {
    // TABLE-44: three one-line paragraphs under double spacing (pitch 26.86pt) with 70pt of room: the third line's
    // text ends at 67.14pt, its pitch at 80.57pt, so it continues on the next page and the first two stay.
    let p = |c: char| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="480" w:lineRule="auto"/></w:pPr><w:r><w:t>{c}</w:t></w:r></w:p>"#);
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:after="0" w:line="11560" w:lineRule="exact"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}{}{}</w:tc></w:tr></w:tbl><w:p/>"#,
        p('x'),
        p('y'),
        p('z')
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let pages = glyphs(&doc);
    let page = |ch: char| pages.iter().position(|p| p.iter().any(|c| c.0 == ch));
    assert_eq!((page('x'), page('y'), page('z')), (Some(0), Some(0), Some(1)));
}

#[test]
fn a_merged_region_splits_with_its_last_row() {
    // TABLE-32: a two-row region whose four 20pt lines start 50pt above the body bottom: two stay, the other two go
    // on at the top of the next page in its last row's continuation, beside that row's own cell.
    let p = |c: char| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="400" w:lineRule="exact"/></w:pPr><w:r><w:t>{c}</w:t></w:r></w:p>"#);
    let tc = |merge: &str, inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{merge}</w:tcPr>{inner}</w:tc>"#);
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:after="0" w:line="11960" w:lineRule="exact"/></w:pPr><w:r><w:t>f</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        tc("", &p('a')),
        tc(r#"<w:vMerge w:val="restart"/>"#, &[p('p'), p('q'), p('r'), p('s')].concat()),
        tc("", &p('b')),
        tc("<w:vMerge/>", &p(' '))
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let pages = glyphs(&doc);
    let page = |ch: char| pages.iter().position(|p| p.iter().any(|c| c.0 == ch));
    assert_eq!([page('a'), page('p'), page('q'), page('b')], [Some(0); 4]);
    assert_eq!([page('r'), page('s')], [Some(1); 2]);
}

#[test]
fn an_exact_row_clips_above_its_bottom_margin() {
    // TABLE-34, TABLE-28: a 20pt exact row with a 20pt bottom margin is 40pt tall but clips at 20pt: "b", 15pt below
    // the first line, has its ink below 20pt and is hidden, though it would show above the row's 40pt bottom.
    let p = |before: u32, c: char| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="{before}" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:t>{c}</w:t></w:r></w:p>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:bottom w:w="400" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="400" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}{}</w:tc></w:tr></w:tbl><w:p/>"#,
        p(0, 'a'),
        p(300, 'b')
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let drawn = |ch: char| glyphs(&doc)[0].iter().any(|c| c.0 == ch);
    assert!(drawn('a'), "the first line shows");
    assert!(!drawn('b'), "the second line is clipped at the exact height");
}

/// Each page's drawn characters with their baselines.
pub(super) fn glyphs(doc: &crate::docx::Document) -> Vec<Vec<(char, f64)>> {
    let s = layout(doc, &fonts());
    s.pages()
        .iter()
        .map(|page| {
            let mut out = Vec::new();
            crate::layout::traverse::paint(page, &mut |p| {
                if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
                    out.extend(r.chars.iter().map(|&(c, _, _)| (c, r.baseline)));
                }
            });
            out
        })
        .collect()
}

/// Text boxes in table cells and at their content bottom (FLOAT-10, FLOAT-14).
mod floats {
    use crate::docx::{blank, write};
    use crate::layout::fragment::Item;
    use crate::layout::layout;
    use crate::layout::traverse::paint_items;

    use super::super::fonts;

    /// A one-page document with `body` (namespaces for text boxes declared).
    fn doc(body: &str) -> crate::docx::Document {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes = write::repackage(&blank::blank_docx(), &[("word/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated)
            .expect("repackages");
        crate::docx::read(&bytes).expect("reads")
    }

    /// A paragraph anchoring a 100pt-wide text box of height `h` (pt), no insets, holding `text`.
    fn text_box(wrap: &str, h: f64, text: &str) -> String {
        format!(
            r#"<w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="{}"/>{wrap}<wp:docPr id="1" name="Box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
            (h * 12700.0) as i64
        )
    }

    /// `content` in a one-cell table whose 100pt-tall row aligns it `v_align`.
    fn in_cell(v_align: &str, content: &str) -> String {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000" w:hRule="atLeast"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/><w:vAlign w:val="{v_align}"/></w:tcPr>{content}</w:tc></w:tr></w:tbl><w:p/>"#
        )
    }

    /// The baseline and characters of each glyph run on the first page.
    fn runs(d: &crate::docx::Document) -> Vec<(f64, String)> {
        let s = layout(d, &fonts());
        paint_items(&s.pages()[0])
            .iter()
            .filter_map(|i| if let Item::Glyphs(r) = i { Some((r.baseline, r.chars.iter().map(|c| c.0).collect())) } else { None })
            .collect()
    }

    fn baseline_of(d: &crate::docx::Document, text: &str) -> Option<f64> {
        runs(d).into_iter().find(|(_, t)| t.contains(text)).map(|(b, _)| b)
    }

    #[test]
    fn float_14_a_wrapping_float_in_a_cell_is_drawn() {
        let d = doc(&in_cell("top", &text_box(r#"<wp:wrapTight wrapText="bothSides"/>"#, 40.0, "Boxed")));
        assert!(baseline_of(&d, "Boxed").is_some(), "a wrapTight text box in a cell shows its text");
    }

    #[test]
    fn float_14_vertical_alignment_moves_a_cell_float_with_its_text() {
        // TABLE-30: the 40pt box is the cell's content height, so a 100pt row centres it 30pt down.
        let at = |v: &str| baseline_of(&doc(&in_cell(v, &text_box("<wp:wrapNone/>", 40.0, "Boxed"))), "Boxed").expect("the box text");
        assert!((at("center") - at("top") - 30.0).abs() < 0.01, "centred {} vs top {}", at("center"), at("top"));
    }

    #[test]
    fn float_14_a_wrapping_cell_float_makes_an_auto_row_taller() {
        // A 60pt box in an auto row: square-wrapped, the row ends at its bottom (60pt down); wrapNone, below the
        // paragraph (about 22pt).
        let after = |wrap: &str| {
            let body = format!(
                r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p>"#,
                text_box(wrap, 60.0, "Boxed")
            );
            baseline_of(&doc(&body), "After").expect("the paragraph after the table")
        };
        let (square, none) = (after(r#"<wp:wrapSquare wrapText="bothSides"/>"#), after("<wp:wrapNone/>"));
        assert!(square - none > 30.0, "square {square} vs none {none}");
    }

    #[test]
    fn float_14_cell_text_wraps_a_float_laid_out_in_the_cell() {
        // The paragraph anchoring a 100pt-wide square-wrapped box puts its text right of the box; wrapNone, at the
        // cell's left.
        let x = |wrap: &str| {
            let b = text_box(wrap, 40.0, "Boxed").replacen("<w:p>", "<w:p><w:r><w:t>Cell</w:t></w:r>", 1);
            let s = layout(&doc(&in_cell("top", &b)), &fonts());
            paint_items(&s.pages()[0])
                .iter()
                .find_map(|i| match i {
                    Item::Glyphs(r) if r.chars.iter().map(|c| c.0).collect::<String>().contains("Cell") => Some(r.x),
                    _ => None,
                })
                .expect("the cell text")
        };
        let (square, none) = (x(r#"<wp:wrapSquare wrapText="bothSides"/>"#), x("<wp:wrapNone/>"));
        assert!((square - none - 100.0).abs() < 0.01, "square {square} vs none {none}");
    }

    #[test]
    fn float_14_a_body_level_float_over_the_first_row_moves_the_table_below_it() {
        // Compat ≤ 14: a 40pt square-wrapped box with layoutInCell="0" over the first row puts the table below it; with
        // layoutInCell="1" the row grows instead and its text stays at the top.
        let cell = |in_cell: &str| {
            let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 40.0, "Boxed")
                .replace(r#"layoutInCell="1""#, &format!(r#"layoutInCell="{in_cell}""#));
            let body = format!(
                r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/></w:tcPr>{b}<w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
            );
            baseline_of(&doc(&body), "Cell").expect("the cell text")
        };
        let (outside, inside) = (cell("0"), cell("1"));
        assert!((outside - inside - 40.0).abs() < 0.01, "outside {outside} vs inside {inside}");
    }

    #[test]
    fn float_6_an_anchor_after_a_page_break_is_placed_on_the_next_page() {
        // Compat ≤ 14: the paragraph goes on after its page break, so the 360pt box anchored after the break sits at the
        // top of page 2 and the next paragraph's long word, too wide for either side of it, goes below it.
        let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 60.0, "Boxed")
            .replace(r#"cx="1270000""#, r#"cx="4572000""#)
            .replacen("<wp:posOffset>0</wp:posOffset>", "<wp:posOffset>685800</wp:posOffset>", 1)
            .replacen("<w:p>", r#"<w:p><w:r><w:t>Before</w:t><w:br w:type="page"/></w:r>"#, 1);
        let d = doc(&format!("{b}<w:p><w:r><w:t>After{}</w:t></w:r></w:p>", "x".repeat(60)));
        let s = layout(&d, &fonts());
        let page2: Vec<(f64, String)> = paint_items(&s.pages()[1])
            .iter()
            .filter_map(|i| if let Item::Glyphs(r) = i { Some((r.baseline, r.chars.iter().map(|c| c.0).collect())) } else { None })
            .collect();
        let at = |t: &str| page2.iter().find(|(_, x)| x.contains(t)).map(|(b, _)| *b);
        let (boxed, after) = (at("Boxed").expect("the box on page 2"), at("After").expect("the next paragraph on page 2"));
        assert!(after - boxed > 50.0, "box {boxed} vs after {after}");
    }

    #[test]
    fn float_14_a_body_level_float_over_a_later_row_moves_the_rows_from_there_below_it() {
        // Compat ≤ 14: a 40pt square-wrapped box with layoutInCell="0", 30pt below the first row's top, misses that row
        // but covers the second, which starts at the box's bottom (70pt down); wrapNone, right below the first row.
        let gap = |wrap: &str| {
            let b = text_box(wrap, 40.0, "Boxed")
                .replace(r#"layoutInCell="1""#, r#"layoutInCell="0""#)
                .replacen("<wp:posOffset>0</wp:posOffset></wp:positionV>", "<wp:posOffset>381000</wp:posOffset></wp:positionV>", 1)
                .replacen("<w:p>", "<w:p><w:r><w:t>First</w:t></w:r>", 1);
            let row = |c: &str| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/></w:tcPr>{c}</w:tc></w:tr>"#);
            let body = format!(
                r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid>{}{}</w:tbl><w:p/>"#,
                row(&b),
                row("<w:p><w:r><w:t>Second</w:t></w:r></w:p>")
            );
            let d = doc(&body);
            baseline_of(&d, "Second").expect("the second row") - baseline_of(&d, "First").expect("the first row")
        };
        let (square, none) = (gap(r#"<wp:wrapSquare wrapText="bothSides"/>"#), gap("<wp:wrapNone/>"));
        assert!(square > 69.0 && square < 72.0 && none < 30.0, "square {square} vs none {none}");
    }

    #[test]
    fn float_14_a_line_relative_float_stays_in_its_cell_whatever_layout_in_cell() {
        // Compat ≤ 14: layoutInCell="0" moves a paragraph-relative box out of the cell, so the centred cell doesn't
        // move it; a line-relative one stays in the cell and moves with the text.
        let at = |v: &str, rel: &str| {
            let b = text_box("<wp:wrapNone/>", 40.0, "Boxed")
                .replace(r#"layoutInCell="1""#, r#"layoutInCell="0""#)
                .replace(r#"<wp:positionV relativeFrom="paragraph">"#, &format!(r#"<wp:positionV relativeFrom="{rel}">"#));
            baseline_of(&doc(&in_cell(v, &b)), "Boxed").expect("the box text")
        };
        assert!((at("center", "paragraph") - at("top", "paragraph")).abs() < 0.01, "outside the cell");
        assert!(at("center", "line") - at("top", "line") > 20.0, "line-relative: in the cell");
    }

    #[test]
    fn float_10_a_text_box_drops_glyphs_below_its_content_bottom() {
        // A 4pt-tall box: the letters' ink starts above its bottom, the full stop's below it.
        let d = doc(&text_box("<wp:wrapNone/>", 4.0, "Hk."));
        let shown: String = runs(&d).into_iter().map(|(_, t)| t).collect();
        assert!(shown.contains("Hk") && !shown.contains('.'), "shown {shown:?}");
    }
}

#[test]
fn a_split_rows_continuation_keeps_its_first_paragraphs_space_before() {
    // TABLE-41: a cell's "a" ends page 1 and its next paragraph "b" starts the row's continuation on page 2; b's 12pt
    // space-before stays above it there.
    let y = |before: u32| {
        let body = format!(
            r#"<w:p><w:pPr><w:spacing w:after="0" w:line="12560" w:lineRule="exact"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="{before}" w:after="0"/></w:pPr><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        let pages = glyphs(&doc);
        assert!(pages[0].iter().any(|c| c.0 == 'a'), "a stays on page 1");
        pages[1].iter().find(|c| c.0 == 'b').expect("b on page 2").1
    };
    assert!((y(240) - y(0) - 12.0).abs() < 0.01, "{} {}", y(240), y(0));
}

#[test]
fn a_nested_row_splits_inside_its_cell() {
    // TABLE-41: a nested row's first cell holds "a" and "b", its second "c"; one line of room is left: "a" and "c"
    // stay on page 1 and "b" continues on page 2, where the nested row goes on.
    let cell = |w: u32, paras: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{paras}</w:tc>"#);
    let p = |t: char| format!(r#"<w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let nested = format!(
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell(1500, &format!("{}{}", p('a'), p('b'))),
        cell(1500, &p('c'))
    );
    let body = format!(
        r#"<w:p><w:pPr><w:spacing w:after="0" w:line="12560" w:lineRule="exact"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3200"/></w:tblGrid><w:tr>{}</w:tr></w:tbl><w:p/>"#,
        cell(3200, &format!("{nested}{}", p('d')))
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let page = |ch: char| glyphs(&doc).iter().position(|p| p.iter().any(|c| c.0 == ch));
    assert_eq!([page('a'), page('b'), page('c'), page('d')], [Some(0), Some(1), Some(0), Some(1)]);
}
