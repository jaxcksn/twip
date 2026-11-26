//! A package from the model alone: the writer against a new document's package, every node unanchored

use std::fmt;
use std::rc::Rc;

use super::super::model::{Block, Document, Inline, Paragraph, Run, RunStyle};
use super::super::para_props::ParaDirect;
use super::super::resolve::Resolver;
use super::super::review::RangeKind;
use super::super::run_xml;
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
/// breaks, their character and paragraph formatting and paragraph style, in the lists the document defines, on a new
/// document's page. Anything else is an error that
/// names it; nothing is left out.
pub fn generate(doc: &Document) -> Result<Vec<u8>, GenerateError> {
    let blank = super::super::read_editable(&super::super::blank::blank_docx()).map_err(GenerateError::Write)?;
    let (Some(src), Some(new_res)) = (blank.source.clone(), blank.resolver.as_deref()) else {
        return Err(GenerateError::Write("the blank package reads without its source".into()));
    };
    let res = doc.resolver.as_deref().ok_or(GenerateError::Unsupported("a document without styles"))?;
    covered(doc, &blank, res, new_res)?;
    let cx = Cx::of(&src);
    let mut section = blank.sections[0].clone();
    section.blocks = doc.sections[0]
        .blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => paragraph(p, res, &cx).map(Block::Para),
            Block::Table(_) => Err(GenerateError::Unsupported("a table")),
        })
        .collect::<Result<_, _>>()?;
    let mut styles = res.styles.clone();
    // Application defaults: empty written ones.
    styles.defaults_raw.get_or_insert_with(|| format!("<{}docDefaults/>", cx.w));
    let res = Resolver { styles, ..res.clone() };
    let out = Document {
        sections: vec![section],
        properties: doc.properties.clone(),
        settings: doc.settings.clone(),
        resolver: Some(Rc::new(res)),
        source: Some(src),
        ..Default::default()
    };
    super::save(&out).map_err(GenerateError::Write)
}

/// The document-wide state generating writes, or keeps as a new document has it: one section on the default page and
/// nothing outside the body.
fn covered(doc: &Document, blank: &Document, res: &Resolver, new_res: &Resolver) -> Result<(), GenerateError> {
    let fail = |what| Err(GenerateError::Unsupported(what));
    let [s] = doc.sections.as_slice() else { return fail("more than one section") };
    let page = &blank.sections[0];
    if s.headers.iter().chain(&s.footers).any(Option::is_some) || !doc.stories.is_empty() {
        return fail("a header or footer");
    }
    if s.change.is_some() {
        return fail("a tracked change");
    }
    if s.page() != page.page() || s.line_grid != page.line_grid || s.rtl_gutter != page.rtl_gutter {
        return fail("a page other than the default");
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

/// `p` unanchored, its style reference written as direct formatting, once its formatting is known to follow from what
/// is written.
fn paragraph(p: &Paragraph, res: &Resolver, cx: &Cx) -> Result<Paragraph, GenerateError> {
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
    let again = crate::docx::para_xml::with(res, ParaDirect::typed(Rc::new(props)), mark, None);
    let mut style = (*p.style).clone();
    style.direct = again.style.direct.clone();
    if !style.layout_eq(&again.style) {
        return fail("paragraph formatting that is not its own");
    }
    let base = run_xml::base(&again.base);
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
fn run(r: &Run, res: &Resolver, base: &crate::format::CharProps, cx: &Cx) -> Result<Run, GenerateError> {
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
