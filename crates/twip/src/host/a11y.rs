//! The accessibility surface: the reading tree the host mirrors into its accessibility DOM (headings,
//! paragraphs, and tables with their rows and cells, header cells marked, at every nesting level) and the cell context
//! announced when the caret enters, leaves or moves through a table.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{Engine, r1};
use crate::docx::{Block, Document};
use crate::editing::text::UnitText;
use crate::layout::{ParaId, TableId};

/// Where the caret's cell context was last announced: the table, its row and grid column, and the nesting level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellContext {
    pub table: Option<TableId>,
    pub row: usize,
    pub col: usize,
    pub depth: usize,
}

impl Engine {
    /// The reading tree as JSON: `{r: role, l: level, a: label, t: [lines], c: [children]}`; each line is its box in
    /// content CSS pixels and its text.
    pub fn accessible_tree(&mut self) -> String {
        let k = self.px_per_pt();
        let Some(e) = &mut self.editor else { return "{\"r\":\"document\",\"l\":0,\"c\":[],\"t\":[]}".into() };
        let stack = e.stack();
        // Every laid-out body line, by paragraph.
        let mut lines: HashMap<ParaId, Vec<Value>> = HashMap::new();
        for (pi, geo) in e.page_geometries() {
            for l in geo.lines.iter().filter(|l| !l.repeat) {
                if geo.stories[l.story].story != crate::editing::position::StoryId::Body {
                    continue;
                }
                let Some(p) = e.index().para(e.document(), l.para) else { continue };
                let t = UnitText::of(p);
                let s: String = t.text.chars().skip(l.first().0 as usize).take((l.last().0.saturating_sub(l.first().0)) as usize).collect();
                if s.trim().is_empty() {
                    continue;
                }
                let Some((x, y)) = stack.to_doc(pi, l.x0(), l.top) else { continue };
                lines
                    .entry(l.para)
                    .or_default()
                    .push(json!({ "x": r1(x * k), "y": r1(y * k), "w": r1((l.x1() - l.x0()) * k), "h": r1((l.bottom - l.top) * k), "s": s }));
            }
        }
        let doc = e.document();
        let mut children = Vec::new();
        for s in &doc.sections {
            blocks(doc, &s.blocks, 0, &mut lines, &mut children);
        }
        json!({ "r": "document", "l": 0, "t": [], "c": children }).to_string()
    }

    /// The announcement the last input calls for (the caret's cell context changed), if any.
    pub fn take_announcement(&mut self) -> Option<String> {
        let e = self.editor.as_mut()?;
        let f = e.format_state()?;
        let now = match (f.table, f.cell) {
            (Some(t), Some((row, cell))) => {
                let depth = e.selection().focus().map_or(0, |p| e.index().chain_of(&p).len());
                let col = e
                    .index()
                    .table(e.document(), t)
                    .and_then(|tb| crate::editing::table::Grid::of(tb).rows.get(row)?.get(cell).map(|c| c.0))
                    .unwrap_or(cell);
                CellContext { table: Some(t), row, col, depth }
            }
            _ => CellContext::default(),
        };
        let was = std::mem::replace(&mut self.cell_context, now);
        if was == now {
            return None;
        }
        let Some(t) = now.table else { return was.table.map(|_| "Left table".to_string()) };
        let tb = e.index().table(e.document(), t)?;
        let mut parts = Vec::new();
        if was.table != now.table {
            let cols = crate::editing::table::Grid::of(tb).cols;
            parts.push(format!("Table, {} rows, {cols} columns", tb.rows.len()));
            if now.depth > 1 {
                parts.push(format!("nested, level {}", now.depth));
            }
        }
        parts.push(format!("Row {}, column {}", now.row + 1, now.col + 1));
        // The column's header cell (a header row's cell over this column), when the caret isn't in it.
        if tb.rows.first().is_some_and(|r| r.header) && now.row > 0 {
            let g = crate::editing::table::Grid::of(tb);
            let header = g.rows.first().and_then(|cells| cells.iter().position(|&(c, s, _)| c <= now.col && now.col < c + s.max(1)));
            if let Some(text) = header.and_then(|k| tb.rows[0].cells.get(k)).map(cell_text).filter(|s| !s.is_empty()) {
                parts.push(text);
            }
        }
        Some(parts.join(", "))
    }
}

/// A cell's text (its paragraphs, spaced).
fn cell_text(c: &crate::docx::Cell) -> String {
    c.blocks
        .iter()
        .filter_map(|b| if let Block::Para(p) = b { Some(crate::docx::fields::text(p)) } else { None })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// The reading nodes of `blocks`: headings and paragraphs with their lines, tables with rows and cells.
fn blocks(_doc: &Document, list: &[Block], depth: usize, lines: &mut HashMap<ParaId, Vec<Value>>, out: &mut Vec<Value>) {
    for b in list {
        match b {
            Block::Para(p) => {
                let Some(t) = lines.remove(&ParaId(p.id)) else { continue };
                match p.style.outline_lvl.filter(|l| (0..9).contains(l)) {
                    Some(l) => out.push(json!({ "r": "heading", "l": l + 1, "t": t, "c": [] })),
                    None => out.push(json!({ "r": "paragraph", "l": 0, "t": t, "c": [] })),
                }
            }
            Block::Table(tb) => {
                let level = depth + 1;
                let mut rows = Vec::new();
                for r in &tb.rows {
                    let mut cells = Vec::new();
                    for c in &r.cells {
                        let mut kids = Vec::new();
                        blocks(_doc, &c.blocks, level, lines, &mut kids);
                        cells.push(json!({ "r": if r.header { "columnheader" } else { "cell" }, "l": level, "t": [], "c": kids }));
                    }
                    rows.push(json!({ "r": "row", "l": level, "t": [], "c": cells }));
                }
                let label = if level > 1 { format!("Table, level {level}") } else { "Table".into() };
                out.push(json!({ "r": "table", "l": level, "a": label, "t": [], "c": rows }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    #[test]
    fn tree_has_tables_and_cells_announce() {
        let mut e = Engine::with_fonts(Fonts::system());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-NEST-DEEP.docx");
        e.open(&std::fs::read(path).unwrap());
        let tree: Value = serde_json::from_str(&e.accessible_tree()).unwrap();
        fn find<'a>(v: &'a Value, role: &str, out: &mut Vec<&'a Value>) {
            if v["r"] == role {
                out.push(v);
            }
            for c in v["c"].as_array().into_iter().flatten() {
                find(c, role, out);
            }
        }
        let mut tables = Vec::new();
        find(&tree, "table", &mut tables);
        assert!(tables.len() >= 2, "tables at every level");
        assert!(tables.iter().any(|t| t["l"].as_u64() > Some(1)), "a nested table has its level");
        let mut rows = Vec::new();
        find(&tree, "row", &mut rows);
        assert!(!rows.is_empty());
        // Into the first table: its size and the cell; then the next cell.
        e.take_announcement();
        e.command("table.select.cell", None);
        let mut moved = false;
        for _ in 0..40 {
            e.key("ArrowDown", false, false, false);
            if let Some(a) = e.take_announcement() {
                assert!(a.contains("Row ") && a.contains("column "), "{a}");
                moved = true;
                break;
            }
        }
        assert!(moved, "entering a table announces its cell");
    }
}
