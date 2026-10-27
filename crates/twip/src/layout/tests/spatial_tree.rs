//! The spatial tree's containment equals the model's: every line of
//! a paragraph is reported inside exactly the table cells that contain that paragraph, in every story.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::docx::{Block, Document};
use crate::layout::traverse::{SpatialNode, StoryRef, spatial_tree};
use crate::layout::{ParaId, TableId, layout};

/// (table, row, model cell index) from the story root down.
type Chain = Vec<(TableId, usize, usize)>;

fn model_chains(blocks: &[Block], chain: &mut Chain, out: &mut HashMap<ParaId, Chain>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                out.insert(ParaId(p.id), chain.clone());
            }
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        chain.push((TableId(t.id), r, c));
                        model_chains(&cell.blocks, chain, out);
                        chain.pop();
                    }
                }
            }
        }
    }
}

fn tree_chains(nodes: &[SpatialNode], chain: &mut Chain, out: &mut Vec<(ParaId, Chain)>) {
    for n in nodes {
        match n {
            SpatialNode::Line(l) => {
                if let Some(p) = l.para {
                    out.push((p, chain.clone()));
                }
            }
            SpatialNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        chain.push((t.table, row.row, cell.place.index));
                        if let Some((p, _)) = cell.closing {
                            out.push((p, chain.clone()));
                        }
                        tree_chains(&cell.nodes, chain, out);
                        chain.pop();
                    }
                }
            }
        }
    }
}

/// The repo's own fixtures, plus any in `tests/fixtures/editor`. A listed fixture that is missing fails.
fn documents() -> Vec<(PathBuf, Document)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<PathBuf> = ["tables.docx", "objects.docx", "lists.docx", "pagination.docx"].iter().map(|f| root.join(f)).collect();
    if let Ok(dir) = std::fs::read_dir(root.join("editor")) {
        paths.extend(dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "docx")));
    }
    paths
        .into_iter()
        .map(|p| {
            let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("fixture {}: {e}", p.display()));
            let doc = crate::docx::read(&bytes).unwrap_or_else(|e| panic!("fixture {}: {e}", p.display()));
            (p, doc)
        })
        .collect()
}

#[test]
fn every_line_sits_in_the_cells_that_contain_its_paragraph() {
    let fonts = super::fonts();
    for (path, doc) in documents() {
        let mut model = HashMap::new();
        for s in &doc.sections {
            model_chains(&s.blocks, &mut Vec::new(), &mut model);
        }
        for blocks in doc.stories.values() {
            model_chains(blocks, &mut Vec::new(), &mut model);
        }
        for blocks in doc.footnotes.values().chain(doc.endnotes.values()) {
            model_chains(blocks, &mut Vec::new(), &mut model);
        }
        let snapshot = layout(&doc, &fonts);
        for (pi, page) in snapshot.pages().iter().enumerate() {
            let tree = spatial_tree(page, false).expect("a full layout has no provisional pages");
            for story in &tree.stories {
                // Text boxes and frames are stories of their own; their paragraphs live inside the anchor's inlines.
                if matches!(story.story, StoryRef::Float { .. }) {
                    continue;
                }
                let mut lines = Vec::new();
                tree_chains(&story.nodes, &mut Vec::new(), &mut lines);
                for (para, chain) in lines {
                    let Some(want) = model.get(&para) else { continue };
                    assert_eq!(&chain, want, "{}: page {pi}, paragraph {para:?}", path.display());
                }
            }
        }
    }
}

#[test]
fn a_cell_closing_paragraph_after_a_nested_table_has_a_caret_place() {
    let fonts = super::fonts();
    for (path, doc) in documents() {
        let snapshot = layout(&doc, &fonts);
        let mut expected = 0;
        let mut count = |blocks: &[Block]| fn_count(blocks, &mut expected);
        for s in &doc.sections {
            count(&s.blocks);
        }
        let mut found = 0;
        for page in snapshot.pages() {
            let tree = spatial_tree(page, false).expect("full layout");
            for story in tree.stories.iter().filter(|s| s.story == StoryRef::Body) {
                found += closings(&story.nodes);
            }
        }
        // A row split across pages shows its closing place once, on the part with the cell's last content.
        assert!(found >= expected, "{}: {found} closing caret places for {expected} closing paragraphs", path.display());
    }
}

fn fn_count(blocks: &[Block], n: &mut usize) {
    for b in blocks {
        if let Block::Table(t) = b {
            for cell in t.rows.iter().flat_map(|r| r.cells.iter()) {
                if cell.v_merge != crate::docx::VMerge::Continue
                    && matches!(cell.blocks.as_slice(), [.., Block::Table(_), Block::Para(p)] if p.inlines.is_empty())
                {
                    *n += 1;
                }
                fn_count(&cell.blocks, n);
            }
        }
    }
}

fn closings(nodes: &[SpatialNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            SpatialNode::Line(_) => 0,
            SpatialNode::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| usize::from(c.closing.is_some()) + closings(&c.nodes)).sum(),
        })
        .sum()
}
