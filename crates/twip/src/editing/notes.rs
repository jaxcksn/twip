//! Footnotes and endnotes

use std::rc::Rc;

use crate::docx::props::RunProps;
use crate::docx::resolve::Resolver;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, Inline, NoteKind, Paragraph, Run};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::position::{Place, Position, StoryId};
use super::selection::Selection;
use super::step::Step;
use super::{Editor, Tx};

fn story(kind: NoteKind, id: i64) -> StoryId {
    match kind {
        NoteKind::Foot => StoryId::Footnote(id),
        NoteKind::End => StoryId::Endnote(id),
    }
}

pub(super) fn refs_in(blocks: &[Block], out: &mut Vec<(NoteKind, i64)>) {
    for b in blocks {
        match b {
            Block::Para(p) => refs_inline(&p.inlines, out),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| refs_in(&c.blocks, out)),
        }
    }
}

/// A kind's reference and text styles.
fn note_styles(kind: NoteKind) -> (&'static str, &'static str) {
    match kind {
        NoteKind::Foot => ("FootnoteReference", "FootnoteText"),
        NoteKind::End => ("EndnoteReference", "EndnoteText"),
    }
}

/// `run` in `to`'s reference style (superscript where the document has none), when it is in `from`'s.
fn restyled_ref(res: &Resolver, cx: &Cx, base: &RunProps, run: &Run, from: NoteKind, to: NoteKind) -> Option<Run> {
    let (old, new) = (note_styles(from).0, note_styles(to).0);
    let mut p = Props::parse(run.direct.0.as_deref(), "rPr", cx);
    if p.get("rStyle").and_then(|s| crate::docx::write::props::attr_of(s, "val")).as_deref() != Some(old) {
        return None;
    }
    let w = &cx.w;
    if res.styles.by_id.contains_key(new) {
        p.set("rStyle", format!("<{w}rStyle {w}val=\"{new}\"/>"));
    } else {
        p.remove("rStyle");
        p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
    }
    Some(res.run_from_xml(base, p.xml().as_deref(), cx))
}

/// A converted note's paragraph in `to`'s text style when it is in `from`'s, and its marks in `to`'s reference
/// style; whether it changed.
fn restyle_note_para(res: &Resolver, cx: &Cx, p: &mut Paragraph, from: NoteKind, to: NoteKind) -> bool {
    let (old, new) = (note_styles(from).1, note_styles(to).1);
    let mut changed = false;
    if p.style.style_id.as_deref() == Some(old) {
        let mut pp = Props::parse(p.style.direct.0.as_deref(), "pPr", cx);
        if res.styles.by_id.contains_key(new) {
            pp.set("pStyle", format!("<{w}pStyle {w}val=\"{new}\"/>", w = cx.w));
        } else {
            pp.remove("pStyle");
        }
        p.style = Rc::new(res.para_from_xml(pp.xml().as_deref(), None, cx).style);
        changed = true;
    }
    let base = res.para_from_xml(p.style.direct.0.as_deref(), None, cx).base;
    for i in &mut p.inlines {
        if let Some(run) = i.run().and_then(|r| restyled_ref(res, cx, &base, r, from, to)) {
            *i = super::format::with_run(i, &run);
            changed = true;
        }
    }
    changed
}

/// A note id `taken` doesn't hold: the one after `max`, or, when that is past the `w:id` range Word reads, the lowest
/// free one from `lo`.
pub(super) fn free_note_id(max: i64, lo: i64, taken: impl Fn(i64) -> bool) -> i64 {
    match max.max(0).checked_add(1).filter(|&n| n <= i64::from(i32::MAX)) {
        Some(n) => n,
        None => (lo.max(1)..).find(|&n| !taken(n)).unwrap_or(1),
    }
}

/// Each paragraph of `blocks`, its tables' cells' included.
fn each_para(blocks: &mut [Block], f: &mut dyn FnMut(&mut Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => f(p),
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| each_para(&mut c.blocks, f)),
        }
    }
}

fn refs_inline(inlines: &[Inline], out: &mut Vec<(NoteKind, i64)>) {
    for i in inlines {
        match i {
            Inline::NoteRef { kind, id, .. } => out.push((*kind, *id)),
            i => {
                if let Some((_, tb)) = i.story() {
                    refs_in(&tb.blocks, out);
                }
            }
        }
    }
}

impl Editor {
    /// The numbers the references to notes of `kind` give, so a new note answers no reference read without its note.
    fn note_refs(&self, kind: NoteKind) -> std::collections::HashSet<i64> {
        let mut out = std::collections::HashSet::new();
        for pid in self.index.para_ids() {
            let Some(p) = self.index.para(&self.doc, pid) else { continue };
            for i in &p.inlines {
                if let Inline::NoteRef { kind: k, id, .. } = i
                    && *k == kind
                {
                    out.insert(*id);
                }
            }
        }
        out
    }

    /// Whether a note's reference can go at the selection: only in the body.
    pub(super) fn takes_notes(&self) -> bool {
        self.selection.focus().map(|p| p.story) == Some(StoryId::Body)
    }

    pub(super) fn insert_note(&mut self, kind: NoteKind, custom: Option<&str>, fonts: &Fonts) -> Outcome {
        if custom.is_some_and(|m| m.is_empty() || m.chars().count() > 10) {
            return Outcome::invalid("a custom mark is 1…10 characters");
        }
        if !self.takes_notes() {
            return Outcome::unavailable("notes are inserted in the body");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let w = cx.w.clone();
        let (ref_style, text_style) = note_styles(kind);
        let has = |s: &str| res.styles.by_id.contains_key(s);
        let mark_prop = |p: &mut Props| {
            if has(ref_style) {
                p.set("rStyle", format!("<{w}rStyle {w}val=\"{ref_style}\"/>"));
            } else {
                p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
            }
        };
        let refs = self.note_refs(kind);
        let notes = match kind {
            NoteKind::Foot => &self.doc.footnotes,
            NoteKind::End => &self.doc.endnotes,
        };
        let max = notes.keys().chain(&refs).copied().max().unwrap_or(0);
        let id = free_note_id(max, 1, |n| notes.contains_key(&n) || refs.contains(&n));
        let mut tx = self.begin(if kind == NoteKind::Foot { "Insert Footnote" } else { "Insert Endnote" });
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let Place::Text { para, offset, .. } = caret.place else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let (Some(run), Some(base)) = (self.run_at(&caret), self.run_base(para, offset, &res, &cx)) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        mark_prop(&mut p);
        let ref_run = res.run_from_xml(&base, p.xml().as_deref(), &cx);
        // A custom mark follows its reference, in its run.
        let mut with = vec![Inline::NoteRef { kind, id, custom: custom.is_some(), run: ref_run.clone() }];
        with.extend(custom.map(|m| Inline::Text(m.to_string(), ref_run)));
        if self.step(&mut tx, Step::Text { para, range: offset..offset, with, edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale caret");
        }
        let mut pp = Props::parse(None, "pPr", &cx);
        if has(text_style) {
            pp.set("pStyle", format!("<{w}pStyle {w}val=\"{text_style}\"/>"));
        }
        let r = res.para_from_xml(pp.xml().as_deref(), None, &cx);
        let text_run = res.run_from_xml(&r.base, None, &cx);
        let mut mp = Props::parse(None, "rPr", &cx);
        mark_prop(&mut mp);
        let mark_run = res.run_from_xml(&r.base, mp.xml().as_deref(), &cx);
        let pid = crate::docx::next_para_id();
        let note = Paragraph {
            style: Rc::new(r.style),
            mark: text_run.clone(),
            list: None,
            inlines: vec![
                match custom {
                    Some(m) => Inline::Text(m.to_string(), mark_run),
                    None => Inline::NoteMark(kind, id, mark_run),
                },
                Inline::Text(" ".into(), text_run),
            ],
            src: None,
            id: pid,
            div: None,
            controls: Vec::new(),
        };
        if self.step(&mut tx, Step::Note { kind, id, blocks: Some(vec![Block::Para(note)]) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("note failed");
        }
        let after = custom.map_or(1, |m| m.chars().count() as u32) + 1;
        self.selection = Selection::Caret(Position::text(story(kind, id), ParaId(pid), Unit(after)));
        self.commit(tx, None, fonts)
    }

    /// The separator story `part` of notes of `kind` draws: its plain text (paragraphs `\n` apart) and whether it is
    /// Word's built-in one (a line, or no continuation notice); `None` for a story the document doesn't have.
    pub fn note_separator(&self, kind: NoteKind, part: crate::docx::NoteSeparator) -> Option<NoteSeparatorInfo> {
        let key = part.key(kind);
        let blocks = self.doc.separators.get(&key)?;
        let builtin =
            blocks.len() == 1 && matches!(&blocks[0], Block::Para(p) if p.inlines.len() == 1 && matches!(p.inlines[0], Inline::Separator { .. }));
        let mut text: Vec<String> = Vec::new();
        paragraphs_text(blocks, &mut text);
        Some(NoteSeparatorInfo { text: text.join("\n"), builtin })
    }

    /// Sets separator story `part` of notes of `kind`: Word's built-in one (`None`: the line; no continuation notice),
    /// else `text`'s paragraphs (`\n` apart; empty: no line). One undo step.
    pub fn set_note_separator(&mut self, kind: NoteKind, part: crate::docx::NoteSeparator, text: Option<&str>, fonts: &Fonts) -> Outcome {
        use crate::docx::NoteSeparator as S;
        let key = part.key(kind);
        let defaults = self.doc.source.as_ref().map(|s| &s.separator_defaults);
        let builtin = |k: &str| defaults.and_then(|d| d.get(k)).map(|b| b.to_vec());
        let blocks = match text {
            None if part == S::Notice => None,
            None => match builtin(&key) {
                Some(b) => Some(b),
                None => return Outcome::unavailable("no built-in separator"),
            },
            Some(t) => {
                // A line like the separator's, its text in place of the rule
                let template = self.doc.separators.get(&key).or_else(|| self.doc.separators.get(&S::Separator.key(kind))).and_then(|b| {
                    b.iter().find_map(|b| match b {
                        Block::Para(p) => Some(p.clone()),
                        Block::Table(_) => None,
                    })
                });
                let Some(template) = template.or_else(|| {
                    builtin(&S::Separator.key(kind)).and_then(|b| {
                        b.into_iter().find_map(|b| match b {
                            Block::Para(p) => Some(p),
                            Block::Table(_) => None,
                        })
                    })
                }) else {
                    return Outcome::unavailable("no separator to take after");
                };
                let t = crate::docx::xml::legal(t);
                Some(
                    t.split('\n')
                        .map(|line| {
                            let mut p = template.clone();
                            let run = p.mark.clone();
                            p.inlines = if line.is_empty() { Vec::new() } else { vec![Inline::Text(line.trim_end_matches('\r').to_string(), run)] };
                            Block::Para(p)
                        })
                        .collect(),
                )
            }
        };
        // The same paragraphs, whatever their ids
        let bare = |p: &crate::docx::Paragraph| crate::docx::Paragraph { id: 0, src: None, ..p.clone() };
        let same = |a: &[Block], b: &[Block]| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(x, y)| match (x, y) {
                    (Block::Para(p), Block::Para(q)) => bare(p) == bare(q),
                    _ => x == y,
                })
        };
        let unchanged = match (self.doc.separators.get(&key), &blocks) {
            (Some(a), Some(b)) => same(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        if unchanged {
            return Outcome::Unchanged("already so");
        }
        let blocks = blocks.map(|b| b.iter().map(super::clipboard::fresh).collect());
        let mut tx = self.begin("Note Separator");
        if self.step(&mut tx, Step::Separator { key, blocks }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("separator failed");
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn set_note_properties(&mut self, p: crate::api::command::NoteProperties, fonts: &Fonts) -> Outcome {
        use crate::api::command::{NotePosition as Pos, NoteRestart as R, NoteScope, NoteType, NumberFormat};
        let foot = p.kind == NoteType::Footnote;
        if p.position.is_some_and(|pos| matches!(pos, Pos::PageBottom | Pos::BeneathText) != foot) {
            return Outcome::invalid("footnotes go at the page's bottom or beneath the text; endnotes at a section's or the document's end");
        }
        if !foot && p.restart == Some(R::EachPage) {
            return Outcome::invalid("endnotes don't restart on each page");
        }
        if p.format.is_some_and(|f| matches!(f, NumberFormat::Bullet | NumberFormat::None)) {
            return Outcome::invalid("a note number format is a number style");
        }
        if p.start.is_some_and(|s| !(1..=16383).contains(&s)) {
            return Outcome::invalid("the first number is 1…16383");
        }
        if p.columns.is_some_and(|c| !foot || c > 4) {
            return Outcome::invalid("footnotes take 0 to 4 columns; endnotes take none");
        }
        let k = usize::from(!foot);
        let sections: Vec<usize> = match p.apply_to.unwrap_or_default() {
            NoteScope::WholeDocument => (0..self.doc.sections.len()).collect(),
            // This section, or the selected sections when the selection spans several (Word's Apply changes to)
            NoteScope::ThisSection => self.selected_sections(),
        };
        let mut tx = self.begin("Note Options");
        let mut changed = false;
        for section in sections {
            let Some(s) = self.doc.sections.get(section) else { continue };
            let mut page = s.page();
            let n = &mut page.notes[k];
            if let Some(f) = p.format {
                n.fmt = Some(f.val().to_string());
            }
            if let Some(start) = p.start {
                n.start = i64::from(start);
            }
            if let Some(r) = p.restart {
                n.restart = match r {
                    R::Continuous => crate::docx::NoteRestart::Continuous,
                    R::EachSection => crate::docx::NoteRestart::EachSect,
                    R::EachPage => crate::docx::NoteRestart::EachPage,
                };
            }
            if foot && let Some(pos) = p.position {
                n.beneath = pos == Pos::BeneathText;
            }
            if let Some(c) = p.columns {
                page.footnote_columns = c;
            }
            if page == s.page() {
                continue;
            }
            let change = s.change.clone();
            if self.step(&mut tx, Step::SectionPage { section, page: Box::new(page), change }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("no section");
            }
            changed = true;
        }
        // Where endnotes go is the document's.
        if !foot && let Some(pos) = p.position {
            let mut settings = super::step::PageSettings::of(&self.doc.settings);
            let was = settings;
            settings.endnotes_sect_end = pos == Pos::SectionEnd;
            if settings != was {
                if self.step(&mut tx, Step::PageSettings(settings)).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("the setting can't change");
                }
                changed = true;
            }
        }
        if !changed {
            self.abort(tx);
            return Outcome::Unchanged("already so");
        }
        self.commit(tx, None, fonts)
    }

    /// Footnotes become endnotes, endnotes footnotes, or both: each note's body moves to the other kind under a new
    /// number, and its reference with it.
    pub(super) fn convert_notes(&mut self, c: crate::api::command::NoteConversion, fonts: &Fonts) -> Outcome {
        use crate::api::command::NoteConversion as C;
        let kinds: &[NoteKind] = match c {
            C::FootnotesToEndnotes => &[NoteKind::Foot],
            C::EndnotesToFootnotes => &[NoteKind::End],
            C::Swap => &[NoteKind::Foot, NoteKind::End],
        };
        let other = |k: NoteKind| if k == NoteKind::Foot { NoteKind::End } else { NoteKind::Foot };
        let doc = &self.doc;
        let notes = |k: NoteKind| if k == NoteKind::Foot { &doc.footnotes } else { &doc.endnotes };
        // New numbers above every note of the kind they become and every reference to one (the separators' are below 1).
        let mut map: std::collections::HashMap<(NoteKind, i64), i64> = std::collections::HashMap::new();
        let mut moves = Vec::new();
        for &from in kinds {
            let to = other(from);
            let (dest, refs) = (notes(to), self.note_refs(to));
            let (mut max, mut lo) = (dest.keys().chain(&refs).copied().max().unwrap_or(0), 1);
            let mut given = std::collections::HashSet::new();
            // In their order, so the numbers keep it.
            let mut from_notes: Vec<(i64, Vec<Block>)> = notes(from).iter().filter(|(id, _)| **id > 0).map(|(id, b)| (*id, b.clone())).collect();
            from_notes.sort_by_key(|(id, _)| *id);
            for (id, body) in from_notes {
                let next = free_note_id(max, lo, |n| dest.contains_key(&n) || refs.contains(&n) || given.contains(&n));
                if next > max {
                    max = next;
                } else {
                    lo = next + 1;
                }
                given.insert(next);
                map.insert((from, id), next);
                moves.push((from, id, next, body));
            }
        }
        if moves.is_empty() {
            return Outcome::Unchanged("no notes to convert");
        }
        // A caret in a note that moves goes with it, to the same place in its body.
        let caret = self.selection.focus().and_then(|f| {
            let (kind, id) = match f.story {
                StoryId::Footnote(id) => (NoteKind::Foot, id),
                StoryId::Endnote(id) => (NoteKind::End, id),
                _ => return None,
            };
            let (para, offset) = f.para_offset()?;
            Some((story(other(kind), *map.get(&(kind, id))?), para, offset))
        });
        let res = self.doc.resolver.clone();
        let cx = self.cx();
        let mut tx = self.begin("Convert Notes");
        for (from, id, _, _) in &moves {
            if self.step(&mut tx, Step::Note { kind: *from, id: *id, blocks: None }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("a note can't move");
            }
        }
        for (from, _, to_id, mut body) in moves {
            let to = other(from);
            each_para(&mut body, &mut |p| {
                for i in &mut p.inlines {
                    if let Inline::NoteMark(k, n, _) = i {
                        (*k, *n) = (to, to_id);
                    }
                }
                // Its other runs: re-resolved on settle
                if res.as_ref().is_some_and(|res| restyle_note_para(res, &cx, p, from, to)) {
                    tx.stale_paras.push(ParaId(p.id));
                }
            });
            if self.step(&mut tx, Step::Note { kind: to, id: to_id, blocks: Some(body) }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("a note can't move");
            }
        }
        let paras: Vec<ParaId> = self.index.para_ids().collect();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if !p.inlines.iter().any(|i| matches!(i, Inline::NoteRef { kind, id, .. } if map.contains_key(&(*kind, *id)))) {
                continue;
            }
            let again = res.as_ref().and_then(|res| self.para_again(para, None, res, &cx));
            let restyled = |run: &Run, from: NoteKind, at: u32| {
                res.as_ref()
                    .zip(again.as_ref())
                    .and_then(|(res, again)| restyled_ref(res, &cx, again.base(at), run, from, other(from)))
                    .unwrap_or_else(|| run.clone())
            };
            // Custom marks: the text after their reference
            let mut custom_of = None;
            let mut at = 0u32;
            let inlines: Vec<Inline> = p
                .inlines
                .iter()
                .map(|i| {
                    let here = at;
                    at += i.units() as u32;
                    match (i, std::mem::take(&mut custom_of)) {
                        (Inline::NoteRef { kind, id, custom, run }, _) if map.contains_key(&(*kind, *id)) => {
                            custom_of = custom.then_some(*kind);
                            let run = restyled(run, *kind, here);
                            Inline::NoteRef { kind: other(*kind), id: map[&(*kind, *id)], custom: *custom, run }
                        }
                        (Inline::Text(t, run), Some(from)) => Inline::Text(t.clone(), restyled(run, from, here)),
                        (i, _) => i.clone(),
                    }
                })
                .collect();
            if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("a reference can't change");
            }
        }
        if let Some((story, para, offset)) = caret {
            self.selection = Selection::Caret(Position::text(story, para, offset));
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn drop_orphan_notes(&mut self, tx: &mut Tx) {
        let mut gone = Vec::new();
        let mut at = None;
        let note = self.selection.focus().and_then(|p| match p.story {
            StoryId::Footnote(id) => Some((NoteKind::Foot, id)),
            StoryId::Endnote(id) => Some((NoteKind::End, id)),
            _ => None,
        });
        for s in &tx.undo {
            match s {
                Step::Text { para, range, with, .. } => {
                    let n = gone.len();
                    refs_inline(with, &mut gone);
                    if note.is_some_and(|k| gone[n..].contains(&k)) {
                        at = Some((*para, range.start));
                    }
                }
                Step::Blocks { with, .. } => refs_in(with, &mut gone),
                Step::Rows { with, .. } => with.iter().flat_map(|r| r.cells.iter()).for_each(|c| refs_in(&c.blocks, &mut gone)),
                Step::Restyle { inlines, .. } => refs_inline(inlines, &mut gone),
                _ => {}
            }
        }
        if gone.is_empty() {
            return;
        }
        gone.sort_by_key(|&(k, id)| (k == NoteKind::End, id));
        gone.dedup();
        let mut left = Vec::new();
        for pid in self.index.para_ids() {
            if let Some(p) = self.index.para(&self.doc, pid) {
                left.extend(p.inlines.iter().filter_map(|i| if let Inline::NoteRef { kind, id, .. } = i { Some((*kind, *id)) } else { None }));
            }
        }
        for (kind, id) in gone {
            let exists = match kind {
                NoteKind::Foot => self.doc.footnotes.contains_key(&id),
                NoteKind::End => self.doc.endnotes.contains_key(&id),
            };
            if exists && !left.contains(&(kind, id)) && self.step(tx, Step::Note { kind, id, blocks: None }).is_ok() && note == Some((kind, id)) {
                let story = at.and_then(|(p, _)| self.index.para_slot(p)).map(|s| self.index.story(s.container));
                if let (Some(story), Some((para, u))) = (story, at) {
                    self.selection = Selection::Caret(Position::text(story, para, u));
                }
            }
        }
    }
}

impl Editor {
    pub(super) fn drop_orphan_controls(&mut self, tx: &mut Tx) {
        use crate::docx::{Block, ControlId, ElementId, Inline};
        fn inlines(v: &[Inline], out: &mut Vec<ControlId>) {
            for i in v {
                match i {
                    Inline::ControlStart(c) | Inline::ControlEnd(c) | Inline::Sym { control: Some(c), .. } => out.push(*c),
                    i if i.story().is_some() => {
                        if let Some((_, tb)) = i.story() {
                            blocks(&tb.blocks, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        fn blocks(v: &[Block], out: &mut Vec<ControlId>) {
            for b in v {
                match b {
                    Block::Para(p) => {
                        out.extend(p.controls.iter().copied());
                        inlines(&p.inlines, out);
                    }
                    Block::Table(t) => {
                        out.extend(t.controls.iter().copied());
                        for r in &t.rows {
                            out.extend(r.controls.iter().copied());
                            for c in &r.cells {
                                out.extend(c.controls.iter().copied());
                                blocks(&c.blocks, out);
                            }
                        }
                    }
                }
            }
        }
        let mut gone = Vec::new();
        for s in &tx.undo {
            match s {
                Step::Text { with, .. } | Step::Restyle { inlines: with, .. } => inlines(with, &mut gone),
                Step::Blocks { with, .. } => blocks(with, &mut gone),
                Step::Split { second, .. } => {
                    gone.extend(second.controls.iter().copied());
                    inlines(&second.inlines, &mut gone);
                }
                Step::Rows { with, .. } => {
                    for r in with {
                        gone.extend(r.controls.iter().copied());
                        for c in &r.cells {
                            gone.extend(c.controls.iter().copied());
                            blocks(&c.blocks, &mut gone);
                        }
                    }
                }
                Step::Note { blocks: Some(b), .. } | Step::HeaderPart { blocks: Some(b), .. } => blocks(b, &mut gone),
                Step::Control { id, control: None } => gone.push(*id),
                _ => {}
            }
        }
        gone.sort();
        gone.dedup();
        for c in gone {
            if self.index.element(ElementId::Control(c)).is_none() && self.doc.controls.contains_key(&c) {
                let _ = self.step(tx, Step::Control { id: c, control: None });
            }
        }
    }
}

/// A note separator story, as [`Editor::note_separator`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSeparatorInfo {
    /// Its plain text, paragraphs `\n` apart (empty for the built-in line).
    pub text: String,
    /// It is Word's built-in line.
    pub builtin: bool,
}

/// The text of each paragraph in `blocks`, in order, a table's cells included. Table nesting is bounded when read.
fn paragraphs_text(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            // Without the stand-ins for objects; a character written as text stays
            Block::Para(p) => out.push(
                p.inlines
                    .iter()
                    .flat_map(|i| {
                        super::text::inline_chars(i).filter(move |&c| c != super::text::OBJECT || matches!(i, Inline::Text(..) | Inline::Sym { .. }))
                    })
                    .collect(),
            ),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| paragraphs_text(&c.blocks, out)),
        }
    }
}
