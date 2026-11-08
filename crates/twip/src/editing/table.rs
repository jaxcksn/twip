//! Table grid

use crate::docx::{Table, VMerge};

pub struct Grid {
    pub rows: Vec<Vec<(usize, usize, VMerge)>>,
    pub cols: usize,
}

impl Grid {
    pub fn of(t: &Table) -> Grid {
        let rows: Vec<Vec<(usize, usize, VMerge)>> = t
            .rows
            .iter()
            .map(|r| {
                let mut col = r.grid_before;
                r.cells
                    .iter()
                    .map(|c| {
                        let at = (col, c.span.max(1), c.v_merge);
                        col += c.span.max(1);
                        at
                    })
                    .collect()
            })
            .collect();
        let cols = t.grid.len().max(rows.iter().filter_map(|r| r.last().map(|&(c, s, _)| c + s)).max().unwrap_or(0));
        Grid { rows, cols }
    }

    pub fn cell_at(&self, row: usize, col: usize) -> Option<usize> {
        self.rows.get(row)?.iter().position(|&(c, s, _)| col >= c && col < c + s)
    }

    pub fn col_of(&self, row: usize, cell: usize) -> Option<usize> {
        self.rows.get(row)?.get(cell).map(|c| c.0)
    }

    pub fn continues(&self, row: usize, cell: usize) -> bool {
        self.rows.get(row).and_then(|r| r.get(cell)).is_some_and(|c| c.2 == VMerge::Continue)
    }

    pub fn merge_start(&self, row: usize, cell: usize) -> (usize, usize) {
        let (mut r, mut c) = (row, cell);
        while self.continues(r, c) && r > 0 {
            let col = self.col_of(r, c).unwrap_or(0);
            match self.cell_at(r - 1, col) {
                Some(above) => {
                    r -= 1;
                    c = above;
                }
                None => break,
            }
        }
        (r, c)
    }

    pub fn reading_order(&self) -> Vec<(usize, usize)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(r, cells)| cells.iter().enumerate().filter(|(_, c)| c.2 != VMerge::Continue).map(move |(k, _)| (r, k)))
            .collect()
    }

    pub fn below(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        (row + 1..self.rows.len()).find_map(|r| self.cell_at(r, col).filter(|&c| !self.continues(r, c)).map(|c| (r, c)))
    }

    pub fn above(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let r = row.checked_sub(1)?;
        let c = self.cell_at(r, col)?;
        Some(self.merge_start(r, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-TBL-MERGED.docx")).expect("FX-TBL-MERGED fixture");
        let doc = crate::docx::read(&bytes).expect("reads");
        doc.sections[0]
            .blocks
            .iter()
            .find_map(|b| match b {
                crate::docx::Block::Table(t) => Some(t.clone()),
                _ => None,
            })
            .expect("a table")
    }

    #[test]
    fn merged_cells_are_visited_once_and_resolve_to_their_start() {
        let t = table();
        let g = Grid::of(&t);
        let order = g.reading_order();
        assert!(order.iter().all(|&(r, c)| !g.continues(r, c)));
        for (r, cells) in g.rows.iter().enumerate() {
            for (c, &(col, _, vm)) in cells.iter().enumerate() {
                if vm == VMerge::Continue {
                    let (sr, sc) = g.merge_start(r, c);
                    assert!(sr < r && !g.continues(sr, sc) && g.col_of(sr, sc) == Some(col));
                }
            }
        }
    }
}
