//! A package from the model alone: the writer against a new document's package, every node unanchored

use crate::docx::resolvers::Resolvers;
use std::fmt;
use std::rc::Rc;

use super::super::model::{Block, Cell, Document, Inline, Paragraph, Row, Run, RunStyle, Section, Table};
use super::super::para_props::ParaDirect;
use super::super::resolve::Resolver;
use super::super::review::RangeKind;
use super::super::run_xml;
use super::super::styles::Region;
use super::super::table_props::{CellDirect, ExceptionsDirect, RowDirect, TableDirect};
use super::props::Cx;

/// What [`generate`] cannot write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerateError {
    /// Content, formatting or a part of the document that generating does not cover, named.
    Unsupported(&'static str),
    /// The writer failed.
    Write(String),
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenerateError::Unsupported(what) => write!(f, "cannot generate {what}"),
            GenerateError::Write(e) => write!(f, "cannot write the package: {e}"),
        }
    }
}

impl std::error::Error for GenerateError {}

/// `doc` as a new package, written from the model alone, whatever it was read from: body paragraphs of text, tabs and
/// breaks, their character and paragraph formatting and paragraph style, in the lists the document defines, tables of
/// them (their table style, table, row and cell properties), on one section's page setup. Anything else is an error
/// that names it; nothing is left out.
pub fn generate(doc: &Document) -> Result<Vec<u8>, GenerateError> {
    let blank = super::super::read_editable(&super::super::blank::blank_docx()).map_err(GenerateError::Write)?;
    let (Some(src), Some(new_res)) = (blank.source.clone(), blank.resolver.as_deref()) else {
        return Err(GenerateError::Write("the blank package reads without its source".into()));
    };
    let res = doc.resolver.as_deref().ok_or(GenerateError::Unsupported("a document without styles"))?;
    covered(doc, &blank, res, new_res)?;
    let cx = Cx::of(&src);
    let section = page(&doc.sections[0], &blank.sections[0], blocks(&doc.sections[0].blocks, res, &cx, None)?)?;
    let mut styles = res.styles.clone();
    // Application defaults: empty written ones.
    styles.defaults_raw.get_or_insert_with(|| format!("<{}docDefaults/>", cx.w));
    let res = Resolver { styles, ..(**res).clone() };
    let edited = section.read.as_deref().is_some_and(|was| *was != section.page())
        || (section.line_grid, section.rtl_gutter) != (blank.sections[0].line_grid, blank.sections[0].rtl_gutter);
    let out = Document {
        sections: vec![section],
        properties: doc.properties.clone(),
        settings: doc.settings.clone(),
        resolver: Some(Rc::new(res.into())),
        source: Some(src),
        edited_sections: if edited { [0].into() } else { Default::default() },
        ..Default::default()
    };
    let bytes = super::save(&out).map_err(GenerateError::Write)?;
    reads_back(&out.sections[0], &bytes)?;
    Ok(bytes)
}

/// Body content unanchored: paragraphs (in `region` when they are in a cell) and tables.
fn blocks(blocks: &[Block], res: &Resolvers, cx: &Cx, region: Option<&Region>) -> Result<Vec<Block>, GenerateError> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => paragraph(p, res, cx, region).map(Block::Para),
            Block::Table(t) => table(t, res, cx).map(Block::Table),
        })
        .collect()
}

/// `t` unanchored: its table style reference among its own properties, which the writer writes, as it does a new
/// table's, from its resolved properties; its rows' properties, exceptions and cells' properties as their typed
/// properties write them; its cells' content in their table style regions.
fn table(t: &Table, res: &Resolvers, cx: &Cx) -> Result<Table, GenerateError> {
    let fail = |what| Err(GenerateError::Unsupported(what));
    if t.change.is_some() || t.grid_change.is_some() {
        return fail("a tracked change");
    }
    if !t.controls.is_empty() {
        return fail("a content control");
    }
    let mut own = (**t.direct.get()).clone();
    if own.style.is_none() {
        own.style = t.style_id.as_deref().map(Into::into);
    }
    let mut out = Table { src: None, direct: TableDirect::typed(Rc::new(own)), ..t.clone() };
    for (ri, row) in out.rows.iter_mut().enumerate() {
        if row.rev.ins.is_some() || row.rev.del.is_some() || row.rev.change.is_some() || row.rev.ex_change.is_some() {
            return fail("a tracked change");
        }
        if !row.controls.is_empty() {
            return fail("a content control");
        }
        if row.div.is_some() {
            return fail("an HTML division");
        }
        *row = Row {
            src: None,
            direct: RowDirect::typed(row.direct.get().clone()),
            exceptions: ExceptionsDirect::typed(row.exceptions.get().clone()),
            ..row.clone()
        };
        for (ci, cell) in row.cells.iter_mut().enumerate() {
            if cell.rev.structural.is_some() || cell.rev.change.is_some() {
                return fail("a tracked change");
            }
            if !cell.controls.is_empty() {
                return fail("a content control");
            }
            let region = res.region_of(t, ri, ci);
            let content = blocks(&cell.blocks, res, cx, Some(&region))?;
            // Its own properties, an empty element for none: the writer adds its width, span and merge to them.
            let props = cell.direct.get();
            let direct = if **props == crate::format::CellProps::default() {
                CellDirect::from_xml(Some(&format!("<{}tcPr/>", cx.w)), cx)
            } else {
                CellDirect::typed(props.clone())
            };
            *cell = Cell { direct, blocks: content, ..cell.clone() };
        }
    }
    Ok(out)
}

/// `s`'s page setup and `blocks` on the new document's section `blank`: written as an edit of its page.
fn page(s: &Section, blank: &Section, blocks: Vec<Block>) -> Result<Section, GenerateError> {
    let fail = |what| Err(GenerateError::Unsupported(what));
    if s.line_grid.is_some_and(|g| super::line_grid_of(super::line_pitch(g)) != g) {
        return fail("a line grid");
    }
    let borders = super::super::page_borders::PageBordersDirect::typed(s.page_borders_direct.get().map(|b| (**b).clone()));
    Ok(Section {
        blocks,
        page_borders_direct: borders,
        headers: blank.headers.clone(),
        footers: blank.footers.clone(),
        change: None,
        read: Some(Rc::new(blank.page())),
        ..s.clone()
    })
}

/// The package `bytes` reads back with `s`'s page setup and its tables as they are.
fn reads_back(s: &Section, bytes: &[u8]) -> Result<(), GenerateError> {
    let back = super::super::read_editable(bytes).map_err(GenerateError::Write)?;
    let [b] = back.sections.as_slice() else { return Err(GenerateError::Write("the package reads back without its section".into())) };
    // The page borders by their typed value: what is read back holds the element written too
    let typed = |s: &Section| {
        let mut p = s.page();
        p.page_borders_direct = super::super::page_borders::PageBordersDirect::typed(p.page_borders_direct.get().map(|b| (**b).clone()));
        p
    };
    if typed(b) != typed(s) || (b.line_grid, b.rtl_gutter) != (s.line_grid, s.rtl_gutter) {
        return Err(GenerateError::Unsupported("a page setup that reads back otherwise"));
    }
    if !same_tables(&s.blocks, &b.blocks) {
        return Err(GenerateError::Unsupported("table formatting that is not its own"));
    }
    Ok(())
}

/// Whether the tables among `a` and `b` (as many blocks) lay out alike, their cells' tables too: every resolved field
/// but the identity of what they were read from, and of their paragraphs, whose formatting is checked as they are
/// written.
fn same_tables(a: &[Block], b: &[Block]) -> bool {
    let bare = |t: &Table| Table {
        src: None,
        id: 0,
        rows: t
            .rows
            .iter()
            .map(|r| Row { src: None, cells: r.cells.iter().map(|c| Cell { blocks: Vec::new(), ..c.clone() }).collect(), ..r.clone() })
            .collect(),
        ..t.clone()
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (Block::Para(_), Block::Para(_)) => true,
            (Block::Table(x), Block::Table(y)) => {
                bare(x) == bare(y)
                    && x.rows.iter().zip(&y.rows).all(|(r, q)| r.cells.iter().zip(&q.cells).all(|(c, d)| same_tables(&c.blocks, &d.blocks)))
            }
            _ => false,
        })
}

/// The document-wide state generating writes, or keeps as a new document has it: one section, with no headers or
/// footers, and nothing outside the body.
fn covered(doc: &Document, blank: &Document, res: &Resolvers, new_res: &Resolvers) -> Result<(), GenerateError> {
    let fail = |what| Err(GenerateError::Unsupported(what));
    let [s] = doc.sections.as_slice() else { return fail("more than one section") };
    if s.headers.iter().chain(&s.footers).any(Option::is_some) || !doc.stories.is_empty() {
        return fail("a header or footer");
    }
    if s.change.is_some() {
        return fail("a tracked change");
    }
    if !doc.footnotes.is_empty() || !doc.endnotes.is_empty() || !doc.separators.is_empty() {
        return fail("a note");
    }
    if !doc.comments.by_id.is_empty() {
        return fail("a comment");
    }
    if !doc.controls.is_empty() {
        return fail("a content control");
    }
    if doc.glossary.is_some() {
        return fail("a glossary");
    }
    if !doc.alt_chunks.is_empty() {
        return fail("imported content");
    }
    if !doc.data_parts.is_empty() {
        return fail("a custom XML part");
    }
    if doc.page_colour.is_some() || doc.page_fill.is_some() {
        return fail("a page background");
    }
    if res.styles.colours != new_res.styles.colours || res.theme != new_res.theme {
        return fail("a theme");
    }
    // Settings by their debug form, which shows every field; the styles' rules that follow from them.
    let rules = |s: &super::super::styles::Styles| (s.compat, s.word2002_rules, s.cell_size_rules, s.open_type);
    if format!("{:?}", doc.settings) != format!("{:?}", blank.settings) || rules(&res.styles) != rules(&new_res.styles) {
        return fail("document settings");
    }
    Ok(())
}

/// `p` unanchored (in `region` when it is in a cell), its style reference written as direct formatting, once its
/// formatting is known to follow from what is written.
fn paragraph(p: &Paragraph, res: &Resolvers, cx: &Cx, region: Option<&Region>) -> Result<Paragraph, GenerateError> {
    let fail = |what| Err(GenerateError::Unsupported(what));
    if p.style.num.is_some_and(|n| !res.numbering.nums.contains_key(&n.list.0)) {
        return fail("a list without its definition");
    }
    if p.div.is_some() {
        return fail("an HTML division");
    }
    if !p.controls.is_empty() {
        return fail("a content control");
    }
    if p.style.change.is_some() {
        return fail("a tracked change");
    }
    let mut props = (**p.style.direct.get()).clone();
    if props.frame.is_some() || props.frame_locked.is_some() {
        return fail("a frame");
    }
    if props.style.is_none()
        && let Some(id) = p.style.style_id.as_deref().filter(|id| Some(*id) != res.styles.default_paragraph.as_deref())
    {
        props.style = Some(id.into());
    }
    let mark = p.mark.direct.props().and_then(|m| m.size);
    let again = crate::docx::para_xml::with(res, ParaDirect::typed(Rc::new(props)), mark, region);
    let mut style = (*p.style).clone();
    style.direct = again.style.direct.clone();
    if !style.layout_eq(&again.style) {
        return fail("paragraph formatting that is not its own");
    }
    let base = again.base.clone();
    let inlines = p
        .inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t, r) => Ok(Inline::Text(t.clone(), run(r, res, &base, cx)?)),
            Inline::Tab(r) => Ok(Inline::Tab(run(r, res, &base, cx)?)),
            Inline::Break(k, r) => Ok(Inline::Break(*k, run(r, res, &base, cx)?)),
            other => Err(GenerateError::Unsupported(unsupported(other))),
        })
        .collect::<Result<_, _>>()?;
    Ok(Paragraph {
        style: Rc::new(again.style),
        mark: run(&p.mark, res, &base, cx)?,
        list: p.list.clone(),
        inlines,
        src: None,
        id: p.id,
        controls: Vec::new(),
        div: None,
    })
}

/// `r` with its authored properties written where `cx` describes, once its formatting is known to follow from them
/// over `base`.
fn run(r: &Run, res: &Resolvers, base: &crate::format::CharProps, cx: &Cx) -> Result<Run, GenerateError> {
    if !r.rev.is_empty() {
        return Err(GenerateError::Unsupported("a tracked change"));
    }
    if !run_xml::resolve(res, base, r.direct.clone(), r.toc).layout_eq(r) {
        return Err(GenerateError::Unsupported("character formatting that is not its own"));
    }
    Ok(match r.direct.moved_to(cx) {
        Some(direct) => Rc::new(RunStyle { direct, ..(**r).clone() }),
        None => r.clone(),
    })
}

/// What an inline generating does not cover is.
fn unsupported(i: &Inline) -> &'static str {
    match i {
        Inline::Text(..) | Inline::Tab(_) | Inline::Break(..) => "text",
        Inline::PTab(..) => "an absolute tab",
        Inline::Sym { .. } => "a symbol",
        Inline::Picture(..) => "a picture",
        Inline::Float(..) => "a floating object",
        Inline::NoteRef { .. } | Inline::NoteMark(..) | Inline::Separator { .. } => "a note",
        Inline::FieldBegin { .. } | Inline::FieldSep { .. } | Inline::FieldEnd { .. } => "a field",
        Inline::Bookmark(_) => "a bookmark",
        Inline::Range(r) if matches!(r.kind, RangeKind::CommentStart | RangeKind::CommentEnd) => "a comment",
        Inline::Range(_) => "a tracked move",
        Inline::CommentRef { .. } => "a comment",
        Inline::ControlStart(_) | Inline::ControlEnd(_) => "a content control",
        Inline::Ruby(..) => "ruby text",
        Inline::DateBlock(..) => "a date",
        Inline::Opaque(_) => "content kept as read",
    }
}
