//! Shown joined parts

use crate::docx::Block;
use crate::layout::cache::LayoutCache;
use crate::layout::env::LayoutEnv;
use crate::layout::fragment::{Child, PageChild};
use crate::layout::ids::ParaId;
use crate::layout::job::{Memos, Meter};
use crate::layout::line::{ParaLines, break_lines_uncached, neighbour_borders};
use crate::layout::space::ParaSpace;
use crate::layout::text::{FieldValues, shape, shape_space};

use super::fonts;

const ARIAL_12: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="24"/>"#;
const HIDDEN: &str = r#"<w:rPr><w:vanish/></w:rPr>"#;

/// `body` read as the editor reads it, hidden text shown; `None` without Arial.
fn doc(body: &str) -> Option<crate::docx::Document> {
    let (mut doc, _) = crate::docx::read_editable(&super::breaking::test_package("", ARIAL_12, body, "")?).expect("reads");
    doc.review.show_hidden = true;
    Some(doc)
}

/// The first paragraph of `body` broken from piece `start` in shown part `part`, `width` points wide.
fn shown(body: &str, (start, part): (usize, usize), width: f64) -> Option<ParaLines> {
    let doc = doc(body)?;
    let Block::Para(p) = &doc.sections[0].blocks[0] else { panic!("a paragraph") };
    let fonts = fonts();
    let env = LayoutEnv::new(&doc, None);
    let (mut cache, mut meter) = (LayoutCache::new(), Meter::unlimited());
    let mut memos = Memos { cache: &mut cache, meter: &mut meter };
    let space = shape_space(p, &FieldValues::default(), &Default::default(), &env.text, None);
    let shaped = shape(p, &space, &fonts, &mut memos).expect("unlimited");
    assert!(shaped.parts.is_some(), "shown parts");
    let ps = ParaSpace {
        width,
        start,
        part,
        borders: neighbour_borders(&p.style, None, None),
        x0: 0.0,
        env: env.text.clone(),
        fit: false,
        letter_break: false,
        shade_gap: 0.0,
    };
    Some(break_lines_uncached(&shaped, p, &ps, &fonts, &mut memos).expect("unlimited"))
}

/// Each line's start, end and part.
fn spans(l: &ParaLines) -> Vec<(usize, usize, usize)> {
    l.lines.iter().map(|x| (x.start, x.end, x.part)).collect()
}

fn para(ppr: &str, text: &str) -> String {
    format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

#[test]
fn an_empty_last_part_takes_a_line() {
    let body = format!(r#"<w:p><w:pPr>{HIDDEN}</w:pPr><w:r><w:t>Alpha</w:t></w:r></w:p><w:p/>"#);
    let Some(l) = shown(&body, (0, 0), 468.0) else { return };
    assert_eq!(spans(&l), [(0, 5, 0), (5, 5, 1)]);
    assert!((l.lines[1].pitch - l.lines[0].pitch).abs() < 0.01, "its own line's height");
}

#[test]
fn a_continuation_starts_in_the_part_it_goes_on_in() {
    let two = format!(r#"<w:p><w:pPr>{HIDDEN}</w:pPr><w:r><w:t>Alpha</w:t></w:r></w:p>{}"#, para("", "Bravo charlie"));
    let Some(l) = shown(&two, (7, 0), 468.0) else { return };
    assert_eq!(spans(&l), [(0, 11, 1)], "no empty line for the part laid out before");
    // An empty part between two: where the line before the break was, before the empty part or after it.
    let ind = r#"<w:ind w:firstLine="720"/>"#;
    let three = format!(r#"<w:p><w:pPr>{HIDDEN}</w:pPr><w:r><w:t>Alpha</w:t></w:r></w:p><w:p><w:pPr>{HIDDEN}</w:pPr></w:p>{}"#, para(ind, "Bravo"));
    let whole = shown(&three, (0, 0), 468.0).expect("Arial");
    assert_eq!(spans(&whole), [(0, 5, 0), (5, 5, 1), (5, 10, 2)]);
    assert_eq!(spans(&shown(&three, (5, 1), 468.0).expect("Arial")), [(0, 0, 1), (0, 5, 2)]);
    let last = shown(&three, (5, 2), 468.0).expect("Arial");
    assert_eq!(spans(&last), [(0, 5, 2)]);
    assert!((last.lines[0].xs[0] - whole.lines[2].xs[0]).abs() < 0.01, "the part's first line keeps its first-line indent");
    // An empty first part laid out before.
    let lead = format!(r#"<w:p><w:pPr>{HIDDEN}</w:pPr></w:p>{}"#, para(ind, "Bravo"));
    let rest = shown(&lead, (0, 1), 468.0).expect("Arial");
    assert_eq!(spans(&rest), [(0, 5, 1)]);
    assert!(rest.lines[0].xs[0] > 35.0, "first-line indent: {:?}", rest.lines[0].xs);
    assert!(!crate::layout::token::At::Piece(0, 1).at_start(), "the paragraph goes on");
}

#[test]
fn a_part_ending_in_a_line_break_has_an_empty_line_of_its_own() {
    let br = |ppr: &str| format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t>Alpha</w:t><w:br/></w:r></w:p>"#);
    let double = r#"<w:spacing w:line="480" w:lineRule="auto"/>"#;
    let body = format!(r#"{}<w:p><w:pPr>{double}</w:pPr><w:r><w:t>Bravo</w:t><w:br/></w:r></w:p>"#, br(HIDDEN));
    let Some(l) = shown(&body, (0, 0), 468.0) else { return };
    assert_eq!(spans(&l), [(0, 6, 0), (6, 6, 0), (6, 12, 1), (12, 12, 1)]);
    assert!(
        (l.lines[3].pitch - l.lines[2].pitch).abs() < 0.01,
        "sized as its part's lines: {:?}",
        l.lines.iter().map(|x| x.pitch).collect::<Vec<_>>()
    );
}

#[test]
fn an_earlier_part_fits_its_lines_by_its_own_pieces() {
    // A text border's box broken across lines takes its sides on each line: only the part's own pieces count.
    let first = format!("<w:pPr>{HIDDEN}</w:pPr><w:r><w:t>{}</w:t></w:r>", "alpha bravo charlie delta ".repeat(3));
    let rest = |rpr: &str| format!(r#"<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t>{}</w:t></w:r></w:p>"#, "echo foxtrot golf ".repeat(12));
    let bdr = r#"<w:bdr w:val="single" w:sz="24" w:space="8" w:color="FF0000"/>"#;
    let starts = |rpr: &str| {
        let l = shown(&format!("<w:p>{first}</w:p>{}", rest(rpr)), (0, 0), 150.0)?;
        Some(l.lines.iter().filter(|x| x.part == 0).map(|x| x.xs.first().copied().unwrap_or(0.0)).collect::<Vec<_>>())
    };
    let Some(plain) = starts("") else { return };
    assert!(plain.len() > 2, "{plain:?}");
    assert_eq!(starts(bdr), Some(plain), "the later part's border doesn't move the first part's lines");
}

/// The colours of `line`'s horizontal edges: those along its top, those along its bottom.
fn edges(line: &crate::layout::line::Line) -> (Vec<u32>, Vec<u32>) {
    let flat: Vec<(f64, u32)> = line.rects.iter().filter(|r| r.0[2] - r.0[0] > 100.0).map(|r| (r.0[1], r.1)).collect();
    let along = |top: bool| flat.iter().filter(|r| (r.0 < 3.0) == top).map(|r| r.1).collect();
    (along(true), along(false))
}

/// How many lines of paragraph `para` `children` hold.
fn lines_of(children: &[Child], para: ParaId) -> usize {
    children
        .iter()
        .map(|c| match c {
            Child::Para(p) if p.para == para => p.lines.len(),
            Child::Box(b) => lines_of(&b.children, para),
            _ => 0,
        })
        .sum()
}

#[test]
fn each_part_draws_its_own_box() {
    let bdr = |c: &str| {
        let e = |s: &str| format!(r#"<w:{s} w:val="single" w:sz="8" w:space="1" w:color="{c}"/>"#);
        format!("<w:pBdr>{}{}{}{}</w:pBdr>", e("top"), e("left"), e("bottom"), e("right"))
    };
    let body = format!("{}{}", para(&format!("{}{HIDDEN}", bdr("FF0000")), "Alpha"), para(&bdr("0000FF"), "Bravo"));
    let Some(l) = shown(&body, (0, 0), 468.0) else { return };
    assert_eq!(spans(&l), [(0, 5, 0), (5, 10, 1)]);
    assert_eq!(edges(&l.lines[0]), (vec![0xFF0000], vec![0xFF0000]), "the red box closes");
    assert_eq!(edges(&l.lines[1]), (vec![0x0000FF], vec![0x0000FF]), "the blue box opens");
    // Boxed alike: one box, the between border at the boundary.
    let between = r#"<w:between w:val="single" w:sz="8" w:space="1" w:color="00FF00"/>"#;
    let alike = bdr("FF0000").replace("</w:pBdr>", &format!("{between}</w:pBdr>"));
    let body = format!("{}{}", para(&format!("{alike}{HIDDEN}"), "Alpha"), para(&alike, "Bravo"));
    let l = shown(&body, (0, 0), 468.0).expect("Arial");
    assert_eq!(edges(&l.lines[0]), (vec![0xFF0000], vec![]), "{:?}", l.lines[0].rects);
    assert_eq!(edges(&l.lines[1]), (vec![0x00FF00], vec![0xFF0000]), "{:?}", l.lines[1].rects);
}

#[test]
fn a_page_break_between_an_empty_part_and_the_next_keeps_each_line_once() {
    // A paragraph of three parts, the middle one empty, each line 20pt, below one that leaves room for `room` lines.
    let exact = r#"<w:spacing w:before="0" w:after="0" w:line="400" w:lineRule="exact"/><w:widowControl w:val="0"/>"#;
    let on_pages = |room: f64| {
        let filler = format!(r#"<w:spacing w:before="0" w:after="0" w:line="{}" w:lineRule="exact"/>"#, ((648.0 - room * 20.0 - 5.0) * 20.0) as i64);
        let body = format!(
            "{}{}<w:p><w:pPr>{exact}{HIDDEN}</w:pPr></w:p>{}",
            para(&filler, "Filler"),
            para(&format!("{exact}{HIDDEN}"), "Alpha"),
            para(exact, "Bravo")
        );
        let doc = doc(&body)?;
        let Block::Para(p) = &doc.sections[0].blocks[1] else { panic!("a paragraph") };
        let id = ParaId(p.id);
        let snap = crate::layout::layout(&doc, &fonts());
        let counts = snap
            .pages()
            .iter()
            .map(|pg| pg.children.iter().map(|c| if let PageChild::Body(b) = c { lines_of(&b.children, id) } else { 0 }).sum::<usize>());
        Some(counts.collect::<Vec<_>>())
    };
    let Some(after_empty) = on_pages(2.0) else { return };
    assert_eq!(after_empty, [2, 1], "the empty part on the first page");
    assert_eq!(on_pages(1.0), Some(vec![1, 2]), "the empty part on the second page");
}

// Beside a float

/// A square-wrapped float 100pt wide and `h` points high at the column's left, anchored at the paragraph's top.
fn float(h: f64) -> String {
    let cy = (h * 12700.0) as i64;
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1270000" cy="{cy}"/><wp:wrapSquare wrapText="bothSides"/><a:graphic><a:graphicData><pic:pic><pic:blipFill/></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}

/// A paragraph whose first part holds a float `h` points high.
fn floated(ppr: &str, h: f64, runs: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{}{runs}</w:p>", float(h))
}

/// The lines of paragraph `para` the page's body holds, in order, placed.
fn placed(children: &[Child], para: ParaId, out: &mut Vec<crate::layout::fragment::PlacedLine>) {
    for c in children {
        match c {
            Child::Para(p) if p.para == para => out.extend(p.lines.iter().cloned()),
            Child::Line { para: Some(q), placed } if *q == para => out.push(placed.clone()),
            Child::Box(b) => placed(&b.children, para, out),
            Child::Group { children, .. } => placed(children, para, out),
            Child::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| placed(&c.content, para, out)),
            _ => {}
        }
    }
}

/// The body lines of the paragraph at `block` of `body` on each page.
fn pages_of(body: &str, block: usize) -> Option<Vec<Vec<crate::layout::fragment::PlacedLine>>> {
    let doc = doc(body)?;
    let id = match &doc.sections[0].blocks[block] {
        Block::Para(p) => ParaId(p.id),
        Block::Table(t) => match t.rows[0].cells[0].blocks.first() {
            Some(Block::Para(p)) => ParaId(p.id),
            _ => panic!("a cell paragraph"),
        },
    };
    let snap = crate::layout::layout(&doc, &fonts());
    let pages = snap.pages().iter().map(|pg| {
        let mut out = Vec::new();
        for c in pg.children.iter() {
            if let PageChild::Body(b) = c {
                placed(&b.children, id, &mut out);
            }
        }
        out
    });
    Some(pages.collect())
}

/// The colours of a placed line's horizontal edges: along its top, along its bottom.
fn drawn_edges(l: &crate::layout::fragment::PlacedLine) -> (Vec<u32>, Vec<u32>) {
    let flat: Vec<(f64, u32)> = l
        .line
        .items
        .iter()
        .filter_map(|i| match i {
            crate::layout::fragment::Item::Rect { rect, colour, .. } if rect[2] - rect[0] > 100.0 => Some((rect[1], *colour)),
            _ => None,
        })
        .collect();
    let along = |top: bool| flat.iter().filter(|r| (r.0 < 3.0) == top).map(|r| r.1).collect();
    (along(true), along(false))
}

#[test]
fn beside_a_float_an_empty_last_part_takes_a_line() {
    let body = format!("{}<w:p/>", floated(HIDDEN, 100.0, "<w:r><w:t>Alpha</w:t></w:r>"));
    let Some(pages) = pages_of(&body, 0) else { return };
    assert_eq!(pages[0].len(), 2, "Alpha and the empty part");
}

#[test]
fn in_a_cell_beside_a_float_an_empty_last_part_takes_a_line() {
    let body = format!("{}<w:p/>", floated(HIDDEN, 100.0, "<w:r><w:t>Alpha</w:t></w:r>"));
    let cell = format!(r#"<w:tbl><w:tblGrid><w:gridCol w:w="9000"/></w:tblGrid><w:tr><w:tc>{body}</w:tc></w:tr></w:tbl><w:p/>"#);
    let Some(pages) = pages_of(&cell, 0) else { return };
    assert_eq!(pages[0].len(), 2, "Alpha and the empty part");
}

#[test]
fn beside_a_float_a_part_ending_in_a_line_break_has_an_empty_line_of_its_own() {
    let after = r#"<w:spacing w:after="240"/>"#;
    let body = format!("{}{}", floated(&format!("{after}{HIDDEN}"), 100.0, "<w:r><w:t>Alpha</w:t><w:br/></w:r>"), para("", "Bravo"));
    let Some(pages) = pages_of(&body, 0) else { return };
    let afters: Vec<f64> = pages[0].iter().map(|l| l.line.after).collect();
    assert_eq!(afters.len(), 3, "Alpha, its empty line, Bravo");
    assert!(afters[0].abs() < 0.01 && (afters[1] - 12.0).abs() < 0.01, "the empty line is the first part's last: {afters:?}");
}

#[test]
fn beside_a_float_a_page_break_after_an_empty_part_goes_on_in_the_next() {
    let exact = r#"<w:spacing w:before="0" w:after="0" w:line="400" w:lineRule="exact"/><w:widowControl w:val="0"/>"#;
    let on_pages = |room: f64| {
        let filler = format!(r#"<w:spacing w:before="0" w:after="0" w:line="{}" w:lineRule="exact"/>"#, ((648.0 - room * 20.0 - 5.0) * 20.0) as i64);
        let body = format!(
            "{}{}<w:p><w:pPr>{exact}{HIDDEN}</w:pPr></w:p>{}",
            para(&filler, "Filler"),
            floated(&format!("{exact}{HIDDEN}"), 10.0, "<w:r><w:t>Alpha</w:t></w:r>"),
            para(exact, "Bravo")
        );
        Some(pages_of(&body, 1)?.iter().map(Vec::len).collect::<Vec<_>>())
    };
    let Some(one) = on_pages(1.0) else { return };
    assert_eq!(one, [1, 2], "the empty part on the second page");
    assert_eq!(on_pages(2.0), Some(vec![2, 1]), "the empty part on the first page");
}

#[test]
fn beside_a_float_each_part_draws_its_own_box() {
    let bdr = |c: &str| {
        let e = |s: &str| format!(r#"<w:{s} w:val="single" w:sz="8" w:space="1" w:color="{c}"/>"#);
        format!("<w:pBdr>{}{}{}{}</w:pBdr>", e("top"), e("left"), e("bottom"), e("right"))
    };
    let body = format!("{}{}", floated(&format!("{}{HIDDEN}", bdr("FF0000")), 100.0, "<w:r><w:t>Alpha</w:t></w:r>"), para(&bdr("0000FF"), "Bravo"));
    let Some(pages) = pages_of(&body, 0) else { return };
    let l = &pages[0];
    assert_eq!(l.len(), 2);
    assert_eq!(drawn_edges(&l[0]), (vec![0xFF0000], vec![0xFF0000]), "the red box closes");
    assert_eq!(drawn_edges(&l[1]), (vec![0x0000FF], vec![0x0000FF]), "the blue box opens");
    assert!(l[1].y >= l[0].y + l[0].line.pitch - 0.01, "below the red box: {} {} {}", l[0].y, l[0].line.pitch, l[1].y);
}
