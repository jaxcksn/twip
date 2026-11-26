//! Lists

use std::collections::HashSet;
use std::ops::ControlFlow;
use std::rc::Rc;

use crate::docx::char_props::from_run_props;
use crate::docx::numbering::Counters;
use crate::docx::props::ParaProps;
use crate::docx::resolve::Resolver;
use crate::docx::write::props::Cx;
use crate::docx::{Block, Document, Inline, ListLabel, Paragraph};
use crate::font::Fonts;
use crate::format::{ListChoice, ParaEdit, ParaProp};
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::index::DocIndex;
use super::selection::Selection;
use super::step::{Step, para_len};
use super::{Editor, Tx};
use crate::document::paragraph::{ListId, ListRef};
use crate::docx::run_xml;
use crate::format::CharProps;

impl Editor {
    pub(super) fn list_item_start(&self) -> Option<(ParaId, ListRef)> {
        let Selection::Caret(c) = self.selection else { return None };
        let (para, off) = c.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        (off == Unit(0)).then_some(())?;
        p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0).map(|n| (para, n))
    }

    pub(super) fn repara(&mut self, tx: &mut Tx, para: ParaId, edit: &dyn Fn(&mut ParaEdit)) -> bool {
        let Some(res) = self.doc.resolver.clone() else { return false };
        let cx = self.cx();
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let mut e = ParaEdit::new((**p.style.direct.get()).clone());
        edit(&mut e);
        let direct = p.style.direct.edit(e);
        let Some(r) = self.para_again(para, Some(direct), &res, &cx) else { return false };
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let (mark, list) = (p.mark.clone(), p.list.clone());
        tx.stale_paras.push(para);
        self.step(tx, Step::Para { para, style: Rc::new(r.style), mark, list }).is_ok()
    }

    pub(super) fn list_enter(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let (para, _) = self.list_item_start()?;
        if self.index.para(&self.doc, para).map(para_len) != Some(Unit(0)) {
            return None;
        }
        let mut tx = self.begin("Numbering");
        let ok = self.repara(&mut tx, para, &|e| {
            in_list(e, 0, 0);
            indents(e, Some(0.0), Some(0.0));
        });
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("numbering failed"));
        }
        Some(self.commit(tx, None, fonts))
    }

    pub(super) fn list_backspace(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let Selection::Caret(c) = self.selection else { return None };
        let (para, off) = c.para_offset()?;
        if off != Unit(0) {
            return None;
        }
        let p = self.index.para(&self.doc, para)?;
        let left = p.style.ind_left;
        let mut tx = self.begin("Paragraph Formatting");
        let ok = if p.style.num.is_some_and(|ListRef { list: ListId(id), level: _ }| id != 0) {
            let l = twips(left);
            self.repara(&mut tx, para, &|e| {
                in_list(e, 0, 0);
                indents(e, Some(l), Some(0.0));
            })
        } else {
            let d = p.style.direct.get();
            let direct_ind = d.indent_left.is_some() || d.indent_right.is_some() || d.indent_first.is_some() || written(d, "ind");
            let first = p.style.ind_first;
            if !self.indent_keys || !direct_ind || !((left > 0.0 && first >= 0.0) || first > 0.0) {
                self.abort(tx);
                return None;
            }
            // Back the way Tab came: the left indent a tab stop back, then the first line's indent.
            if first > 0.0 {
                let tab = self.tab_grid();
                let back = (left > 0.0).then(|| twips(super::format::grid_step(left, tab, false)));
                self.repara(&mut tx, para, &|e| match back {
                    Some(l) => {
                        e.indent_left = Some(l);
                        e.part(ParaProp::IndentLeft);
                    }
                    None => {
                        e.indent_first = Some(0.0);
                        e.part(ParaProp::IndentFirst);
                    }
                })
            } else {
                let l = twips((((left - 0.01) / 18.0).floor() * 18.0).max(0.0));
                self.repara(&mut tx, para, &|e| indents(e, Some(l), Some(0.0)))
            }
        };
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("formatting failed"));
        }
        Some(self.commit(tx, None, fonts))
    }

    pub(super) fn list_items(&self, paras: &[ParaId]) -> Vec<(ParaId, ListRef)> {
        let num =
            |&para: &ParaId| Some((para, self.index.para(&self.doc, para)?.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?));
        paras.iter().filter_map(num).collect()
    }

    pub(super) fn tab_changes_level(&mut self) -> bool {
        if self.list_item_start().is_some() {
            return true;
        }
        if !matches!(self.selection, Selection::Text { .. }) {
            return false;
        }
        let paras = self.selected_paras();
        paras.len() > 1 && self.list_items(&paras).len() == paras.len()
    }

    pub(super) fn list_level(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        let paras = self.selected_paras();
        let items = self.list_items(&paras);
        if items.is_empty() {
            return Outcome::Unchanged("not in a list");
        }
        let mut tx = self.begin("Numbering");
        let mut changed = false;
        for (para, ListRef { list: ListId(num), level: ilvl }) in items {
            let to = if back { ilvl - 1 } else { ilvl + 1 };
            if !(0..=8).contains(&to) {
                continue;
            }
            if !self.repara(&mut tx, para, &|e| in_list(e, num, to)) {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
            changed = true;
        }
        if !changed {
            self.abort(tx);
            return Outcome::Unchanged("no level there");
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn toggle_list(&mut self, bullet: bool, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let paras = self.selected_paras();
        let Some(&first) = paras.first() else { return Outcome::Unchanged("no paragraph") };
        let kind = |e: &Self, para: ParaId| -> Option<(bool, (i64, i64))> {
            let p = e.index.para(&e.doc, para)?;
            let ListRef { list: ListId(id), level: ilvl } = p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?;
            let r = e.doc.resolver.as_ref()?;
            let level = r.numbering.level(id, ilvl, &r.styles)?;
            Some((level.fmt == "bullet", (id, ilvl)))
        };
        let label = if bullet { "Bullets" } else { "Numbering" };
        if paras.iter().all(|&p| kind(self, p).is_some_and(|(b, _)| b == bullet)) {
            let mut tx = self.begin(label);
            for &para in &paras {
                let Some((_, (id, ilvl))) = kind(self, para) else { continue };
                let lvl = res.numbering.level(id, ilvl, &res.styles);
                let left = lvl.and_then(|l| l.ppr.ind_left).unwrap_or(36.0) + lvl.and_then(|l| l.ppr.ind_first).unwrap_or(-18.0).min(0.0);
                let style_id = self.index.para(&self.doc, para).and_then(|p| p.style.style_id.clone());
                let ok = self.repara(&mut tx, para, &|e| {
                    let list_paragraph = e.style.as_deref() == Some("ListParagraph");
                    if list_paragraph {
                        e.style = None;
                        e.whole(ParaProp::Style);
                    }
                    let styled = !list_paragraph && (e.style.is_some() || written(e, "pStyle"));
                    let style = res.styles.paragraph_style(if styled { style_id.as_deref() } else { None });
                    let numbered = res.styles.chain_ppr(style).num_id.is_some_and(|n| n != 0);
                    if (e.list.is_some() || e.list_level.is_some() || written(e, "numPr")) && !numbered {
                        (e.list, e.list_level) = (None, None);
                        e.whole(ParaProp::List);
                    } else {
                        in_list(e, 0, 0);
                    }
                    indents(e, Some(twips(left)), None);
                });
                if !ok {
                    self.abort(tx);
                    return Outcome::Unchanged("numbering failed");
                }
            }
            return self.commit(tx, None, fonts);
        }
        let before = self.index.para_slot(first).and_then(|s| {
            let blocks = self.index.blocks(&self.doc, s.container)?;
            match blocks.get(s.block.checked_sub(1)?)? {
                Block::Para(q) => Some(ParaId(q.id)),
                Block::Table(_) => None,
            }
        });
        let mut tx = self.begin(label);
        let (num, ilvl) = match before.and_then(|q| kind(self, q)).filter(|(b, _)| *b == bullet) {
            Some((_, n)) => n,
            None => (self.new_list(&mut tx, bullet), 0),
        };
        let list_style = res.styles.by_id.contains_key("ListParagraph");
        for &para in &paras {
            // A paragraph in a list keeps its level, where the list has it.
            let defined = |l: i64| self.doc.resolver.as_ref().is_some_and(|r| r.numbering.level(num, l, &r.styles).is_some());
            let ilvl = kind(self, para).map(|(_, (_, own))| own.clamp(0, 8)).filter(|&own| defined(own)).unwrap_or(ilvl);
            let ok = self.repara(&mut tx, para, &|e| {
                in_list(e, num, ilvl);
                indents(e, None, None);
                if list_style && !(e.style.is_some() || written(e, "pStyle")) {
                    e.style = Some("ListParagraph".into());
                    e.whole(ParaProp::Style);
                }
            });
            if !ok {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn new_list(&mut self, tx: &mut Tx, bullet: bool) -> i64 {
        let mut levels = String::new();
        for k in 0..9 {
            let left = 720 * (k + 1);
            let (fmt, text, jc, hanging, font) = if bullet {
                match k % 3 {
                    0 => ("bullet", "\u{F0B7}".to_string(), "left", 360, Some("Symbol")),
                    1 => ("bullet", "o".to_string(), "left", 360, Some("Courier New")),
                    _ => ("bullet", "\u{F0A7}".to_string(), "left", 360, Some("Wingdings")),
                }
            } else {
                match k % 3 {
                    0 => ("decimal", format!("%{}.", k + 1), "left", 360, None),
                    1 => ("lowerLetter", format!("%{}.", k + 1), "left", 360, None),
                    _ => ("lowerRoman", format!("%{}.", k + 1), "right", 180, None),
                }
            };
            let rpr = font.map_or(String::new(), |f| format!(r#"<w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:hint="default"/></w:rPr>"#));
            levels.push_str(&format!(
                r#"<w:lvl w:ilvl="{k}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="{jc}"/><w:pPr><w:ind w:left="{left}" w:hanging="{hanging}"/></w:pPr>{rpr}</w:lvl>"#
            ));
        }
        self.add_list_xml(tx, "hybridMultilevel", &levels)
    }

    /// The single-level bullet list Word gives the List Bullet style, linked to style `style`.
    pub(super) fn new_style_bullets(&mut self, tx: &mut Tx, style: &str) -> i64 {
        let level = format!(
            r#"<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:pStyle w:val="{}"/><w:lvlText w:val="{}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:hint="default"/></w:rPr></w:lvl>"#,
            crate::docx::write::props::esc(style),
            '\u{F0B7}'
        );
        self.add_list_xml(tx, "singleLevel", &level)
    }

    /// Adds a list of `levels` (`w:lvl` elements), returning its number id; 0 when it couldn't be added.
    fn add_list_xml(&mut self, tx: &mut Tx, multi: &str, levels: &str) -> i64 {
        let Some(res) = self.doc.resolver.clone() else { return 0 };
        let abstract_id = res.numbering.abstracts.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.abstract_id)).max().unwrap_or(-1) + 1;
        let num_id = res.numbering.nums.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.num_id)).max().unwrap_or(0) + 1;
        let abstract_xml = format!(
            r#"<w:abstractNum xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:abstractNumId="{abstract_id}"><w:multiLevelType w:val="{multi}"/>{levels}</w:abstractNum>"#
        );
        let part = format!(
            r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{abstract_xml}<w:num xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:numId="{num_id}"><w:abstractNumId w:val="{abstract_id}"/></w:num></w:numbering>"#
        );
        let parsed = crate::docx::numbering::Numbering::parse(Some(&part), &res.styles.colours);
        let mut r = (*res).clone();
        r.numbering.abstracts.extend(parsed.abstracts);
        r.numbering.nums.extend(parsed.nums);
        let mut added = self.doc.added_lists.clone();
        added.push(crate::docx::AddedList { abstract_id, num_id, abstract_xml });
        if self.step(tx, Step::Resolver { resolver: Some(Rc::new(r)), added }).is_err() {
            return 0;
        }
        num_id
    }

    pub(super) fn renumber(&mut self, tx: &mut Tx) {
        let touched = self.touched_paras(tx);
        let all = tx.relabel;
        for (para, list) in self.labels_changed(|p| all || touched.contains(&p)) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let (style, mark) = (p.style.clone(), p.mark.clone());
            let _ = self.step(tx, Step::Para { para, style, mark, list });
        }
    }

    /// The paragraphs a transaction formatted or put in: their labels are made again in full.
    fn touched_paras(&self, tx: &Tx) -> HashSet<ParaId> {
        let mut out = HashSet::new();
        for s in &tx.undo {
            match s {
                Step::Para { para, .. } => {
                    out.insert(*para);
                }
                Step::Join { second, .. } => {
                    out.insert(*second);
                }
                Step::Blocks { container, range, .. } => {
                    let blocks = self.index.blocks(&self.doc, *container).and_then(|b| b.get(range.clone())).unwrap_or_default();
                    let mut paras = Vec::new();
                    walk(blocks, &mut paras);
                    out.extend(paras.into_iter().map(|(p, _)| ParaId(p.id)));
                }
                _ => {}
            }
        }
        out
    }

    /// What paragraph `p`'s list number is formatted over, its level's own formatting aside: the paragraph's style
    /// chain and its mark's formatting.
    pub(super) fn label_base(&self, para: ParaId, p: &Paragraph, res: &Resolver, cx: &Cx) -> CharProps {
        let region = self.region_of_para(para, res);
        let base = crate::docx::para_xml::of(res, &p.style.direct, region.as_ref(), cx).base;
        run_xml::folded(res, &run_xml::base(&base), p.mark.direct.props().map(|m| &**m), false)
    }

    /// The labels that change: those whose text changed, or, for the paragraphs `full` names, any part of them.
    pub(super) fn labels_changed(&self, full: impl Fn(ParaId) -> bool) -> Vec<(ParaId, Option<ListLabel>)> {
        let Some(res) = self.doc.resolver.clone() else { return Vec::new() };
        if res.numbering.nums.is_empty() {
            return Vec::new();
        }
        let cx = self.cx();
        let mut counters = Counters::default();
        let mut changes: Vec<(ParaId, Option<ListLabel>)> = Vec::new();
        for (p, uncounted) in numbered_paras(&self.doc, &self.index) {
            let para = ParaId(p.id);
            let label = counted(&res, p, uncounted, &cx)
                .and_then(|(id, ilvl)| counters.next(&res.numbering, &res.styles, id, ilvl).map(|text| (id, ilvl, text)));
            let want = match (&label, &p.list) {
                (None, None) => continue,
                (Some((_, _, text)), Some(have)) if *text == have.text && !full(para) => continue,
                (None, Some(_)) => None,
                (Some((id, ilvl, text)), _) => {
                    let Some(level) = res.numbering.level(*id, *ilvl, &res.styles).cloned() else { continue };
                    let under = self.label_base(para, p, &res, &cx);
                    let picture = res.numbering.bullet_image(&level);
                    let label = ListLabel {
                        text: text.clone(),
                        run: run_xml::label(&res, &under, Some(&from_run_props(&level.rpr))),
                        jc: crate::docx::paragraph::list_jc(&level.jc),
                        suff: crate::docx::paragraph::list_suffix(&level.suff),
                        picture,
                        legacy: level.legacy,
                    };
                    if p.list.as_ref() == Some(&label) {
                        continue;
                    }
                    Some(label)
                }
            };
            if self.index.para_slot(para).is_some() {
                changes.push((para, want));
            }
        }
        changes
    }
}

/// Puts a paragraph in list `num` (0: none) at level `ilvl`.
pub(super) fn in_list(e: &mut ParaEdit, num: i64, ilvl: i64) {
    e.list = Some(if num == 0 { ListChoice::None } else { ListChoice::In(ListId(num)) });
    e.list_level = Some(ilvl);
    e.whole(ParaProp::List);
}

/// Sets a paragraph's indents whole: its start and first line's, no end indent.
fn indents(e: &mut ParaEdit, left: Option<f64>, first: Option<f64>) {
    (e.indent_left, e.indent_right, e.indent_first) = (left, None, first);
    e.whole(ParaProp::IndentLeft);
}

/// Points to the nearest twip, zero unsigned.
pub(super) fn twips(pt: f64) -> f64 {
    super::format::twip(pt) + 0.0
}

/// Whether authored properties `p` hold child `local` of their `w:pPr` as written: one they don't model, or one kept
/// beside its value.
pub(super) fn written(p: &crate::format::ParaProps, local: &str) -> bool {
    p.foreign.as_deref().is_some_and(|f| {
        let named = |c: &&crate::format::ForeignChild| &*c.name.local == local && matches!(&*c.name.ns, "" | crate::docx::xml::W);
        f.children.iter().chain(&f.kept).any(|c| named(&c))
    })
}

/// The list and level a paragraph's label counts in, when it shows one.
pub(super) fn counted(res: &Resolver, p: &Paragraph, uncounted: bool, cx: &Cx) -> Option<(i64, i64)> {
    let ListRef { list: ListId(id), level: ilvl } = p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0 && !uncounted)?;
    let from_style = !direct_num(crate::docx::para_xml::ppr(Some(res), &p.style.direct, cx).as_deref(), cx);
    res.label_shows(id, ilvl, from_style, p.style.style_id.as_deref()).then_some((id, ilvl))
}

/// Whether a paragraph's direct `w:pPr` sets a `w:numId` itself (one in its tracked earlier properties doesn't).
pub(super) fn direct_num(ppr: Option<&str>, cx: &Cx) -> bool {
    let Some(x) = ppr else { return false };
    let b = x.as_bytes();
    // A `numId` start tag
    let tag = x.match_indices("numId").any(|(i, _)| {
        matches!(i.checked_sub(1).map(|k| b[k]), Some(b':' | b'<'))
            && b.get(i + 5).is_some_and(|c| matches!(c, b'/' | b'>') || c.is_ascii_whitespace())
    });
    tag && (!(x.contains("pPrChange") || x.contains("<!--"))
        || cx.parse(x, |_, el| ParaProps::parse(Some(el), &Default::default()).num_id.is_some()).unwrap_or(false))
}

/// Each paragraph in `blocks` and the text boxes anchored in them, as the reader reaches them.
fn walk<'d>(blocks: &'d [Block], out: &mut Vec<(&'d Paragraph, bool)>) {
    let _ = visit(blocks, &mut |p| {
        out.push((p, false));
        ControlFlow::<()>::Continue(())
    });
}

/// [`walk`] until `f` breaks.
fn visit<'d, B>(blocks: &'d [Block], f: &mut impl FnMut(&'d Paragraph) -> ControlFlow<B>) -> ControlFlow<B> {
    for b in blocks {
        match b {
            Block::Para(p) => {
                f(p)?;
                for i in &p.inlines {
                    match i {
                        Inline::Float(fl, _) => {
                            for t in fl.text_box.iter().chain(fl.parts.iter().filter_map(|g| g.text_box.as_ref())) {
                                visit(&t.blocks, f)?;
                            }
                        }
                        _ => {
                            if let Some((_, t)) = i.story() {
                                visit(&t.blocks, f)?;
                            }
                        }
                    }
                }
            }
            Block::Table(t) => {
                for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    visit(&c.blocks, f)?;
                }
            }
        }
    }
    ControlFlow::Continue(())
}

/// Whether a paragraph holds anything but marks: an empty one that ends a section is left out of its list.
fn has_content(p: &Paragraph) -> bool {
    p.inlines.iter().any(|i| match i {
        Inline::Text(s, _) => !s.is_empty(),
        Inline::Tab(_)
        | Inline::PTab(..)
        | Inline::Break(..)
        | Inline::Sym { .. }
        | Inline::Picture(..)
        | Inline::Float(..)
        | Inline::NoteRef { .. }
        | Inline::FieldBegin { .. }
        | Inline::Ruby(..) => true,
        _ => false,
    })
}

/// Every paragraph in the order lists number through them as the document is read: the body (a text box's story
/// after its anchor's paragraph), the headers and footers, the notes, the comments, the building blocks. Each with
/// whether the reader leaves it uncounted: an empty paragraph that ends a section after another paragraph.
pub(super) fn numbered_paras<'d>(doc: &'d Document, idx: &DocIndex) -> Vec<(&'d Paragraph, bool)> {
    let mut out = Vec::new();
    for (ends, blocks) in numbered_stories(doc, idx) {
        walk(blocks, &mut out);
        if ends
            && matches!(blocks, [.., Block::Para(_), Block::Para(p)] if !has_content(p))
            && let Some(last) = out.last_mut()
        {
            last.1 = true;
        }
    }
    out
}

/// The first paragraph of list `num` in the order lists number through them.
pub(super) fn first_numbered(doc: &Document, idx: &DocIndex, num: i64) -> Option<ParaId> {
    numbered_stories(doc, idx).find_map(|(_, blocks)| {
        visit(blocks, &mut |p| match p.style.num {
            Some(ListRef { list: ListId(id), level: _ }) if id == num => ControlFlow::Break(ParaId(p.id)),
            _ => ControlFlow::Continue(()),
        })
        .break_value()
    })
}

/// The stories of [`numbered_paras`] in order, each with whether it is a section another follows.
fn numbered_stories<'d>(doc: &'d Document, idx: &DocIndex) -> impl Iterator<Item = (bool, &'d [Block])> {
    let ended = doc.sections.len().saturating_sub(1);
    let comments = doc.comments.by_id.values().map(|c| c.body.as_slice());
    let glossary = doc.glossary.iter().flat_map(|g| g.parts.iter()).filter(|p| p.state != crate::docx::PartState::Removed).map(|p| p.body.as_slice());
    let stories = super::elements::stories(doc, idx).into_iter().enumerate().map(move |(k, (_, b))| (k < ended, b));
    stories.chain(comments.chain(glossary).map(|b| (false, b)))
}
