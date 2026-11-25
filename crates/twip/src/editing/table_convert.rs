//! Text to table and table to text

use crate::api::command::{TableToText, TextSeparator, TextToTable};
use crate::docx::{Block, Control, ControlId, Inline, ParaStyle, Paragraph, Run, VMerge};
use crate::font::Fonts;
use crate::layout::Unit;

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::Selection;
use super::step::Step;
use super::tables::{MAX_COLUMNS, MAX_ROWS, blank_para, tidy_merges};
use crate::document::paragraph::{ListId, ListRef};

/// Where text splits into cells, or what joins cells into text.
#[derive(Clone, Copy, PartialEq)]
enum Sep {
    Paragraphs,
    Tab,
    Char(char),
}

impl Sep {
    fn of(kind: TextSeparator, character: Option<&str>) -> Option<Sep> {
        Some(match kind {
            TextSeparator::Paragraphs => Sep::Paragraphs,
            TextSeparator::Tabs => Sep::Tab,
            TextSeparator::Commas => Sep::Char(','),
            TextSeparator::Other => Sep::Char(character?.chars().next()?),
        })
    }

    /// The separator as an inline in `run`'s formatting.
    fn inline(self, run: &Run) -> Option<Inline> {
        match self {
            Sep::Paragraphs => None,
            Sep::Tab => Some(Inline::Tab(run.clone())),
            Sep::Char(c) => Some(Inline::Text(c.to_string(), run.clone())),
        }
    }
}

/// How many pieces [`pieces`] cuts `inlines` into.
fn piece_count(inlines: &[Inline], sep: Sep) -> usize {
    let mut depth = 0usize;
    let mut n = 1;
    for i in inlines {
        match i {
            Inline::FieldBegin { .. } | Inline::ControlStart(_) => depth += 1,
            Inline::FieldEnd { .. } | Inline::ControlEnd(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        match (sep, i) {
            (Sep::Tab, Inline::Tab(_)) if depth == 0 => n += 1,
            (Sep::Char(c), Inline::Text(s, _)) if depth == 0 => n += s.matches(c).count(),
            _ => {}
        }
    }
    n
}

/// `inlines` split at `sep`, outside fields and content controls.
fn pieces(inlines: &[Inline], sep: Sep) -> Vec<Vec<Inline>> {
    let mut out: Vec<Vec<Inline>> = vec![Vec::new()];
    let mut depth = 0usize;
    for i in inlines {
        match i {
            Inline::FieldBegin { .. } | Inline::ControlStart(_) => depth += 1,
            Inline::FieldEnd { .. } | Inline::ControlEnd(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        match (sep, i) {
            (Sep::Tab, Inline::Tab(_)) if depth == 0 => out.push(Vec::new()),
            (Sep::Char(c), Inline::Text(s, run)) if depth == 0 && s.contains(c) => {
                for (k, part) in s.split(c).enumerate() {
                    if k > 0 {
                        out.push(Vec::new());
                    }
                    if !part.is_empty()
                        && let Some(last) = out.last_mut()
                    {
                        last.push(Inline::Text(part.to_string(), run.clone()));
                    }
                }
            }
            _ => {
                if let Some(last) = out.last_mut() {
                    last.push(i.clone());
                }
            }
        }
    }
    out
}

/// A new paragraph formatted like `p` holding `inlines`.
fn controls(b: &Block) -> &[ControlId] {
    match b {
        Block::Para(p) => &p.controls,
        Block::Table(t) => &t.controls,
    }
}

fn para_of(p: &Paragraph, inlines: Vec<Inline>, list: bool) -> Paragraph {
    let mut q = blank_para(p);
    q.style = p.style.clone();
    q.div = p.div.clone();
    if list {
        q.list = p.list.clone();
    }
    if inlines.iter().any(|i| i.units() > 0 || matches!(i, Inline::Text(..))) {
        q.inlines = inlines;
    } else {
        q.inlines.splice(0..0, inlines);
    }
    q
}

/// How Convert Text to Table sizes the new table's columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConvertFit {
    /// Fixed column width: each column this wide, or (`None`, Word's Auto) the text width shared equally.
    Fixed(Option<f64>),
    /// AutoFit to contents: each column as wide as what it holds.
    Contents,
    /// AutoFit to window: the table fills the width it is in.
    Window,
}

/// Most levels of nested tables Convert Table to Text flattens; deeper ones stay tables.
const MAX_FLATTEN: usize = 32;

/// Rows `rows` of `t` as text joined at `sep`, in the content controls `around` and theirs; nested tables flattened
/// the same way while `depth` (levels left) lasts, else kept.
fn flatten(
    t: &crate::docx::Table,
    rows: std::ops::RangeInclusive<usize>,
    around: &[ControlId],
    sep: Sep,
    depth: Option<usize>,
    out: &mut Vec<Block>,
) {
    for row in &t.rows[rows] {
        let outer: Vec<_> = around.iter().chain(&t.controls).chain(&row.controls).copied().collect();
        // The open paragraph a cell's first paragraph joins
        let mut open: Option<Paragraph> = None;
        for cell in row.cells.iter().filter(|c| c.v_merge != VMerge::Continue) {
            let mut first = true;
            for b in &cell.blocks {
                match b {
                    Block::Para(p) => {
                        let joined = first && sep != Sep::Paragraphs;
                        match open.as_mut() {
                            Some(q) if joined => {
                                q.inlines.extend(sep.inline(&p.mark));
                                q.inlines.extend(p.inlines.iter().filter(|i| !matches!(i, Inline::Text(s, _) if s.is_empty())).cloned());
                            }
                            _ => {
                                out.extend(open.take().map(Block::Para));
                                let mut q = para_of(p, p.inlines.clone(), true);
                                q.controls = outer.iter().chain(&cell.controls).chain(&p.controls).copied().collect();
                                open = Some(q);
                            }
                        }
                    }
                    Block::Table(nested) => {
                        out.extend(open.take().map(Block::Para));
                        let inside: Vec<_> = outer.iter().chain(&cell.controls).copied().collect();
                        match depth.and_then(|d| d.checked_sub(1)) {
                            Some(d) if !nested.rows.is_empty() => flatten(nested, 0..=nested.rows.len() - 1, &inside, sep, Some(d), out),
                            _ => {
                                let mut nested = nested.clone();
                                nested.controls = inside.iter().chain(&nested.controls).copied().collect();
                                out.push(super::clipboard::fresh(&Block::Table(nested)));
                            }
                        }
                    }
                }
                first = false;
            }
        }
        out.extend(open.take().map(Block::Para));
    }
}

/// The row and cell controls of rows `rows` of `t` and of the nested tables [`flatten`] flattens with `depth`, into
/// `ids`; whether a cell or a paragraph in one is in a control of its own, which a cell joined to the one before it
/// can't keep.
fn row_and_cell_controls(t: &crate::docx::Table, rows: std::ops::RangeInclusive<usize>, depth: Option<usize>, ids: &mut Vec<ControlId>) -> bool {
    let mut own = false;
    for row in &t.rows[rows] {
        ids.extend_from_slice(&row.controls);
        for cell in &row.cells {
            ids.extend_from_slice(&cell.controls);
            own |= !cell.controls.is_empty();
            for b in &cell.blocks {
                match b {
                    Block::Para(p) => own |= !p.controls.is_empty(),
                    Block::Table(n) => {
                        if let Some(d) = depth.and_then(|d| d.checked_sub(1))
                            && !n.rows.is_empty()
                        {
                            own |= row_and_cell_controls(n, 0..=n.rows.len() - 1, Some(d), ids);
                        }
                    }
                }
            }
        }
    }
    own
}

impl Editor {
    /// `p`'s formatting without its numbering, re-resolved: the cells after a numbered paragraph's first.
    fn unnumbered(&self, p: &Paragraph) -> Option<std::rc::Rc<ParaStyle>> {
        use crate::docx::write::props::Props;
        p.style.num.filter(|&ListRef { list: ListId(id), level: _ }| id != 0)?;
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let w = &cx.w;
        let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
        let styled = res.styles.chain_ppr(res.styles.paragraph_style(p.style.style_id.as_deref())).num_id.is_some_and(|n| n != 0);
        if styled {
            pr.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"0\"/><{w}numId {w}val=\"0\"/></{w}numPr>"));
        } else {
            pr.remove("numPr");
        }
        let again = self.para_again(crate::layout::ParaId(p.id), Some(pr.xml().as_deref()), &res, &cx)?;
        Some(std::rc::Rc::new(again.style))
    }

    pub(super) fn convert_text_to_table(&mut self, o: &TextToTable, fonts: &Fonts) -> Outcome {
        use crate::api::state::TableFit;
        let fit = match o.autofit.unwrap_or_default() {
            TableFit::Fixed => ConvertFit::Fixed(o.column_width.map(|w| w.0)),
            TableFit::Contents => ConvertFit::Contents,
            TableFit::Window => ConvertFit::Window,
        };
        self.convert_text_to_table_fit(o, fit, fonts)
    }

    /// The paragraphs Convert Text to Table would convert, in the selection's container: none when the selection
    /// crosses a table or holds no paragraph.
    fn convert_source(&self) -> Result<(super::Container, std::ops::Range<usize>), Outcome> {
        let ends = self.selection.anchor().zip(self.selection.focus()).and_then(|(a, f)| Some((a.para_offset()?.0, f.para_offset()?.0)));
        let Some((a, f)) = ends else { return Err(Outcome::unavailable("no text selected")) };
        let (Some(sa), Some(sf)) = (self.index.para_slot(a), self.index.para_slot(f)) else { return Err(Outcome::unavailable("no paragraph")) };
        if sa.container != sf.container {
            return Err(Outcome::Unchanged("the selection crosses a table"));
        }
        Ok((sa.container, sa.block.min(sf.block)..sa.block.max(sf.block) + 1))
    }

    /// The rows and columns Convert Text to Table would make of the selection with `o` (its preview): the columns
    /// `o` gives, else the most pieces a paragraph has.
    pub fn text_to_table_size(&self, o: &TextToTable) -> Option<(u32, u32)> {
        let sep = Sep::of(o.separator, o.character.as_deref())?;
        let (container, range) = self.convert_source().ok()?;
        let blocks = self.index.blocks(&self.doc, container)?;
        let mut counts: Vec<usize> = Vec::with_capacity(range.len());
        for b in blocks.get(range)? {
            // Paragraphs, none in a content control
            let Block::Para(p) = b else { return None };
            if !p.controls.is_empty() {
                return None;
            }
            counts.push(if sep == Sep::Paragraphs { 1 } else { piece_count(&p.inlines, sep) });
        }
        let widest = counts.iter().copied().max().unwrap_or(1);
        let ncols = o.columns.map_or(widest, |c| c as usize).clamp(1, MAX_COLUMNS);
        let rows = if sep == Sep::Paragraphs { counts.len().div_ceil(ncols) } else { counts.iter().map(|n| n.max(&1).div_ceil(ncols)).sum() };
        (rows <= MAX_ROWS).then_some((rows as u32, ncols as u32))
    }

    /// Convert Text to Table with its AutoFit behaviour `fit`, in one undo step.
    pub fn convert_text_to_table_fit(&mut self, o: &TextToTable, fit: ConvertFit, fonts: &Fonts) -> Outcome {
        if let ConvertFit::Fixed(Some(w)) = fit
            && !(w.is_finite() && (1.0..=1584.0).contains(&w))
        {
            return Outcome::invalid("a column is 1…1584 pt wide");
        }
        if fit != ConvertFit::Contents {
            return self.convert_text_to_table_in(o, fit, fonts);
        }
        self.joined("Convert Text to Table", fonts, |e| {
            let out = e.convert_text_to_table_in(o, fit, fonts);
            if !matches!(out, Outcome::Changed(_)) {
                return out;
            }
            let Some(tg) = e.table_target() else { return out };
            let cols = e.index.table(&e.doc, tg.table).map_or(0, |t| t.grid.len());
            if let Some(last) = cols.checked_sub(1) {
                e.auto_fit_columns_to_contents(tg.table, (0, last), fonts);
            }
            out
        })
    }

    fn convert_text_to_table_in(&mut self, o: &TextToTable, fit: ConvertFit, fonts: &Fonts) -> Outcome {
        let Some(sep) = Sep::of(o.separator, o.character.as_deref()) else { return Outcome::unavailable("no separator") };
        let (container, range) = match self.convert_source() {
            Ok(x) => x,
            Err(out) => return out,
        };
        let Some(blocks) = self.index.blocks(&self.doc, container) else { return Outcome::unavailable("no paragraph") };
        let mut paras: Vec<&Paragraph> = Vec::with_capacity(range.len());
        for b in &blocks[range.clone()] {
            match b {
                Block::Para(p) => paras.push(p),
                Block::Table(_) => return Outcome::Unchanged("the selection crosses a table"),
            }
        }
        // A content control around some of the paragraphs would be cut.
        if paras.iter().any(|p| !p.controls.is_empty()) {
            return self.region_boundary();
        }
        let split: Vec<Vec<Vec<Inline>>> =
            paras.iter().map(|p| if sep == Sep::Paragraphs { vec![p.inlines.clone()] } else { pieces(&p.inlines, sep) }).collect();
        let widest = if sep == Sep::Paragraphs { 1 } else { split.iter().map(Vec::len).max().unwrap_or(1) };
        let ncols = o.columns.map_or(widest, |c| c as usize).clamp(1, MAX_COLUMNS);
        // Cells: a paragraph's pieces start a row, or with paragraph marks every paragraph is a cell.
        let mut content: Vec<Vec<Vec<Block>>> = Vec::new();
        for (p, parts) in paras.iter().zip(split) {
            let unnumbered = if parts.len() > 1 { self.unnumbered(p) } else { None };
            for (k, piece) in parts.into_iter().enumerate() {
                let new_row = content.last().is_none_or(|r| r.len() == ncols) || (sep != Sep::Paragraphs && k == 0);
                if new_row {
                    if content.len() == MAX_ROWS {
                        return Outcome::Unchanged("too many rows");
                    }
                    content.push(Vec::with_capacity(ncols));
                }
                if let Some(row) = content.last_mut() {
                    let mut q = para_of(p, piece, k == 0);
                    if let Some(style) = unnumbered.as_ref().filter(|_| k > 0) {
                        q.style = style.clone();
                    }
                    row.push(vec![Block::Para(q)]);
                }
            }
        }
        let (first, last) = (paras[0].clone(), paras[paras.len() - 1].clone());
        let width = match fit {
            ConvertFit::Fixed(Some(w)) => w * ncols as f64,
            _ => self.text_width(container),
        };
        let mut table = self.new_table(content, ncols, width, &first);
        match fit {
            ConvertFit::Window => self.fit_to_window(&mut table),
            ConvertFit::Contents => table.width = crate::docx::props::Width::Auto,
            ConvertFit::Fixed(Some(_)) => {
                let cx = self.cx();
                let mut props = super::step::TableProps::of(&table);
                super::table_format::set_own(&mut props, "tblLayout", format!("<{w}tblLayout {w}type=\"fixed\"/>", w = cx.w), &cx);
                (table.fixed, table.direct) = (true, props.direct);
            }
            ConvertFit::Fixed(None) => {}
        }
        let cell = table.rows.first().and_then(|r| r.cells.first()).and_then(|c| c.blocks.first()).map(|b| match b {
            Block::Para(p) => p.id,
            Block::Table(t) => t.id,
        });
        let is_table = |k: Option<usize>| k.and_then(|k| blocks.get(k)).is_some_and(|b| matches!(b, Block::Table(_)));
        // Tables side by side join into one, and a story ends in a paragraph.
        let before = is_table(range.start.checked_sub(1));
        let after = range.end == blocks.len() || is_table(Some(range.end));
        let mut with = Vec::with_capacity(3);
        if before {
            with.push(Block::Para(blank_para(&first)));
        }
        with.push(Block::Table(table));
        if after {
            with.push(Block::Para(blank_para(&last)));
        }
        let story = self.index.story(container);
        let mut tx = self.begin("Convert Text to Table");
        if self.step(&mut tx, Step::Blocks { container, range, with }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("convert failed");
        }
        if let Some(p) = cell {
            self.selection = Selection::Caret(Position::text(story, crate::layout::ParaId(p), Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    /// A new table as AutoFit to Window makes it: 100% of the width it is in, its columns fitted.
    fn fit_to_window(&self, t: &mut crate::docx::Table) {
        let cx = self.cx();
        let mut props = super::step::TableProps::of(t);
        super::table_format::set_width(&mut props, crate::docx::props::Width::Pct(1.0), &cx);
        (t.width, t.direct) = (props.width, props.direct);
    }

    pub(super) fn convert_table_to_text(&mut self, o: &TableToText, fonts: &Fonts) -> Outcome {
        self.convert_table_to_text_nested(o, o.convert_nested.unwrap_or(false), fonts)
    }

    /// Convert Table to Text; `nested` converts the tables in its cells too, with the same separator (Word's "Convert
    /// nested tables"), else they stay tables.
    pub fn convert_table_to_text_nested(&mut self, o: &TableToText, nested: bool, fonts: &Fonts) -> Outcome {
        let Some(sep) = Sep::of(o.separator, o.character.as_deref()) else { return Outcome::unavailable("no separator") };
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        let Some(slot) = self.index.table_slot(tg.table) else { return Outcome::unavailable("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::unavailable("not in a table") };
        let (r0, r1) = if matches!(self.selection, Selection::Cells { .. }) { tg.rows } else { (0, t.rows.len().saturating_sub(1)) };
        if t.rows.is_empty() {
            return Outcome::Unchanged("no rows");
        }
        // A row-level content control partly in the rows would be cut; a cell's has no place in a joined row.
        let spans = super::tables::control_rows(&t.rows);
        let cut =
            t.rows[r0..=r1].iter().flat_map(|r| r.controls.iter()).any(|c| spans.get(c).is_some_and(|&(first, last, _)| first < r0 || last > r1));
        // Row and cell controls, those of the nested tables flattened too: the rows and cells they were read around
        // go, so they are written from their properties around the blocks they hold now.
        let depth = nested.then_some(MAX_FLATTEN);
        let mut moved: Vec<ControlId> = Vec::new();
        let cell_controls = row_and_cell_controls(t, r0..=r1, depth, &mut moved);
        if cut || (cell_controls && sep != Sep::Paragraphs) {
            return self.region_boundary();
        }
        moved.sort_unstable();
        moved.dedup();
        let moved: Vec<(ControlId, Control)> = moved
            .into_iter()
            .filter_map(|id| self.doc.controls.get(&id).filter(|c| !c.open.is_empty()).map(|c| (id, Control::new(c.props.clone()))))
            .collect();
        let mut out: Vec<Block> = Vec::new();
        flatten(t, r0..=r1, &[], sep, depth, &mut out);
        // The rows before and after stay tables.
        let before = (r0 > 0).then(|| {
            let mut x = t.clone();
            x.rows.truncate(r0);
            tidy_merges(&mut x.rows);
            super::clipboard::fresh(&Block::Table(x))
        });
        let after = (r1 + 1 < t.rows.len()).then(|| {
            let mut x = t.clone();
            x.rows.drain(..=r1);
            tidy_merges(&mut x.rows);
            super::clipboard::fresh(&Block::Table(x))
        });
        let mut with: Vec<Block> = before.into_iter().chain(out).chain(after).collect();
        let Some(blocks) = self.index.blocks(&self.doc, slot.container) else { return Outcome::unavailable("not in a table") };
        let like = with.iter().find_map(|b| if let Block::Para(p) = b { Some(p.clone()) } else { None });
        let Some(like) = like else { return Outcome::Unchanged("no text") };
        // Tables side by side join into one, and a story ends in a paragraph; a paragraph put between two blocks is
        // in the content controls both are in.
        let between = |a: Option<&Block>, b: Option<&Block>| {
            let mut p = blank_para(&like);
            let (a, b) = (a.map_or(&[][..], controls), b.map_or(&[][..], controls));
            p.controls = a.iter().zip(b).take_while(|(x, y)| x == y).map(|(x, _)| *x).collect();
            Block::Para(p)
        };
        let prev = slot.block.checked_sub(1).and_then(|k| blocks.get(k));
        if prev.is_some_and(|b| matches!(b, Block::Table(_))) {
            with.insert(0, between(prev, with.first()));
        }
        let mut k = 1;
        while k < with.len() {
            if matches!((&with[k - 1], &with[k]), (Block::Table(_), Block::Table(_))) {
                with.insert(k, between(with.get(k - 1), with.get(k)));
            }
            k += 1;
        }
        let next = blocks.get(slot.block + 1);
        if matches!(with.last(), Some(Block::Table(_))) && next.is_none_or(|b| matches!(b, Block::Table(_))) {
            with.push(between(with.last(), next));
        }
        let story = self.index.story(slot.container);
        let mut tx = self.begin("Convert Table to Text");
        for (id, control) in moved {
            let _ = self.step(&mut tx, Step::Control { id, control: Some(control) });
        }
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("convert failed");
        }
        self.selection = Selection::Caret(Position::text(story, crate::layout::ParaId(like.id), Unit(0)));
        self.commit(tx, None, fonts)
    }
}
