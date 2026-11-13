//! Table rows

use std::collections::HashMap;

use crate::docx::model::next_para_id;
use crate::docx::{Block, Inline};
use crate::layout::fragment::{Child, PageChild, PageFragment, RowFragment};
use crate::layout::ids::{ParaId, TableId};
use crate::layout::layout;
use crate::layout::traverse::semantic_chars;

use super::{fixture, fonts};

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

/// Where `c` is first drawn on page 1 of a blank document with `body`, and the text width in twips.
fn glyph_x(body: &str, c: char) -> (f64, i64) {
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    let s = &doc.sections[0];
    let text_w = ((s.page_w - s.left - s.right) * 20.0).round() as i64;
    let mut x = None;
    crate::layout::traverse::paint(&layout(&doc, &fonts()).pages()[0], &mut |p| {
        if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
            x = x.or(r.chars.iter().find(|g| g.0 == c).map(|g| g.1));
        }
    });
    (x.expect("drawn"), text_w)
}

/// The x of the first `c` glyph, laid out in compat `mode` with the given compat flags.
fn glyph_x_in(body: &str, mode: i64, flags: &[&str], c: char) -> f64 {
    let mut doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    doc.settings.compat_mode = mode;
    doc.settings.compat.extend(flags.iter().map(|f| f.to_string()));
    let mut x = None;
    crate::layout::traverse::paint(&layout(&doc, &fonts()).pages()[0], &mut |p| {
        if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
            x = x.or(r.chars.iter().find(|g| g.0 == c).map(|g| g.1));
        }
    });
    x.expect("drawn")
}

fn dxa_cell(w: u32, t: &str) -> String {
    format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#)
}

const MAR: &str = r#"<w:tblCellMar><w:left w:w="100" w:type="dxa"/><w:right w:w="100" w:type="dxa"/></w:tblCellMar>"#;

#[test]
fn compat_15_centres_a_row_with_its_grid_after() {
    let table = |tr: &str, last: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:jc w:val="center"/><w:tblLayout w:type="fixed"/>{MAR}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:trPr>{tr}</w:trPr>{}{}{last}</w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>"#,
            dxa_cell(2000, "a"),
            dxa_cell(2000, "b"),
            dxa_cell(2000, "d"),
            dxa_cell(2000, "e"),
            dxa_cell(1000, "f")
        )
    };
    let after = glyph_x_in(&table(r#"<w:gridAfter w:val="1"/><w:wAfter w:w="1000" w:type="dxa"/>"#, ""), 15, &[], 'a');
    let full = glyph_x_in(&table("", &dxa_cell(1000, "c")), 15, &[], 'a');
    assert!((after - full).abs() < 0.01, "the skipped column is centred with the row: {after} vs {full}");
}

#[test]
fn compat_14_row_by_row_centres_a_short_row_by_its_own_cells() {
    let body = format!(
        r#"<w:tbl><w:tblPr><w:jc w:val="center"/><w:tblLayout w:type="fixed"/>{MAR}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{}{}{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p/>"#,
        dxa_cell(2000, "a"),
        dxa_cell(2000, "x"),
        dxa_cell(2000, "y"),
        dxa_cell(2000, "b")
    );
    let shift = |flags: &[&str]| glyph_x_in(&body, 12, flags, 'b') - glyph_x_in(&body, 12, flags, 'a');
    assert!((shift(&["alignTablesRowByRow"]) - 100.0).abs() < 0.01, "the one-cell row is centred alone");
    assert!(shift(&[]).abs() < 0.01, "without the option every row is centred by the grid");
}

#[test]
fn compat_14_nested_right_row_ends_at_the_cell_edge() {
    let nested = |tbl_jc: &str, tr: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblLayout w:type="fixed"/>{MAR}</w:tblPr><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:tbl><w:tblPr>{tbl_jc}<w:tblLayout w:type="fixed"/>{MAR}</w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr>{tr}</w:trPr>{}</w:tr></w:tbl><w:p/></w:tc></w:tr></w:tbl><w:p/>"#,
            dxa_cell(3000, "n")
        )
    };
    let flags = ["alignTablesRowByRow"];
    let right = r#"<w:jc w:val="right"/>"#;
    // The outer cell's content box starts at the margin and is 5800 wide: the nested row starts 2800 in.
    let table = glyph_x_in(&nested(right, ""), 12, &flags, 'n');
    let row = glyph_x_in(&nested("", right), 12, &flags, 'n');
    assert!((table - (72.0 + 145.0)).abs() < 0.01, "no margin added: {table}");
    assert!((row - table).abs() < 0.01, "a row's own jc lands the same: {row}");
}

#[test]
fn a_centred_merged_region_cut_by_the_page_end_centres_in_its_page_part() {
    let blank = crate::docx::read(&crate::docx::blank::blank_docx()).expect("reads");
    let sec = &blank.sections[0];
    // A spacer leaves room for two and a half 72pt rows above the body bottom.
    let spacer = ((sec.page_h - sec.top - sec.bottom - 180.0) * 20.0).round() as i64;
    let tc = |merge: &str, t: &str| {
        format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vMerge{merge}/><w:vAlign w:val="center"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#
        )
    };
    let tr = |merge: &str, t: &str| format!(r#"<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val="1440"/></w:trPr>{}</w:tr>"#, tc(merge, t));
    let y = |rows: usize| {
        let mut body = format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{spacer}" w:lineRule="exact"/></w:pPr></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}"#,
            tr(r#" w:val="restart""#, "T")
        );
        for _ in 1..rows {
            body += &tr("", "");
        }
        body += "</w:tbl><w:p/>";
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        glyphs(&doc)[0].iter().find(|g| g.0 == 'T').map(|g| g.1).expect("drawn on page 1")
    };
    let (cut, whole) = (y(3), y(2));
    assert!((cut - whole).abs() < 0.01, "the third row goes to page 2; T centres in the two rows on page 1: {cut} vs {whole}");
}

#[test]
fn an_empty_indented_paragraph_doesnt_widen_its_autofit_cell() {
    let table = |ind: &str| {
        format!(
            r#"<w:tbl><w:tblGrid><w:gridCol w:w="500"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="500" w:type="dxa"/></w:tcPr><w:p><w:pPr>{ind}</w:pPr></w:p></w:tc>{}</w:tr></w:tbl><w:p/>"#,
            dxa_cell(2000, "b")
        )
    };
    let x = |ind: &str| glyph_x_in(&table(ind), 12, &[], 'b');
    assert!((x(r#"<w:ind w:left="720"/>"#) - x("")).abs() < 0.01, "the 500 column stays 500");
}

#[test]
fn compat_14_pct_table_in_a_text_box_takes_15_twips_more() {
    let body = |grid: &str, cells: &str| {
        format!(
            r#"<w:p><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1905000" cy="1905000"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1905000" cy="1905000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblLayout w:type="fixed"/>{MAR}</w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr>{cells}</w:tr></w:tbl><w:p/></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
        )
    };
    let two = body(r#"<w:gridCol w:w="1000"/><w:gridCol w:w="1000"/>"#, &(dxa_cell(1000, "p") + &dxa_cell(1000, "q")));
    // Compat 12: the 3000 text width with its 100 margins, 3200, and 15 more for the pct base. Compat 15: 3000.
    let col = |mode: i64| glyph_x_in(&two, mode, &[], 'q') - glyph_x_in(&two, mode, &[], 'p');
    assert!((col(12) - 3215.0 / 40.0).abs() < 0.03, "compat 12: {}", col(12));
    assert!((col(15) - 3000.0 / 40.0).abs() < 0.03, "compat 15: {}", col(15));
}

#[test]
fn a_pct_indent_is_a_share_of_the_text_width() {
    let table = |ind: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/>{ind}</w:tblPr><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="1000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        )
    };
    let (pct, text_w) = glyph_x(&table(r#"<w:tblInd w:w="1000" w:type="pct"/>"#), 'a');
    let (dxa, _) = glyph_x(&table(&format!(r#"<w:tblInd w:w="{}" w:type="dxa"/>"#, 1000 * text_w / 5000)), 'a');
    let (none, _) = glyph_x(&table(""), 'a');
    assert!((pct - dxa).abs() < 0.01 && dxa - none > 80.0, "{pct} {dxa} {none}");
}

#[test]
fn a_pct_skip_is_a_share_of_the_table_width_and_rebuilds_the_grid() {
    let cell = |t: char| format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let table = |before: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:gridBefore w:val="1"/>{before}</w:trPr>{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>"#,
            cell('p'),
            cell('q'),
            cell('a'),
            cell('b'),
            cell('c')
        )
    };
    let x = |before: &str, c: char| glyph_x(&table(before), c).0;
    // 20 % of the 6000 table, not of the text width; the row's two cells share the other 4800, and the other row
    // lays out on that grid: 1200 / 2400 / 2400.
    let pct = r#"<w:wBefore w:w="1000" w:type="pct"/>"#;
    assert!((x(pct, 'p') - x("", 'p') + 40.0).abs() < 0.01 && (x(pct, 'q') - x("", 'q') + 20.0).abs() < 0.01);
    assert!((x(pct, 'b') - x("", 'b') + 40.0).abs() < 0.01 && (x(pct, 'c') - x(pct, 'q')).abs() < 0.01);
}

#[test]
fn a_pct_cell_below_a_pct_skip_keeps_its_share_of_the_table_width() {
    let cell = |w: u32, span: u32, t: char| {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="pct"/><w:gridSpan w:val="{span}"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:gridAfter w:val="1"/><w:wAfter w:w="1000" w:type="pct"/></w:trPr>{}</w:tr><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(4000, 2, 'p'),
        cell(500, 1, 'a'),
        cell(4500, 2, 'b')
    );
    // The skip row rebuilds the grid to 2400 / 2400 / 1200, but the next row's pct cells take 10 % and 90 % of the
    // 6000 table: 600 / 5400, not the 2400 the grid gives the first.
    let (a, b) = (glyph_x(&body, 'a').0, glyph_x(&body, 'b').0);
    assert!((b - a - 30.0).abs() < 0.01, "a {a} b {b}");
}

#[test]
fn a_border_wider_than_its_margin_takes_the_other_sides_margin_first() {
    let right_edge = |border: &str| {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblBorders>{border}</w:tblBorders><w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="200" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        let Some(Block::Table(t)) = doc.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("a table") };
        let s = layout(&doc, &fonts());
        let left = rows_of(&s.pages()[0], TableId(t.id))[0].edges[0].0;
        let mut x = None;
        crate::layout::traverse::paint(&s.pages()[0], &mut |p| {
            if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
                x = x.or(r.chars.iter().find(|g| g.0 == 'a').map(|g| g.1));
            }
        });
        x.expect("drawn") - left
    };
    let (plain, bordered) = (right_edge(""), right_edge(r#"<w:left w:val="single" w:sz="16" w:space="0" w:color="000000"/>"#));
    assert!((bordered - plain - 1.0).abs() < 0.01, "the 2pt left border's half moves the text 1pt: {plain} {bordered}");
}

#[test]
fn a_right_to_left_paragraph_swaps_left_and_right() {
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
    let body = r#"<w:p><w:pPr><w:spacing w:after="0" w:line="12560" w:lineRule="exact"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0" w:line="480" w:lineRule="auto"/></w:pPr><w:r><w:t>z</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    let page = glyphs(&doc).iter().position(|p| p.iter().any(|c| c.0 == 'z'));
    assert_eq!(page, Some(1), "the row is on the second page");
}

#[test]
fn a_cell_line_whose_extra_space_below_overhangs_goes_on() {
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

mod floats {
    use crate::docx::{blank, write};
    use crate::layout::fragment::Item;
    use crate::layout::layout;
    use crate::layout::traverse::paint_items;

    use super::super::fonts;

    fn doc(body: &str) -> crate::docx::Document {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        let bytes = write::repackage(&blank::blank_docx(), &[("word/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated)
            .expect("repackages");
        crate::docx::read(&bytes).expect("reads")
    }

    fn text_box(wrap: &str, h: f64, text: &str) -> String {
        format!(
            r#"<w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="{}"/>{wrap}<wp:docPr id="1" name="Box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
            (h * 12700.0) as i64
        )
    }

    fn in_cell(v_align: &str, content: &str) -> String {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000" w:hRule="atLeast"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/><w:vAlign w:val="{v_align}"/></w:tcPr>{content}</w:tc></w:tr></w:tbl><w:p/>"#
        )
    }

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
    fn float_10_a_text_box_aligns_its_text_down_by_its_anchor() {
        let at = |anchor: &str, legacy: bool| {
            let tb = text_box(r#"<wp:wrapNone/>"#, 100.0, "Boxed").replace("<wps:bodyPr ", &format!(r#"<wps:bodyPr anchor="{anchor}" "#));
            let mut d = doc(&tb);
            if legacy {
                // The option has no effect on layout.
                d.settings.compat_mode = 14;
                d.settings.compat.insert("doNotVertAlignInTxbx".into());
            }
            baseline_of(&d, "Boxed")
        };
        let (Some(t), Some(c), Some(b)) = (at("t", false), at("ctr", false), at("b", false)) else { return };
        assert!(c - t > 30.0 && ((b - t) - 2.0 * (c - t)).abs() < 0.01, "{t} {c} {b}");
        let (Some(t), Some(c)) = (at("t", true), at("ctr", true)) else { return };
        assert!(c - t > 30.0, "{t} {c}");
    }

    #[test]
    fn float_14_a_wrapping_float_in_a_cell_is_drawn() {
        let d = doc(&in_cell("top", &text_box(r#"<wp:wrapTight wrapText="bothSides"/>"#, 40.0, "Boxed")));
        assert!(baseline_of(&d, "Boxed").is_some(), "a wrapTight text box in a cell shows its text");
    }

    #[test]
    fn float_14_vertical_alignment_moves_a_cell_float_with_its_text() {
        let at = |v: &str| baseline_of(&doc(&in_cell(v, &text_box("<wp:wrapNone/>", 40.0, "Boxed"))), "Boxed").expect("the box text");
        assert!((at("center") - at("top") - 30.0).abs() < 0.01, "centred {} vs top {}", at("center"), at("top"));
    }

    #[test]
    fn a_cell_float_counts_for_the_alignment_only_where_it_reaches() {
        let cell = |wrap: &str, x: i64| {
            let float = text_box(wrap, 100.0, "Boxed")
                .replace("<wp:posOffset>0</wp:posOffset></wp:positionH>", &format!("<wp:posOffset>{x}</wp:posOffset></wp:positionH>"))
                .replacen("<w:p>", "<w:p><w:r><w:t>Cell</w:t></w:r>", 1);
            let mut d = doc(&in_cell("center", &float));
            d.settings.compat_mode = 15;
            baseline_of(&d, "Cell").expect("the cell text")
        };
        let square = cell(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 1524000);
        // A wrapNone box right of the cell counts nothing; inside it counts as a square one does.
        let (out, inside) = (cell("<wp:wrapNone/>", 5080000), cell("<wp:wrapNone/>", 1524000));
        assert!(out - square > 20.0 && (inside - square).abs() < 0.01, "{out} {inside} {square}");
        // Compat 15: a tight box whose polygon lies along its top adds nothing.
        let flat = r#"<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>"#;
        assert!((cell(flat, 1524000) - out).abs() < 0.01, "{} {out}", cell(flat, 1524000));
    }

    #[test]
    fn a_margin_relative_wrapping_float_in_a_cell_is_drawn() {
        let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 30.0, "Boxed")
            .replace(r#"<wp:positionV relativeFrom="paragraph">"#, r#"<wp:positionV relativeFrom="margin">"#)
            .replacen("<w:p>", "<w:p><w:r><w:t>Cell</w:t></w:r>", 1);
        assert!(baseline_of(&doc(&in_cell("top", &b)), "Boxed").is_some());
    }

    #[test]
    fn float_14_a_wrapping_cell_float_makes_an_auto_row_taller() {
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
    fn float_14_a_body_level_float_over_the_first_row_takes_the_table_beside_it_when_it_fits_else_below() {
        let cell = |in_cell: &str, w: u32| {
            let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 40.0, "Boxed")
                .replace(r#"layoutInCell="1""#, &format!(r#"layoutInCell="{in_cell}""#));
            let body = format!(
                r#"<w:tbl><w:tblPr><w:tblW w:w="{w}" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="{w}"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{b}<w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
            );
            start_of(&doc(&body), "Cell")
        };
        // The box takes 100pt of the 468pt column: a 250pt table goes beside it, a 420pt one below it.
        let (inside, beside, below) = (cell("1", 5000), cell("0", 5000), cell("0", 8400));
        assert!((beside.0 - inside.0 - 100.0).abs() < 0.01 && (beside.1 - inside.1).abs() < 0.01, "beside {beside:?} vs inside {inside:?}");
        assert!((below.0 - inside.0).abs() < 0.01 && (below.1 - inside.1 - 40.0).abs() < 0.01, "below {below:?} vs inside {inside:?}");
    }

    #[test]
    fn float_6_an_anchor_after_a_page_break_is_placed_on_the_next_page() {
        let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 60.0, "Boxed")
            .replace(r#"cx="1270000""#, r#"cx="4572000""#)
            .replacen("<wp:posOffset>0</wp:posOffset>", "<wp:posOffset>685800</wp:posOffset>", 1)
            .replacen("<w:p>", r#"<w:p><w:r><w:t>Before</w:t><w:br w:type="page"/></w:r>"#, 1);
        let glyphs = |p: &crate::layout::fragment::PageFragment| -> Vec<(f64, String)> {
            paint_items(p)
                .iter()
                .filter_map(|i| if let Item::Glyphs(r) = i { Some((r.baseline, r.chars.iter().map(|c| c.0).collect())) } else { None })
                .collect()
        };
        let at = |runs: &[(f64, String)], t: &str| runs.iter().find(|(_, x)| x.contains(t)).map(|(b, _)| *b);
        let doc_compat = |body: &str, mode: i64| {
            let mut d = doc(body);
            d.settings.compat_mode = mode;
            d
        };
        for compat in [12, 15] {
            let titled = b.replacen("</w:drawing></w:r></w:p>", "</w:drawing></w:r><w:r><w:t>Title</w:t></w:r></w:p>", 1);
            let body = format!("{titled}<w:p><w:r><w:t>After{}</w:t></w:r></w:p>", "x".repeat(60));
            let s = layout(&doc_compat(&body, compat), &fonts());
            let page2 = glyphs(&s.pages()[1]);
            let (boxed, after) = (at(&page2, "Boxed").expect("the box on page 2"), at(&page2, "After").expect("the next paragraph on page 2"));
            assert!(at(&page2, "Title").is_some(), "compat {compat}: the title on page 2");
            assert!(after - boxed > 50.0, "compat {compat}: box {boxed} vs after {after}");
            let s = layout(&doc_compat(&format!("{b}<w:p><w:r><w:t>After</w:t></w:r></w:p>"), compat), &fonts());
            assert!(at(&glyphs(&s.pages()[0]), "Boxed").is_some(), "compat {compat}: the box stays on page 1");
        }
    }

    #[test]
    fn float_14_a_body_level_float_over_a_later_row_moves_the_rows_from_there_below_it() {
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
    fn float_14_in_a_cell_page_is_the_cells_box_and_a_wrapping_box_stays_inside_it() {
        let at = |h_rel: &str, offset: i64, wrap: &str| {
            let b = text_box(wrap, 40.0, "Boxed").replace(
                r#"<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset>"#,
                &format!(r#"<wp:positionH relativeFrom="{h_rel}"><wp:posOffset>{}</wp:posOffset>"#, offset * 12700),
            );
            let margins = r#"<w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr>"#;
            start_of(&doc(&in_cell("top", &b).replacen("</w:tblPr>", margins, 1)), "Boxed").0
        };
        // `page` counts from the cell's edge, 5.4pt (the cell margin) left of `column`.
        let (page, column) = (at("page", 0, "<wp:wrapNone/>"), at("column", 0, "<wp:wrapNone/>"));
        assert!((column - page - 5.4).abs() < 0.01, "page {page} column {column}");
        // A 100pt square box 200pt into the 239.2pt text of a 250pt cell ends at the cell's edge; one without wrap doesn't.
        let square = at("column", 200, r#"<wp:wrapSquare wrapText="bothSides"/>"#);
        assert!((square - page - 150.0).abs() < 0.01, "square {square}");
        assert!((at("column", 200, "<wp:wrapNone/>") - column - 200.0).abs() < 0.01);
    }

    #[test]
    fn a_wrapping_box_in_a_cell_stays_inside_it_from_any_horizontal_reference() {
        let at = |h_rel: &str| {
            let b = text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 40.0, "Boxed").replace(
                r#"<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset>"#,
                &format!(r#"<wp:positionH relativeFrom="{h_rel}"><wp:posOffset>{}</wp:posOffset>"#, 200 * 12700),
            );
            let b = b.replacen("<w:p>", "<w:p><w:r><w:t>Cell text beside the box</w:t></w:r>", 1);
            start_of(&doc(&in_cell("top", &b)), "Boxed").0
        };
        // A 100pt box 200pt into a 250pt cell ends at the cell's right edge, from the margin as from the column.
        assert!((at("margin") - at("column")).abs() < 0.01, "margin {} column {}", at("margin"), at("column"));
    }

    #[test]
    fn compat_14_a_first_row_too_wide_beside_its_tight_page_float_goes_below_it() {
        let first = |wrap: &str| {
            let b = text_box(wrap, 40.0, "Boxed").replace(r#"layoutInCell="1""#, r#"layoutInCell="0""#).replacen(
                "<w:p>",
                "<w:p><w:r><w:t>First</w:t></w:r>",
                1,
            );
            let body = format!(
                r#"<w:tbl><w:tblPr><w:tblW w:w="8000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="8000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="8000" w:type="dxa"/></w:tcPr>{b}</w:tc></w:tr></w:tbl><w:p/>"#
            );
            let mut d = doc(&body);
            d.settings.compat_mode = 14;
            baseline_of(&d, "First").expect("the row")
        };
        let (tight, none) = (first(r#"<wp:wrapTight wrapText="bothSides"/>"#), first("<wp:wrapNone/>"));
        assert!(tight - none > 39.0, "tight {tight} vs none {none}");
    }

    /// The first glyph run holding `text`: its x and baseline.
    fn start_of(d: &crate::docx::Document, text: &str) -> (f64, f64) {
        let s = layout(d, &fonts());
        paint_items(&s.pages()[0])
            .iter()
            .find_map(|i| match i {
                Item::Glyphs(r) if r.chars.iter().map(|c| c.0).collect::<String>().contains(text) => Some((r.x, r.baseline)),
                _ => None,
            })
            .expect("the text")
    }

    /// A one-cell table after a 100pt high box at the column's left, with or without wrapping around it.
    fn after_box(square: bool, tbl_pr: &str, cell: &str, text: &str, compat: i64, options: &[&str]) -> (f64, f64) {
        let wrap = if square { r#"<wp:wrapSquare wrapText="bothSides"/>"# } else { "<wp:wrapNone/>" };
        let body = format!(
            r#"{}<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr>{cell}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#,
            text_box(wrap, 100.0, "Boxed")
        );
        let mut d = doc(&body);
        d.settings.compat_mode = compat;
        for o in options {
            d.settings.compat.insert((*o).into());
        }
        start_of(&d, &text[..4])
    }

    #[test]
    fn an_in_flow_table_goes_beside_a_float_when_its_row_fits_else_below() {
        let fixed = |w: u32, jc: &str| format!(r#"<w:tblW w:w="{w}" w:type="dxa"/><w:tblLayout w:type="fixed"/>{jc}"#);
        for compat in [14, 15] {
            let at = |square, w, jc: &str| after_box(square, &fixed(w, jc), "", "Cell text", compat, &[]);
            let ((x, b), (x0, b0)) = (at(true, 4000, ""), at(false, 4000, ""));
            assert!((x - x0 - 100.0).abs() < 0.05 && (b - b0).abs() < 0.01, "compat {compat}: beside {x} {b} vs {x0} {b0}");
            let ((x, b), (x0, b0)) = (at(true, 4000, r#"<w:jc w:val="center"/>"#), at(false, 4000, r#"<w:jc w:val="center"/>"#));
            assert!((x - x0 - 50.0).abs() < 0.05 && (b - b0).abs() < 0.01, "compat {compat}: centred in the piece {x} vs {x0}");
            let ((x, b), (x0, b0)) = (at(true, 8000, ""), at(false, 8000, ""));
            assert!((x - x0).abs() < 0.05 && b - b0 > 60.0, "compat {compat}: below {x} {b} vs {x0} {b0}");
        }
    }

    #[test]
    fn a_split_row_beside_a_float_goes_on_at_its_first_parts_place() {
        // A table beside a 100pt box whose one row runs onto the next page: the row goes on 100pt in there too.
        let paras: String = (0..70).map(|i| format!("<w:p><w:r><w:t>Line {i}</w:t></w:r></w:p>")).collect();
        let body = format!(
            r#"{}<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{paras}</w:tc></w:tr></w:tbl><w:p/>"#,
            text_box(r#"<wp:wrapSquare wrapText="bothSides"/>"#, 100.0, "Boxed")
        );
        for compat in [14, 15] {
            let mut d = doc(&body);
            d.settings.compat_mode = compat;
            let s = layout(&d, &fonts());
            let x = |page: usize| {
                paint_items(&s.pages()[page])
                    .iter()
                    .find_map(|i| match i {
                        Item::Glyphs(r) if r.chars.first().is_some_and(|c| c.0 == 'L') => Some(r.x),
                        _ => None,
                    })
                    .expect("a cell line")
            };
            assert!(s.pages().len() > 1 && (x(1) - x(0)).abs() < 0.01 && x(0) > 150.0, "compat {compat}: {} {}", x(0), x(1));
        }
    }

    #[test]
    fn compat_14_fits_an_autofit_table_to_the_room_beside_a_float() {
        let long = "Long text that would make the table wider than the room beside the box if it could ".repeat(2);
        let at = |square, compat, options: &[&str]| after_box(square, r#"<w:tblW w:w="0" w:type="auto"/>"#, "", &long, compat, options);
        let ((x, b), (x0, b0)) = (at(true, 14, &[]), at(false, 14, &[]));
        assert!((x - x0 - 100.0).abs() < 0.05 && (b - b0).abs() < 0.01, "beside {x} {b} vs {x0} {b0}");
        for (compat, options) in [(15, &[][..]), (14, &["doNotAutofitConstrainedTables"][..])] {
            let ((x, b), (x0, b0)) = (at(true, compat, options), at(false, compat, options));
            assert!((x - x0).abs() < 0.05 && b - b0 > 60.0, "compat {compat} {options:?}: below {x} {b} vs {x0} {b0}");
        }
    }

    #[test]
    fn a_raw_table_width_leaves_the_indent_out_of_the_fit_beside_a_float() {
        let pr = r#"<w:tblW w:w="7000" w:type="dxa"/><w:tblInd w:w="720" w:type="dxa"/><w:tblLayout w:type="fixed"/>"#;
        let cell = r#"<w:tcW w:w="7000" w:type="dxa"/>"#;
        for compat in [14, 15] {
            let at = |square, options: &[&str]| after_box(square, pr, cell, "Cell text", compat, options);
            let ((x, b), (x0, b0)) = (at(true, &[]), at(false, &[]));
            assert!((x - x0).abs() < 0.05 && b - b0 > 60.0, "compat {compat}: below {x} {b} vs {x0} {b0}");
            let raw = ["layoutRawTableWidth"];
            let ((x, b), (x0, b0)) = (at(true, &raw), at(false, &raw));
            assert!((x - x0 - 100.0).abs() < 0.05 && (b - b0).abs() < 0.01, "compat {compat}: beside {x} {b} vs {x0} {b0}");
        }
    }

    #[test]
    fn float_10_a_text_box_drops_glyphs_below_its_content_bottom() {
        let d = doc(&text_box("<wp:wrapNone/>", 4.0, "Hk."));
        let shown: String = runs(&d).into_iter().map(|(_, t)| t).collect();
        assert!(shown.contains("Hk") && !shown.contains('.'), "shown {shown:?}");
    }
}

#[test]
fn a_split_rows_continuation_keeps_its_first_paragraphs_space_before() {
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
fn compat_12_a_cell_float_laid_out_outside_the_cell_counts_from_the_row_top() {
    // A layoutInCell="0" text box 10pt below the cell's second paragraph: it counts from the row's top, so the first
    // paragraph's height doesn't move it.
    let boxed = r#"<w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>2540000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>127000</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="635000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="1" name="B"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:prstGeom prst="rect"/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Z</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let y = |first: u32| {
        let body = format!(
            r#"<w:tbl><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{first}" w:lineRule="exact"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr>{boxed}<w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let mut doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        doc.settings.compat_mode = 12;
        let page = &glyphs(&doc)[0];
        let at = |ch: char| page.iter().find(|c| c.0 == ch).map(|c| c.1).expect("drawn");
        (at('b'), at('Z'))
    };
    let ((b1, z1), (b2, z2)) = (y(400), y(800));
    assert!((b2 - b1 - 20.0).abs() < 0.01, "the second paragraph moves down: {b1} {b2}");
    assert!((z2 - z1).abs() < 0.01, "the box stays: {z1} {z2}");
}

#[test]
fn a_nested_autofit_table_takes_its_cells_content_width_in_compat_12() {
    // Outer cell 4830 wide (content 4614); the nested cells ask for 4945 and 4946, so the nested columns shrink to the
    // room they get. Compat 12 must not widen that room by the nested table's edge margins.
    let cell = |w: u32, inner: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{inner}</w:tc>"#);
    let p = |t: char| format!(r#"<w:p><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let mar = r#"<w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar>"#;
    let nested = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="0" w:type="dxa"/>{mar}</w:tblPr><w:tblGrid><w:gridCol w:w="3812"/><w:gridCol w:w="3388"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
        cell(4945, &p('a')),
        cell(4946, &p('b'))
    );
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="0" w:type="dxa"/>{mar}</w:tblPr><w:tblGrid><w:gridCol w:w="4830"/></w:tblGrid><w:tr>{}</w:tr></w:tbl><w:p/>"#,
        cell(4830, &format!("{nested}<w:p/>"))
    );
    let col = |mode: i64| {
        let mut doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        doc.settings.compat_mode = mode;
        let mut x = HashMap::new();
        crate::layout::traverse::paint(&layout(&doc, &fonts()).pages()[0], &mut |p| {
            if let crate::layout::fragment::Item::Glyphs(r) = p.absolute() {
                for g in r.chars.iter() {
                    x.entry(g.0).or_insert(g.1);
                }
            }
        });
        x[&'b'] - x[&'a']
    };
    let (c12, c15) = (col(12), col(15));
    assert!((c12 - c15).abs() < 0.01, "first nested column: compat 12 {c12}, compat 15 {c15}");
}

#[test]
fn a_nested_row_splits_inside_its_cell() {
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

#[test]
fn an_art_border_takes_its_art_width_across_rows() {
    let baseline = |val: &str| {
        let b = format!(r#"w:val="{val}" w:sz="12" w:space="0" w:color="000000""#);
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblBorders><w:top {b}/><w:insideH {b}/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        glyphs(&doc)[0].iter().find(|c| c.0 == 'b').map(|c| c.1).expect("b is drawn")
    };
    let d = baseline("apples") - baseline("single");
    assert!((d - 21.0).abs() < 0.01, "{d}");
}

#[test]
fn with_cell_spacing_a_cells_top_border_leaves_the_row_above_alone() {
    let baseline = |top: &str| {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblCellSpacing w:w="100" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:tcBorders>{top}</w:tcBorders></w:tcPr><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        glyphs(&doc)[0].iter().find(|c| c.0 == 'b').map(|c| c.1).expect("b is drawn")
    };
    let d = baseline(r#"<w:top w:val="double" w:sz="12" w:space="0" w:color="FF0000"/>"#) - baseline("");
    assert!((d - 4.5).abs() < 0.01, "{d}");
}

#[test]
fn a_header_row_after_row_0_joins_the_first_row_run() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let styles = format!(
        r#"<w:styles xmlns:w="{w}"><w:style w:type="table" w:styleId="TS"><w:name w:val="TS"/><w:tblPr><w:tblStyleRowBandSize w:val="1"/></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:sz w:val="40"/></w:rPr></w:tblStylePr><w:tblStylePr w:type="band1Horz"><w:rPr><w:sz w:val="28"/></w:rPr></w:tblStylePr></w:style></w:styles>"#
    );
    let row = |pr: &str, t: char| {
        format!(r#"<w:tr>{pr}<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TS"/><w:tblLook w:val="0420"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{}{}{}</w:tbl><w:p/>"#,
        row("", 'a'),
        row("<w:trPr><w:tblHeader/></w:trPr>", 'b'),
        row("", 'c')
    );
    let parts = [("word/styles.xml".to_string(), styles.into_bytes())];
    let src = crate::docx::write::repackage(&with_body(&crate::docx::blank::blank_docx(), &body), &parts, zip::CompressionMethod::Deflated)
        .expect("repackages");
    let doc = crate::docx::read(&src).expect("reads");
    let Some(Block::Table(t)) = doc.sections[0].blocks.first() else { panic!("a table") };
    let size = |ri: usize| match &t.rows[ri].cells[0].blocks[0] {
        Block::Para(p) => p.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.size) } else { None }),
        _ => None,
    };
    assert_eq!([size(0), size(1), size(2)], [Some(20.0), Some(20.0), Some(14.0)]);
}

#[test]
fn a_12pt_normal_gives_way_to_the_table_style_only_without_the_override_setting() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let styles = format!(
        r#"<w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:rPr><w:sz w:val="24"/></w:rPr></w:style><w:style w:type="table" w:default="1" w:styleId="T0"/><w:style w:type="table" w:styleId="T1"><w:basedOn w:val="T0"/><w:rPr><w:rFonts w:ascii="Arial"/></w:rPr></w:style></w:styles>"#
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="T1"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let size = |mode: u32, setting: &str| {
        let settings = format!(
            r#"<w:settings xmlns:w="{w}"><w:compat>{setting}<w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat></w:settings>"#
        );
        let parts = [
            ("word/styles.xml".to_string(), styles.clone().into_bytes()),
            ("word/settings.xml".to_string(), settings.into_bytes()),
            ("word/_rels/document.xml.rels".to_string(), rels.as_bytes().to_vec()),
        ];
        let src = crate::docx::write::repackage(&with_body(&crate::docx::blank::blank_docx(), body), &parts, zip::CompressionMethod::Deflated)
            .expect("repackages");
        let doc = crate::docx::read(&src).expect("reads");
        let Some(Block::Table(t)) = doc.sections[0].blocks.first() else { panic!("a table") };
        let Block::Para(p) = &t.rows[0].cells[0].blocks[0] else { panic!("a paragraph") };
        p.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.size) } else { None })
    };
    let set = |v: u32| {
        format!(
            r#"<w:compatSetting w:name="overrideTableStyleFontSizeAndJustification" w:uri="http://schemas.microsoft.com/office/word" w:val="{v}"/>"#
        )
    };
    assert_eq!([size(12, ""), size(14, ""), size(14, &set(0))], [Some(11.0); 3]);
    assert_eq!([size(12, &set(1)), size(14, &set(1)), size(15, "")], [Some(12.0); 3]);
}

#[test]
fn compat_12_single_borders_give_a_shared_edge_to_the_right_cell() {
    use crate::layout::fragment::Item;
    let body = r#"<w:tbl><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:tcBorders><w:right w:val="single" w:sz="36" w:space="0" w:color="FF0000"/></w:tcBorders></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:tcBorders><w:left w:val="double" w:sz="4" w:space="0" w:color="0000FF"/></w:tcBorders></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>"#;
    let colours = |mode: i64, single: bool| {
        let mut doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
        doc.settings.compat_mode = mode;
        if single {
            doc.settings.compat.insert("useSingleBorderforContiguousCells".into());
        }
        let snap = layout(&doc, &fonts());
        let mut c: Vec<u32> = crate::layout::traverse::paint_items(&snap.pages()[0])
            .into_iter()
            .filter_map(|i| if let Item::Rect { colour, .. } = i { Some(colour) } else { None })
            .filter(|&c| c == 0xFF0000 || c == 0x0000FF)
            .collect();
        c.dedup();
        c
    };
    // The thick red border wins the conflict; with the option the right cell's blue double does, in compat ≤ 14.
    assert_eq!(colours(12, false), [0xFF0000]);
    assert_eq!(colours(12, true), [0x0000FF]);
    assert_eq!(colours(15, true), [0xFF0000]);
}

#[test]
fn a_rows_top_border_takes_its_width_in_whole_twips() {
    let baseline = |sz: u32| {
        let b = format!(r#"w:val="single" w:sz="{sz}" w:space="0" w:color="000000""#);
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblBorders><w:top {b}/><w:insideH {b}/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        glyphs(&doc)[0].iter().find(|c| c.0 == 'b').map(|c| c.1).expect("b is drawn")
    };
    let d = baseline(4) - baseline(3);
    assert!((d - 0.3).abs() < 0.001, "{d}");
}

fn autofit_cell(w: &str, no_wrap: bool, t: &str) -> String {
    let nw = if no_wrap { "<w:noWrap/>" } else { "" };
    format!(r#"<w:tc><w:tcPr>{w}{nw}</w:tcPr><w:p><w:r><w:t xml:space="preserve">{t}</w:t></w:r></w:p></w:tc>"#)
}

#[test]
fn a_no_wrap_cell_keeps_its_preferred_width_over_a_narrower_table_width() {
    let x = |no_wrap: bool| {
        let c = |t: &str| autofit_cell(r#"<w:tcW w:w="3000" w:type="dxa"/>"#, no_wrap, t);
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
            c("a"),
            c("b")
        );
        glyph_x(&body, 'b').0 - glyph_x(&body, 'a').0
    };
    // Without noWrap the dxa table width shrinks both columns; with it each keeps its 3000.
    assert!((x(true) - 150.0).abs() < 0.01, "{}", x(true));
    assert!(x(false) < 140.0, "{}", x(false));
}

#[test]
fn a_no_wrap_auto_cell_is_as_wide_as_its_unwrapped_text() {
    let x = |no_wrap: bool| {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
            autofit_cell("", no_wrap, &super::words(30)),
            autofit_cell("", false, &format!("Q {}", super::words(60)))
        );
        glyph_x(&body, 'Q').0
    };
    // Both cells want more than the column; the noWrap one keeps its whole text width, the other shrinks.
    assert!(x(true) > x(false) + 40.0, "{} {}", x(true), x(false));
}

#[test]
fn compat_15_ignores_grow_autofit_and_squeezes_a_too_wide_table() {
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
        autofit_cell("", false, &"M".repeat(70)),
        autofit_cell("", false, "Qshort")
    );
    // The unbreakable word is wider than the column: compat 15 squeezes the table into it, compat 12 with
    // growAutofit lets it grow.
    let (c15, c12) = (glyph_x_in(&body, 15, &["growAutofit"], 'Q'), glyph_x_in(&body, 12, &["growAutofit"], 'Q'));
    assert!(c15 < 72.0 + 468.0 - 20.0, "{c15}");
    assert!(c12 > 72.0 + 468.0, "{c12}");
}

#[test]
fn a_pct_table_width_does_not_take_off_the_indent() {
    let x = |ind: i32| {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblInd w:w="{ind}" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl><w:p/>"#,
            autofit_cell("", false, "a"),
            autofit_cell("", false, "b")
        );
        glyph_x(&body, 'b').0 - glyph_x(&body, 'a').0
    };
    // The table is the whole column wide however far it is indented: its two equal columns stay equal.
    assert!((x(-1440) - x(0)).abs() < 0.06 && (x(1440) - x(0)).abs() < 0.06, "{} {} {}", x(-1440), x(0), x(1440));
}

#[test]
fn a_one_column_pct_skip_is_a_pct_cell_of_its_column() {
    let cell = |w: u32, span: u32, t: char| {
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="pct"/><w:gridSpan w:val="{span}"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#)
    };
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:gridAfter w:val="1"/><w:wAfter w:w="1000" w:type="pct"/></w:trPr>{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl><w:p/>"#,
        cell(4000, 2, 'p'),
        cell(500, 1, 'a'),
        cell(1500, 1, 'b'),
        cell(3000, 1, 'c')
    );
    // The skip makes the last column pct 1000 under the 3000 cell; the first row's 4000 over the other two raises
    // the second to 3500, and the 5000 cap leaves the last 1000: 600 / 4200 / 1200.
    let (a, b, c) = (glyph_x(&body, 'a').0, glyph_x(&body, 'b').0, glyph_x(&body, 'c').0);
    assert!((b - a - 30.0).abs() < 0.01 && (c - b - 210.0).abs() < 0.01, "a {a} b {b} c {c}");
}

fn exact_rows(n: usize, tbl_borders: &str, last_cells: &str) -> String {
    let rows: String = (1..=n)
        .map(|i| {
            let tc = if i == n { last_cells } else { "" };
            format!(
                r#"<w:tr><w:trPr><w:trHeight w:val="720" w:hRule="exact"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{tc}</w:tcPr><w:p><w:r><w:t>R{i:02}</w:t></w:r></w:p></w:tc></w:tr>"#
            )
        })
        .collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblBorders>{tbl_borders}</w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>{rows}</w:tbl><w:p/>"#
    )
}

fn first_on_page(body: &str, page: usize) -> (String, f64) {
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), body)).expect("reads");
    let chars = &super::page_chars(&doc)[page];
    (chars.iter().take(3).map(|c| c.0).collect(), chars[0].2)
}

#[test]
fn a_row_fits_with_its_own_cells_bottom_border_not_the_last_rows() {
    let b = r#"w:val="single" w:sz="8" w:space="0" w:color="000000""#;
    // 18 rows of 720 fill the 12960-twip body exactly; the last row's 1pt cell bottom border doesn't count for them.
    let body = exact_rows(24, "", &format!("<w:tcBorders><w:bottom {b}/></w:tcBorders>"));
    assert_eq!(first_on_page(&body, 1).0, "R19");
    // A table bottom border counts for every row: row 18 goes on.
    assert_eq!(first_on_page(&exact_rows(24, &format!("<w:bottom {b}/>"), ""), 1).0, "R18");
}

#[test]
fn a_table_continued_on_a_page_takes_its_top_border_above_the_row() {
    let inside = |sz: u32| exact_rows(24, &format!(r#"<w:insideH w:val="single" w:sz="{sz}" w:space="0" w:color="000000"/>"#), "");
    // The row at the top of the second page has the table's top border (none) above it, not the inside one.
    let (thin, thick) = (first_on_page(&inside(4), 1), first_on_page(&inside(24), 1));
    assert_eq!(thin.0, "R19");
    assert!((thin.1 - thick.1).abs() < 0.01, "{} {}", thin.1, thick.1);
}

#[test]
fn every_part_of_a_split_at_least_row_is_at_least_its_height() {
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let next_y = |tr: &str| {
        let fill: String = (0..38).map(|_| line("fill")).collect();
        let cell: String = (1..=6).map(|i| line(&format!("a{i}"))).collect();
        let body = format!(
            r#"{fill}<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr>{tr}</w:trPr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{cell}</w:tc></w:tr><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr></w:tbl><w:p/>"#,
            line("Next")
        );
        let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
        let page = &super::page_chars(&doc)[1];
        page.iter().find(|c| c.0 == 'N').map(|c| c.2).expect("Next on page 2")
    };
    // The continuation holds one 15pt line; with a 50pt atLeast height it is 50pt high, so the next row starts 35pt lower.
    let d = next_y(r#"<w:trHeight w:val="1000" w:hRule="atLeast"/>"#) - next_y("");
    assert!((d - 35.0).abs() < 0.01, "{d}");
}

#[test]
fn a_centred_cells_continuation_starts_at_the_parts_top() {
    let line = |t: &str| {
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="300" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let fill: String = (0..38).map(|_| line("fill")).collect();
    let a: String = (1..=10).map(|i| line(&format!("a{i:02}"))).collect();
    let b: String = (1..=6).map(|i| line(&format!("b{i:02}"))).collect();
    let body = format!(
        r#"{fill}<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{a}</w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:vAlign w:val="center"/></w:tcPr>{b}</w:tc></w:tr></w:tbl><w:p/>"#
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let page = &super::page_chars(&doc)[1];
    let y = |t: &str| {
        let s: String = page.iter().map(|c| c.0).collect();
        let k = s.find(t).expect("on page 2");
        page[s[..k].chars().count()].2
    };
    // The second cell's last line goes on beside the first cell's continued lines, level with the first of them.
    assert!((y("b06") - y("a06")).abs() < 0.01, "{} {}", y("b06"), y("a06"));
}

#[test]
fn centred_cells_align_in_the_rows_box_whatever_their_own_margins() {
    let cell = |mar: &str, t: &str| {
        format!(
            r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{mar}<w:vAlign w:val="center"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#
        )
    };
    let own = r#"<w:tcMar><w:top w:w="0" w:type="dxa"/><w:bottom w:w="300" w:type="dxa"/></w:tcMar>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000" w:hRule="atLeast"/></w:trPr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(own, "X"),
        cell("", "Y")
    );
    let doc = crate::docx::read(&with_body(&crate::docx::blank::blank_docx(), &body)).expect("reads");
    let page = &super::page_chars(&doc)[0];
    let y = |c: char| page.iter().find(|g| g.0 == c).map(|g| g.2).expect("drawn");
    // The 300 bottom margin is the row's below for both cells: the same offset.
    assert!((y('X') - y('Y')).abs() < 0.01, "{} {}", y('X'), y('Y'));
}

#[test]
fn a_table_style_named_table_normal_is_the_built_in_default() {
    let x = |name: &str| {
        let blank = crate::docx::blank::blank_docx();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&blank)).expect("zip");
        let mut styles = String::new();
        std::io::Read::read_to_string(&mut zip.by_name("word/styles.xml").expect("styles"), &mut styles).expect("reads");
        let style = format!(
            r#"<w:style w:type="table" w:styleId="TS"><w:name w:val="{name}"/><w:tblPr><w:tblCellMar><w:left w:w="1000" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#
        );
        let styles = styles.replace("</w:styles>", &style);
        let body = r#"<w:tbl><w:tblPr><w:tblStyle w:val="TS"/><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Q</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
        let bytes = crate::docx::write::repackage(
            &with_body(&blank, body),
            &[("word/styles.xml".into(), styles.into_bytes())],
            zip::CompressionMethod::Deflated,
        )
        .expect("repackages");
        let mut doc = crate::docx::read(&bytes).expect("reads");
        doc.settings.compat_mode = 15;
        super::page_chars(&doc)[0].iter().find(|c| c.0 == 'Q').map(|c| c.1).expect("drawn")
    };
    // Its 1000 left margin isn't used: the built-in's 108 is, as for the other name; an ordinary name keeps it.
    assert!((x("Plain") - x("Table Normal") - 44.6).abs() < 0.01, "{} {}", x("Plain"), x("Table Normal"));
    assert!((x("Normal Table") - x("Table Normal")).abs() < 0.01 && (x("table normal") - x("Plain")).abs() < 0.01);
}
