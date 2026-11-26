//! Captions

use std::rc::Rc;

use crate::api::review::Body;
use crate::docx::fields;
use crate::docx::{Block, FieldKind, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::position::{Position, StoryId};
use super::selection::{ObjectRef, Selection};
use super::step::{Step, para_len};

/// Word's built-in caption labels, always offered.
pub const BUILT_IN_LABELS: [&str; 3] = ["Equation", "Figure", "Table"];

/// Where a caption goes beside the item it captions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionPosition {
    /// Before the item.
    Above,
    /// After the item.
    Below,
}

/// A caption number's format (`\*`): 1, 2, 3; a, b, c; A, B, C; i, ii, iii; I, II, III.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionFormat {
    /// 1, 2, 3.
    #[default]
    Arabic,
    /// a, b, c.
    LowerLetter,
    /// A, B, C.
    UpperLetter,
    /// i, ii, iii.
    LowerRoman,
    /// I, II, III.
    UpperRoman,
}

impl CaptionFormat {
    fn switch(self) -> &'static str {
        match self {
            CaptionFormat::Arabic => "ARABIC",
            CaptionFormat::LowerLetter => "alphabetic",
            CaptionFormat::UpperLetter => "ALPHABETIC",
            CaptionFormat::LowerRoman => "roman",
            CaptionFormat::UpperRoman => "ROMAN",
        }
    }

    /// The format a `SEQ` code's `\*` names.
    pub fn of_code(code: &str) -> CaptionFormat {
        let w = fields::words(code);
        let arg = w.iter().position(|x| x == "\\*").and_then(|k| w.get(k + 1)).map_or("", String::as_str);
        match arg {
            "alphabetic" => CaptionFormat::LowerLetter,
            "ALPHABETIC" | "Alphabetic" => CaptionFormat::UpperLetter,
            "roman" => CaptionFormat::LowerRoman,
            "ROMAN" | "Roman" => CaptionFormat::UpperRoman,
            _ => CaptionFormat::Arabic,
        }
    }
}

/// The character between a chapter number and a caption's number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChapterSeparator {
    /// `-`.
    Hyphen,
    /// `.`.
    Period,
    /// `:`.
    Colon,
    /// An em dash.
    EmDash,
    /// An en dash.
    EnDash,
}

impl ChapterSeparator {
    fn char(self) -> char {
        match self {
            ChapterSeparator::Hyphen => '-',
            ChapterSeparator::Period => '.',
            ChapterSeparator::Colon => ':',
            ChapterSeparator::EmDash => '\u{2014}',
            ChapterSeparator::EnDash => '\u{2013}',
        }
    }
}

/// A caption's chapter number: the number of the nearest heading of `level` before it, then `separator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterNumber {
    /// The heading level, 1 to 9.
    pub level: u8,
    /// What follows the chapter number.
    pub separator: ChapterSeparator,
}

/// A caption to insert: its label, its text after the number, where it goes and how it is numbered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptionOptions {
    /// The label, which is also the sequence it numbers in.
    pub label: String,
    /// What follows the number, verbatim.
    pub text: String,
    /// `None`: above a table, else below.
    pub position: Option<CaptionPosition>,
    /// The number alone, without the label before it.
    pub exclude_label: bool,
    /// The number's format.
    pub format: CaptionFormat,
    /// A chapter number before it.
    pub chapter: Option<ChapterNumber>,
}

impl CaptionOptions {
    /// A caption with Word's defaults for `label`.
    pub fn new(label: &str) -> CaptionOptions {
        CaptionOptions {
            label: label.to_string(),
            text: String::new(),
            position: None,
            exclude_label: false,
            format: CaptionFormat::Arabic,
            chapter: None,
        }
    }

    /// Its `SEQ` code.
    pub fn seq_code(&self) -> String {
        let label = fields::quote_arg(&sequence_of(&self.label)).unwrap_or_default();
        let chapter = self.chapter.map_or(String::new(), |c| format!(" \\s {}", c.level.clamp(1, 9)));
        format!(" SEQ {label} \\* {}{chapter} ", self.format.switch())
    }
}

/// What a caption's label may be: Word's rule for labels (a letter first, then letters, digits, spaces and
/// underscores, 40 at most, no space last).
pub(crate) fn valid_label(label: &str) -> bool {
    let mut c = label.chars();
    c.next().is_some_and(char::is_alphabetic)
        && c.all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        && !label.ends_with(' ')
        && label.chars().count() <= 40
}

/// The sequence (`SEQ` identifier) a caption label numbers in: its spaces as underscores, as Word writes it.
pub fn sequence_of(label: &str) -> String {
    label.replace(' ', "_")
}

/// The item a caption captions.
enum Item {
    /// A paragraph of text.
    Para(ParaId),
    /// An inline picture's paragraph.
    Picture(ParaId),
    /// A table: the outermost one the selection is in, or one just made.
    Table(TableId),
    /// A floating object.
    Float(FloatId),
}

/// How many fields one re-evaluation sets: a hostile document's past it keep their values.
const MAX_REFRESH: usize = 1 << 20;

impl Editor {
    /// The labels a caption can take: Word's built-in ones, the host's `custom` ones, then those the document's
    /// sequence fields use, each once (without case), in that order.
    pub fn caption_labels(&self, custom: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |l: &str| {
            if !l.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(l)) {
                out.push(l.to_string());
            }
        };
        BUILT_IN_LABELS.iter().for_each(|l| push(l));
        custom.iter().for_each(|l| push(l));
        for para in self.paras_in_order() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for i in &p.inlines {
                // A sequence's label: its underscores as the spaces they stand for.
                if let Inline::FieldBegin { kind: Some(FieldKind::Seq(id)), .. } = i {
                    push(&id.replace('_', " "));
                }
            }
        }
        out
    }

    /// The item the selection is on: a selected object, the table the selection is in, else its paragraph.
    fn caption_item(&self) -> Option<Item> {
        match self.selection {
            Selection::Object(ObjectRef::Inline { para, .. }) => return Some(Item::Picture(para)),
            Selection::Object(ObjectRef::Floating(f)) => return Some(Item::Float(f)),
            Selection::Cells { table, .. } => return Some(self.outer_table(table)),
            _ => {}
        }
        let at = self.selection.focus()?;
        let (para, _) = at.para_offset()?;
        let chain = self.index.chain_of(&at);
        match chain.first() {
            Some(l) => Some(Item::Table(l.table)),
            None => Some(Item::Para(para)),
        }
    }

    fn outer_table(&self, table: TableId) -> Item {
        let mut t = table;
        while let Some(slot) = self.index.table_slot(t) {
            match slot.container {
                Container::Cell { table, .. } => t = table,
                _ => break,
            }
        }
        Item::Table(t)
    }

    /// Inserts a caption (Word's Insert Caption): a paragraph in the Caption style holding the label, its sequence
    /// field (after the chapter's number when asked) and the text, above or below the selected picture, table or
    /// paragraph; for a floating object, a text box beside it. The sequence's numbers after it, and every chapter
    /// number, update with it. One undo step.
    pub fn insert_caption(&mut self, c: &CaptionOptions, fonts: &Fonts) -> Outcome {
        self.caption_on(c, None, fonts)
    }

    /// [`Editor::insert_caption`] for `item`, else the item the selection is on.
    fn caption_on(&mut self, c: &CaptionOptions, item: Option<Item>, fonts: &Fonts) -> Outcome {
        if !valid_label(&c.label) {
            return Outcome::invalid("a label is a letter, then letters, digits, spaces and underscores");
        }
        if c.chapter.is_some_and(|ch| !(1..=9).contains(&ch.level)) {
            return Outcome::invalid("the chapter level is 1…9");
        }
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
        let Some(item) = item.or_else(|| self.caption_item()) else { return Outcome::unavailable("nothing to caption") };
        let position = c.position.unwrap_or(if matches!(item, Item::Table(_)) { CaptionPosition::Above } else { CaptionPosition::Below });
        self.begin_batch(Some("Insert Caption".into()));
        if !self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key("Caption"))
            && let Some(b) = crate::services::styles::builtin::built_in("Caption")
        {
            self.add_built_in(b, fonts);
        }
        let out = match item {
            Item::Float(f) => self.caption_box(f, c, position, fonts),
            other => self.caption_para(other, c, position, fonts),
        };
        if !matches!(out, Outcome::Changed(_)) {
            self.end_batch(false, fonts);
            return out;
        }
        // The label's sequence renumbers (Word), and the new caption's chapter number shows.
        let seq = sequence_of(&c.label);
        let own: Vec<crate::docx::FieldId> = self
            .selection
            .focus()
            .and_then(|at| at.para_offset())
            .and_then(|(p, _)| self.index.para(&self.doc, p))
            .map(|p| p.inlines.iter().filter_map(|i| if let Inline::FieldBegin { id, .. } = i { Some(*id) } else { None }).collect())
            .unwrap_or_default();
        // Under document protection, the others only where it lets a field go.
        let kept = self.protected_fields(&|f, k| !own.contains(&f) && matches!(k, FieldKind::Seq(n) if n.eq_ignore_ascii_case(&seq)));
        self.refresh_values(
            &|f, k| {
                !kept.contains(&f)
                    && match k {
                        FieldKind::Seq(n) => n.eq_ignore_ascii_case(&seq),
                        FieldKind::StyleRef(_) => own.contains(&f),
                        _ => false,
                    }
            },
            fonts,
        );
        self.end_batch(true, fonts)
    }

    /// A caption paragraph's inlines in `run`: label, chapter number and separator, sequence field, text.
    fn caption_inlines(&self, c: &CaptionOptions, run: &crate::docx::Run) -> Vec<Inline> {
        let mut v = Vec::new();
        if !c.exclude_label {
            v.push(Inline::Text(format!("{} ", c.label), run.clone()));
        }
        if let Some(ch) = c.chapter {
            v.extend(fields::field_inlines(&format!(" STYLEREF {} \\s ", ch.level), "", run));
            v.push(Inline::Text(ch.separator.char().to_string(), run.clone()));
        }
        v.extend(fields::field_inlines(&c.seq_code(), "", run));
        if !c.text.is_empty() {
            v.push(Inline::Text(c.text.clone(), run.clone()));
        }
        v
    }

    fn caption_para(&mut self, item: Item, c: &CaptionOptions, position: CaptionPosition, fonts: &Fonts) -> Outcome {
        let (container, block, keep_with) = match item {
            Item::Para(p) | Item::Picture(p) => {
                let Some(slot) = self.index.para_slot(p) else { return Outcome::unavailable("no paragraph") };
                (slot.container, slot.block, matches!(item, Item::Picture(_)).then_some(p))
            }
            Item::Table(t) => {
                let Some(slot) = self.index.table_slot(t) else { return Outcome::unavailable("no table") };
                (slot.container, slot.block, None)
            }
            Item::Float(_) => return Outcome::unavailable("a floating object"),
        };
        let Some(like) = self.index.blocks(&self.doc, container).and_then(|bl| {
            bl.iter().skip(block).chain(bl.iter().take(block).rev()).find_map(|b| if let Block::Para(p) = b { Some(p.clone()) } else { None })
        }) else {
            return Outcome::unavailable("no paragraph");
        };
        let w = self.cx().w;
        // Above, it keeps with what it captions.
        let keep = if position == CaptionPosition::Above { format!("<{w}keepNext/>") } else { String::new() };
        let ppr = format!("<{w}pPr><{w}pStyle {w}val=\"Caption\"/>{keep}</{w}pPr>");
        let mut p = self.new_para(&ppr, &[], container, &like);
        let run = p.mark.clone();
        p.inlines = self.caption_inlines(c, &run);
        let at = if position == CaptionPosition::Above { block } else { block + 1 };
        // The controls both its neighbours are in, so none ends up split.
        p.controls = self
            .index
            .blocks(&self.doc, container)
            .map(|bl| {
                let before = at.checked_sub(1).and_then(|k| bl.get(k)).map(super::merge::block_controls);
                let after = bl.get(at).map(super::merge::block_controls);
                match (before, after) {
                    (Some(x), Some(y)) => x.iter().zip(y).take_while(|(a, b)| a == b).map(|(a, _)| *a).collect(),
                    _ => Vec::new(),
                }
            })
            .unwrap_or_default();
        let id = ParaId(p.id);
        let mut tx = self.begin("Insert Caption");
        if self.step(&mut tx, Step::Blocks { container, range: at..at, with: vec![Block::Para(p)] }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the caption couldn't be inserted");
        }
        // Below a picture, the picture keeps with its caption.
        if position == CaptionPosition::Below
            && let Some(pic) = keep_with
            && let (Some(res), Some(q)) = (self.doc.resolver.clone(), self.index.para(&self.doc, pic))
            && !q.style.keep_next
        {
            let cx = self.cx();
            let mut pr = crate::docx::write::props::Props::parse(self.ppr(&q.style.direct, &cx).as_deref(), "pPr", &cx);
            pr.set("keepNext", format!("<{w}keepNext/>"));
            if let Some(r) = self.para_again(pic, Some(pr.xml().as_deref()), &res, &cx)
                && let Some(q) = self.index.para(&self.doc, pic)
            {
                let (mark, list) = (q.mark.clone(), q.list.clone());
                let _ = self.step(&mut tx, Step::Para { para: pic, style: Rc::new(r.style), mark, list });
            }
        }
        let story = self.index.para_slot(id).map_or(StoryId::Body, |s| self.index.story(s.container));
        let end = self.index.para(&self.doc, id).map_or(Unit(0), para_len);
        self.selection = Selection::Caret(Position::text(story, id, end));
        self.commit(tx, None, fonts)
    }

    /// A floating object's caption: a text box of its width just above or below it, anchored with it.
    fn caption_box(&mut self, f: FloatId, c: &CaptionOptions, position: CaptionPosition, fonts: &Fonts) -> Outcome {
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::unavailable("no object") };
        let Some(p) = self.index.para(&self.doc, anchor).cloned() else { return Outcome::unavailable("no object") };
        let Some((k, obj, run)) = p.inlines.iter().enumerate().find_map(|(k, i)| match i {
            Inline::Float(fl, r) if fl.id == f.0 => Some((k, (**fl).clone(), r.clone())),
            _ => None,
        }) else {
            return Outcome::unavailable("no object");
        };
        let w = self.cx().w;
        let ppr = format!("<{w}pPr><{w}pStyle {w}val=\"Caption\"/></{w}pPr>");
        let mut inner = self.new_para(&ppr, &[], Container::Section(0), &p);
        let r = inner.mark.clone();
        inner.inlines = self.caption_inlines(c, &r);
        let inner_id = inner.id;
        let name = self.new_object_name("Text Box");
        let h = 24.0;
        let fid = crate::docx::next_para_id();
        let gap = 3.6;
        let dy = match position {
            CaptionPosition::Below => obj.h + gap,
            CaptionPosition::Above => -h - gap,
        };
        // An aligned (or percentage) place: the offset from the same frame of where the object is laid out, moved.
        let v_offset = if obj.v_align.is_some() || obj.relative.pos[1].is_some() {
            let at = self.float_box(f, fonts).and_then(|(page, r)| self.moved_float(f, page, r, [r[0], r[1] + dy, r[2], r[3] + dy]));
            match at {
                Some(m) => m.v_offset,
                None => return Outcome::unavailable("the object isn't laid out"),
            }
        } else {
            obj.v_offset + dy
        };
        let float = crate::docx::Float {
            id: fid,
            w: obj.w,
            auto_w: false,
            h,
            image: None,
            alt: "".into(),
            lock_aspect: false,
            text_box: Some(crate::docx::TextBox {
                blocks: vec![Block::Para(inner)],
                insets: [7.2, 3.6, 7.2, 3.6],
                outline: 0.0,
                auto_fit: true,
                v_anchor: 0,
                flow: 0,
            }),
            h_relative: obj.h_relative,
            h_align: obj.h_align,
            h_offset: obj.h_offset,
            v_relative: obj.v_relative,
            v_align: None,
            v_offset,
            wrap: obj.wrap,
            wrap_text: obj.wrap_text,
            dist: obj.dist,
            effect: [0.0; 4],
            poly: None,
            poly_edited: false,
            behind: obj.behind,
            allow_overlap: true,
            layout_in_cell: obj.layout_in_cell,
            order: usize::MAX / 2,
            z: self.front_z(),
            src: None,
            locked: false,
            frame_src: None,
            in_note: obj.in_note,
            parts: Vec::new(),
            shape: Some(Box::new(crate::drawing::Shape::new(
                crate::drawing::NewShape::TextBox,
                crate::drawing::units::Emu::from_pt(obj.w),
                crate::drawing::units::Emu::from_pt(h),
                &name,
            ))),
            rot_swap: false,
            vml: false,
            // A percentage across: the same share of the same frame.
            relative: crate::docx::Relative { pos: [obj.relative.pos[0], None], ..Default::default() },
        };
        let mut inlines = p.inlines.clone();
        inlines.insert(k + 1, Inline::Float(Box::new(float), run));
        let mut tx = self.begin("Insert Caption");
        if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the caption couldn't be inserted");
        }
        let id = ParaId(inner_id);
        let end = self.index.para(&self.doc, id).map_or(Unit(0), para_len);
        self.selection = Selection::Caret(Position::text(StoryId::TextBox(FloatId(fid)), id, end));
        self.commit(tx, None, fonts)
    }

    /// The fields `of` takes that enforced document protection keeps from changing where they are (as it would keep
    /// a field from going there): none without protection.
    fn protected_fields(&self, of: &dyn Fn(crate::docx::FieldId, &FieldKind) -> bool) -> std::collections::HashSet<crate::docx::FieldId> {
        let mut out = std::collections::HashSet::new();
        if self.enforced_protection().is_none() {
            return out;
        }
        for para in self.paras_in_order() {
            let (Some(p), Some(slot)) = (self.index.para(&self.doc, para), self.index.para_slot(para)) else { continue };
            let story = self.index.story(slot.container);
            let mut at = 0u32;
            for i in &p.inlines {
                if let Inline::FieldBegin { id, kind: Some(k), .. } = i
                    && of(*id, k)
                {
                    let pos = Position::text(story, para, Unit(at));
                    if self.protection_refuses_at(crate::api::command::CommandId::InsertField, &pos, &pos).is_some() {
                        out.insert(*id);
                    }
                }
                at += i.units() as u32;
            }
        }
        out
    }

    /// Sets the values (and cached results) of the body's fields of the kinds `want` takes to what evaluating the
    /// document gives now: sequence numbers after a caption, references after their targets changed, the note a
    /// `NOTEREF` reads. One read-only evaluation of the body; returns how many values changed. One whose result can't
    /// be written (a result across paragraphs) still shows its new value.
    pub fn refresh_values(&mut self, want: &dyn Fn(crate::docx::FieldId, &FieldKind) -> bool, fonts: &Fonts) -> usize {
        type Note = Option<(crate::docx::NoteKind, i64)>;
        let (now, _) = fields::field_changes(&self.doc, crate::docx::now_local());
        let mut changed: Vec<(ParaId, crate::docx::FieldId, Option<String>, Option<Note>)> = Vec::new();
        for c in now {
            let para = ParaId(c.para);
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let Some(Inline::FieldBegin { kind: Some(k), value, lock: false, .. }) =
                p.inlines.iter().find(|i| matches!(i, Inline::FieldBegin { id, .. } if *id == c.id))
            else {
                continue;
            };
            if !want(c.id, k) {
                continue;
            }
            let note = match (k, c.note) {
                (FieldKind::NoteRef(_, was), Some(n)) if *was != n => Some(n),
                _ => None,
            };
            let v = c.value.filter(|v| value.as_ref() != Some(v));
            if v.is_some() || note.is_some() {
                changed.push((para, c.id, v, note));
                if changed.len() >= MAX_REFRESH {
                    break;
                }
            }
        }
        if changed.is_empty() {
            return 0;
        }
        let mut tx = self.begin("Update Fields");
        let mut n = 0;
        for (para, f, v, note) in &changed {
            let Some(p) = self.index.para(&self.doc, *para) else { continue };
            let mut inlines: Vec<Inline> = p.inlines.clone();
            for i in &mut inlines {
                if let Inline::FieldBegin { id, value, kind, .. } = i
                    && id == f
                {
                    if v.is_some() {
                        value.clone_from(v);
                    }
                    if let (Some(FieldKind::NoteRef(_, was)), Some(n)) = (kind, note) {
                        *was = *n;
                    }
                }
            }
            if self.step(&mut tx, Step::Restyle { para: *para, inlines }).is_ok() {
                n += 1;
                if let Some(v) = v {
                    self.set_result(&mut tx, *f, &Body::Text(v.clone()));
                }
            }
        }
        self.commit(tx, None, fonts);
        n
    }

    /// Writes the layout's numbers into the cached results of fields `ids` that layout evaluates: a `PAGEREF`'s page,
    /// a `NOTEREF`'s note number (laid out first). Returns how many changed.
    pub fn fill_layout_results(&mut self, ids: &std::collections::HashSet<crate::docx::FieldId>, fonts: &Fonts) -> usize {
        if ids.is_empty() {
            return 0;
        }
        self.complete(fonts);
        let pages = self.layout.bookmark_pages(&self.doc);
        let notes: std::collections::HashMap<(crate::docx::NoteKind, i64), String> =
            self.layout.note_refs_shown().into_iter().map(|(k, (n, _))| (k, n.to_string())).collect();
        let mut want: Vec<(crate::docx::FieldId, String)> = Vec::new();
        // `\p`: each paragraph's page, and its place in the document.
        let order: Vec<ParaId> = self.paras_in_order();
        let (mut on_page, mut place) = (None, None);
        for &para in &order {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            if !p.inlines.iter().any(|i| matches!(i, Inline::FieldBegin { id, .. } if ids.contains(id))) {
                continue;
            }
            for s in fields::spans(&p.inlines) {
                let Inline::FieldBegin { id, kind: Some(kind), .. } = &p.inlines[s.begin] else { continue };
                if !ids.contains(id) {
                    continue;
                }
                let code = fields::code_text(&p.inlines, s);
                let v = match kind {
                    FieldKind::PageRef(name) => crate::layout::text::PageRef::shown(pages.get(name), &code),
                    FieldKind::NoteRef(_, Some(note)) => match notes.get(note) {
                        Some(n) => n.clone(),
                        None => continue,
                    },
                    _ => continue,
                };
                let name = match kind {
                    FieldKind::PageRef(n) | FieldKind::NoteRef(n, _) => n,
                    _ => continue,
                };
                let v = match pages.get(name).filter(|_| fields::relative_at(&p.inlines, s.begin)) {
                    Some(r) => {
                        let on_page: &std::collections::HashMap<ParaId, usize> = on_page.get_or_insert_with(|| self.layout.para_pages());
                        let place: &std::collections::HashMap<ParaId, usize> =
                            place.get_or_insert_with(|| order.iter().enumerate().map(|(k, p)| (*p, k)).collect());
                        // By document order: the layout's, on one page.
                        let above = if r.para == para {
                            r.above(name, p, s.begin, None, None)
                        } else {
                            place.get(&r.para).zip(place.get(&para)).is_some_and(|(a, b)| a < b)
                        };
                        r.relative(&v, matches!(kind, FieldKind::PageRef(_)), above, on_page.get(&para).copied())
                    }
                    None => v,
                };
                if fields::result_text(&p.inlines, s) != v {
                    want.push((*id, v));
                }
            }
        }
        if want.is_empty() {
            return 0;
        }
        let mut tx = self.begin("Update Fields");
        for (f, v) in &want {
            self.set_result(&mut tx, *f, &Body::Text(v.clone()));
        }
        self.commit(tx, None, fonts);
        want.len()
    }

    /// The host's AutoCaption setting: a caption with these options goes with each Word table inserted (Word's list
    /// is Word tables and OLE objects, which the engine doesn't insert). `None` turns it off.
    pub fn set_auto_caption(&mut self, c: Option<CaptionOptions>) {
        self.auto_caption = c;
    }

    /// The host's AutoCaption setting, if one is on.
    pub fn auto_caption(&self) -> Option<&CaptionOptions> {
        self.auto_caption.as_ref()
    }

    /// Inserts a table (Word's Insert Table) with its AutoCaption when the host set one: one undo step, the caret in
    /// its first cell.
    pub(super) fn insert_table_captioned(&mut self, rows: usize, columns: usize, fonts: &Fonts) -> Outcome {
        let Some(c) = self.auto_caption.clone() else { return self.insert_table(rows, columns, fonts) };
        self.begin_batch(Some("Insert Table".into()));
        let out = self.insert_table(rows, columns, fonts);
        if matches!(out, Outcome::Changed(_)) {
            let back = self.selection;
            // The table just made, the innermost the caret is in.
            let made = self.selection.focus().and_then(|at| self.index.chain_of(&at).last().map(|l| Item::Table(l.table)));
            if matches!(self.caption_on(&c, made, fonts), Outcome::Changed(_)) {
                self.selection = back;
            }
        }
        self.end_batch(matches!(out, Outcome::Changed(_)), fonts)
    }
}

/// A paragraph's caption label: the identifier of its first sequence field.
pub fn caption_label(p: &Paragraph) -> Option<&str> {
    p.inlines.iter().find_map(|i| match i {
        Inline::FieldBegin { kind: Some(FieldKind::Seq(id)), .. } => Some(id.as_str()),
        _ => None,
    })
}
