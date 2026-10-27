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
