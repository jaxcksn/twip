//! Steps: every step kind applied then inverted restores the document exactly; incremental layout after each
//! step equals a full layout.

use crate::docx::{Block, Document, Inline};
use crate::layout::{DocumentLayout, ParaId, Revision, TableId, Unit, dump, layout};

use super::super::index::{Container, DocIndex};
use super::super::step::{Step, para_len};
use super::fonts;

fn read(name: &str) -> Document {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    crate::docx::read_editable(&std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("reads")
}

fn paras(doc: &Document) -> Vec<(ParaId, usize)> {
    fn walk(blocks: &[Block], out: &mut Vec<(ParaId, usize)>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push((ParaId(p.id), para_len(p).0 as usize)),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

fn tables(doc: &Document) -> Vec<TableId> {
    fn walk(blocks: &[Block], out: &mut Vec<TableId>) {
        for b in blocks {
            if let Block::Table(t) = b {
                out.push(TableId(t.id));
                t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out));
            }
        }
    }
    let mut out = Vec::new();
    doc.sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

/// Steps of every kind at several places in the document.
fn steps(doc: &Document, idx: &DocIndex) -> Vec<Step> {
    let mut out = Vec::new();
    let ps = paras(doc);
    let run = |p: ParaId| idx.para(doc, p).map(|p| p.mark.clone()).expect("paragraph");
    for &(p, len) in ps.iter().step_by((ps.len() / 6).max(1)) {
        let mid = Unit((len / 2) as u32);
        out.push(Step::Text { para: p, range: mid..mid, with: vec![Inline::Text("xyz".into(), run(p))], edges: Default::default() });
        if len >= 2 {
            out.push(Step::Text { para: p, range: Unit(0)..Unit(2), with: Vec::new(), edges: Default::default() });
        }
        let par = idx.para(doc, p).expect("paragraph");
        let second = crate::docx::Paragraph { inlines: Vec::new(), src: None, id: crate::docx::next_para_id(), ..par.clone() };
        out.push(Step::Split { para: p, at: mid, second });
    }
    for w in ps.windows(2).step_by((ps.len() / 4).max(1)) {
        let (a, b) = (idx.para_slot(w[0].0).expect("slot"), idx.para_slot(w[1].0).expect("slot"));
        if a.container == b.container && b.block == a.block + 1 {
            out.push(Step::Join { first: w[0].0, second: w[1].0 });
        }
    }
    out.push(Step::Blocks { container: Container::Section(0), range: 1..2, with: Vec::new() });
    for t in tables(doc) {
        let table = idx.table(doc, t).expect("table");
        let mut row = table.rows[0].clone();
        for c in &mut row.cells {
            for b in &mut c.blocks {
                renew(b);
            }
        }
        out.push(Step::Rows { table: t, range: 1.min(table.rows.len())..1.min(table.rows.len()), with: vec![row], grid: None });
        if table.rows.len() > 1 {
            out.push(Step::Rows { table: t, range: 0..1, with: Vec::new(), grid: None });
        }
    }
    out
}

/// Fresh ids for copied content (a copy is new content).
fn renew(b: &mut Block) {
    match b {
        Block::Para(p) => p.id = crate::docx::next_para_id(),
        Block::Table(t) => {
            t.id = crate::docx::next_para_id();
            t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).flat_map(|c| c.blocks.iter_mut()).for_each(renew);
        }
    }
}

#[test]
fn every_step_inverts_exactly_and_keeps_layout_incremental() {
    let fonts = fonts();
    for name in ["FX-TEXT.docx", "FX-TBL-MERGED.docx", "FX-NEST-3.docx"] {
        let doc0 = read(name);
        let idx0 = DocIndex::build(&doc0);
        for (k, step) in steps(&doc0, &idx0).into_iter().enumerate() {
            let what = format!("{name} step {k}: {step:?}");
            let (mut doc, mut idx) = (doc0.clone(), idx0.clone());
            let mut dl = DocumentLayout::new();
            dl.edit(Revision(1), None);
            dl.complete(&doc, fonts);
            let applied = step.apply(&mut doc, &mut idx).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            assert_eq!(idx, DocIndex::build(&doc), "{what}: index after the step");
            dl.edit(Revision(2), applied.hint);
            dl.complete(&doc, fonts);
            assert_eq!(dump::page_text(&dl.snapshot(), &doc), dump::page_text(&layout(&doc, fonts), &doc), "{what}: incremental layout");
            applied.inverse.apply(&mut doc, &mut idx).unwrap_or_else(|e| panic!("{what}: inverse {e:?}"));
            assert_eq!(doc.sections, doc0.sections, "{what}: the inverse restores the document");
            assert_eq!(idx, idx0, "{what}: the inverse restores the index");
        }
    }
}
