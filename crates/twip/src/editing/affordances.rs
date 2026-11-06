//! Placeholder affordances: what design mode draws over fields and controls, and the view-only state behind it

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::docx::{ControlId, ControlKind, ElementId, FieldId, FieldKind, FieldView, Inline};
use crate::layout::{ParaId, TableId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::merge::Where;
use super::selection::Selection;
use super::sub_templates::Resolution;
pub use super::view::{
    AffordanceView, Block, BlockState, Button, CardState, FieldState, Icon, MONO_ADVANCE, Part, Segment, Tab, Target, mono_width,
};

/// The affordances' state: view settings from the host and what the pointer is over. Never saved, never undone.
#[derive(Clone, Debug, Default)]
pub struct Affordances {
    /// The tooltip's type text by field code.
    pub field_types: BTreeMap<String, String>,
    /// Merge fields (by first argument) the data source lacks.
    pub unmapped: BTreeSet<String>,
    /// Drawn hovered until cleared.
    pub highlight: Option<ElementId>,
    /// What the pointer is over.
    pub hovered: Option<Target>,
    /// A repeating section its tab selected, while the selection is still the one that made.
    pub selected: Option<(Target, Selection)>,
    /// The version text and newer version's notice for each sub-template, by category and name.
    pub versions: HashMap<(String, String), crate::api::command::SubTemplateVersion>,
    /// The selected placeholder, found once while one frame's views are built.
    frame_selected: Option<Option<Target>>,
}

/// A placeholder described for the host.
#[derive(Clone, Debug, PartialEq)]
pub struct Described {
    pub target: Target,
    pub label: String,
    pub key: Option<String>,
    pub code: Option<String>,
    pub type_text: String,
    pub unmapped: bool,
    /// Where it shows: a field's last segment, else its first page's box.
    pub anchor: (usize, [f64; 4]),
}

/// A repeating section's or shown sub-template's tab band: 16 px at 100%, in points so layout doesn't depend on the
/// zoom.
pub const BAND: f64 = 12.0;

/// A sub-template card's height (44 px at 100%) and the gap above and below it, in points.
pub const CARD_HEIGHT: f64 = 33.0;
pub const CARD_GAP: f64 = 6.0;
pub const CARD: f64 = CARD_HEIGHT + 2.0 * CARD_GAP;

fn control_kind_name(k: &ControlKind) -> Option<&'static str> {
    match k {
        ControlKind::PlainText { .. } => Some("plain text"),
        ControlKind::RichText => Some("rich text"),
        ControlKind::Date { .. } => Some("date"),
        ControlKind::DropDown => Some("drop-down"),
        ControlKind::Picture
        | ControlKind::Checkbox { .. }
        | ControlKind::Group
        | ControlKind::RepeatingSection
        | ControlKind::RepeatingItem
        | ControlKind::Other => None,
    }
}

impl Editor {
    pub(super) fn affordances_on(&self) -> bool {
        self.view_settings.placeholder_ui && self.mode == super::Mode::Design
    }

    /// A field shown as its result that holds a value the host fills: no evaluated kind, or a sub-template reference
    /// that can't be expanded.
    fn marked_field(&self, kind: &Option<FieldKind>, view: FieldView, id: FieldId) -> bool {
        use super::sub_templates::Resolution as R;
        if view == FieldView::Code {
            return false;
        }
        match kind {
            None => true,
            Some(FieldKind::SubTemplate) => self.references.by_field.get(&id).is_none_or(|r| matches!(r.resolution, R::NotAlone | R::NotInBody)),
            Some(
                FieldKind::Page
                | FieldKind::NumPages
                | FieldKind::SectionPages
                | FieldKind::Section
                | FieldKind::PageRef(_)
                | FieldKind::Ref(_)
                | FieldKind::Seq(_)
                | FieldKind::StyleRef(_)
                | FieldKind::Date
                | FieldKind::If
                | FieldKind::Bookmark(_)
                | FieldKind::Invalid,
            ) => false,
        }
    }

    fn shows_placeholder_text(&self, c: ControlId) -> bool {
        self.doc.controls.get(&c).is_some_and(|k| k.props.showing_placeholder && control_kind_name(&k.props.kind).is_some())
    }

    /// The fields and controls marked on `page`, in reading order.
    /// With the units their marks cover when that's within one paragraph (block-level controls have none). Each
    /// paragraph's fields are found in one pass: a paragraph can hold many.
    fn page_targets(&mut self, page: usize) -> Vec<(Target, Option<Run>)> {
        let Some(geo) = self.pages.page(page, &self.index) else { return Vec::new() };
        let mut seen = HashSet::new();
        let mut out: Vec<(Target, Option<Run>)> = Vec::new();
        for para in geo.lines.iter().flat_map(|l| l.paras.iter().copied()) {
            if !seen.insert(para) {
                continue;
            }
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            // A block-level control (rich text around paragraphs) is in its paragraphs' controls, not their inlines.
            for c in &p.controls {
                if self.shows_placeholder_text(*c) && !out.iter().any(|(t, _)| *t == Target::Control(*c)) {
                    out.push((Target::Control(*c), None));
                }
            }
            let starts: Vec<u32> = p.inlines.iter().scan(0u32, |at, i| Some(std::mem::replace(at, *at + i.units() as u32))).collect();
            let run = |a: usize, b: u32| Some((para, Unit(starts[a]), Unit(b)));
            let spans = crate::docx::fields::spans(&p.inlines);
            let mut at = Vec::new();
            for (k, sp) in spans.iter().enumerate() {
                let Inline::FieldBegin { id, kind, view, .. } = &p.inlines[sp.begin] else { continue };
                if !self.marked_field(kind, *view, *id) || is_link(&p.inlines, *sp) {
                    continue;
                }
                // A field inside a shown field's result is marked over that field; inside its code, or an IF, not.
                let cover = match spans[..k].iter().rev().find(|o| o.begin < sp.begin && sp.begin < o.end) {
                    None => *sp,
                    Some(o) => {
                        let shown =
                            matches!(&p.inlines[o.begin], Inline::FieldBegin { view: FieldView::Result, kind, .. } if *kind != Some(FieldKind::If));
                        if !(shown && o.sep.is_some_and(|sep| sep < sp.begin)) {
                            continue;
                        }
                        *o
                    }
                };
                at.push((sp.begin, Target::Field(*id), run(cover.begin, starts[cover.end] + 1)));
            }
            for (k, i) in p.inlines.iter().enumerate() {
                if let Inline::ControlStart(c) = i
                    && self.shows_placeholder_text(*c)
                    && let Some(last) = p.inlines[k..].iter().position(|j| matches!(j, Inline::ControlEnd(x) if x == c))
                {
                    at.push((k, Target::Control(*c), run(k, starts[k + last])));
                }
            }
            at.sort_by_key(|(k, ..)| *k);
            out.extend(at.into_iter().map(|(_, t, r)| (t, r)));
        }
        out
    }

    /// The field whose range marks the field beginning at `begin`: itself, or the shown field (a link) whose result
    /// holds it, since a shown field is one unit. `None`: not marked, or inside a field's code or an IF.
    fn mark_span(&self, inlines: &[Inline], begin: usize) -> Option<usize> {
        let Inline::FieldBegin { id, kind, view, .. } = &inlines[begin] else { return None };
        if !self.marked_field(kind, *view, *id) || is_link(inlines, crate::docx::fields::span_at(inlines, begin)?) {
            return None;
        }
        // Most fields aren't inside another: only then are the paragraph's spans needed.
        let depth = inlines[..begin].iter().fold(0i32, |d, i| match i {
            Inline::FieldBegin { .. } => d + 1,
            Inline::FieldEnd { .. } => d - 1,
            _ => d,
        });
        if depth <= 0 {
            return Some(begin);
        }
        let outer = crate::docx::fields::spans(inlines).into_iter().find(|o| o.begin < begin && begin < o.end);
        let Some(o) = outer else { return Some(begin) };
        let shown = matches!(&inlines[o.begin], Inline::FieldBegin { view: FieldView::Result, kind, .. } if *kind != Some(FieldKind::If));
        (shown && o.sep.is_some_and(|sep| sep < begin)).then_some(o.begin)
    }

    /// Where a target's mark goes: its own range, or the field holding it ([`Editor::mark_span`]).
    fn mark_where(&self, t: Target) -> Option<Where> {
        let w = self.locate(t.element())?;
        let Target::Field(_) = t else { return Some(w) };
        let Where::Inline { para, first, .. } = w else { return Some(w) };
        let p = self.index.para(&self.doc, para)?;
        match self.mark_span(&p.inlines, first)? {
            k if k == first => Some(w),
            k => match &p.inlines[k] {
                Inline::FieldBegin { id, .. } => self.locate(ElementId::Field(*id)),
                _ => None,
            },
        }
    }

    fn is_target(&self, t: Target) -> bool {
        match t {
            Target::Control(c) => self.shows_placeholder_text(c),
            Target::RepeatingSection(c) => self.doc.controls.get(&c).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection),
            Target::Field(_) => self.mark_where(t).is_some(),
            Target::SubTemplate(f) => {
                self.references.by_field.get(&f).is_some_and(|r| !matches!(r.resolution, Resolution::NotAlone | Resolution::NotInBody))
            }
            Target::InputBlock(c) => self.block_picture(c).is_some(),
            Target::IfField(f) => self.if_span(f).is_some(),
        }
    }

    /// A placeholder's target, from its element.
    fn target_of(&self, e: ElementId) -> Option<Target> {
        let t = match e {
            ElementId::Field(f)
                if self.references.by_field.get(&f).is_some_and(|r| !matches!(r.resolution, Resolution::NotAlone | Resolution::NotInBody)) =>
            {
                Target::SubTemplate(f)
            }
            ElementId::Field(f) if self.if_span(f).is_some() => Target::IfField(f),
            ElementId::Field(f) => Target::Field(f),
            ElementId::Control(c) if self.doc.controls.get(&c).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection) => {
                Target::RepeatingSection(c)
            }
            ElementId::Control(c) if self.doc.controls.get(&c).is_some_and(|k| k.props.kind == ControlKind::Picture) => Target::InputBlock(c),
            ElementId::Control(c) => Target::Control(c),
        };
        self.is_target(t).then_some(t)
    }

    /// A picture control's unfilled placeholder picture: its paragraph and unit, and its image.
    fn block_picture(&self, c: ControlId) -> Option<(ParaId, Unit, usize)> {
        let Where::Inline { para, first, last, .. } = self.locate(ElementId::Control(c))? else { return None };
        let p = self.index.para(&self.doc, para)?;
        let (k, image) = (first..last).find_map(|k| match &p.inlines[k] {
            Inline::Picture(pic, _) => pic.image.filter(|i| self.doc.placeholders.contains_key(i) && !self.doc.filled.contains(i)).map(|i| (k, i)),
            _ => None,
        })?;
        let unit = p.inlines[..k].iter().map(|i| i.units() as u32).sum();
        Some((para, Unit(unit), image))
    }

    /// The input block the selection is: a placeholder picture selected as an object.
    fn selected_block(&self) -> Option<ControlId> {
        let Selection::Object(super::selection::ObjectRef::Inline { para, offset }) = self.selection else { return None };
        let mut all: Vec<ControlId> = self.doc.controls.iter().filter(|(_, k)| k.props.kind == ControlKind::Picture).map(|(c, _)| *c).collect();
        all.sort_unstable();
        all.into_iter().find(|c| self.block_picture(*c).is_some_and(|(p, u, _)| p == para && u == offset))
    }

    /// Whether the selection is an input block: with the affordances on, the block is the placeholder, never its
    /// picture, so nothing reports or edits a picture.
    pub fn input_block_selected(&self) -> bool {
        self.view_settings.placeholder_ui && self.selected_block().is_some()
    }

    /// The selected input block's control as units of its paragraph, in design mode, where a drag moves all of it.
    pub(super) fn movable_block(&self) -> Option<(ControlId, ParaId, usize, usize, Unit, Unit)> {
        if self.mode != super::Mode::Design || !self.view_settings.placeholder_ui {
            return None;
        }
        let c = self.selected_block()?;
        match self.locate(ElementId::Control(c))? {
            Where::Inline { para, first, last, start, end, .. } => Some((c, para, first, last, start, end)),
            _ => None,
        }
    }

    /// Moves the selected input block's control, whole and keeping its id, to `to`: one undo step, the block still
    /// selected.
    pub(super) fn move_block(&mut self, to: super::position::Position, fonts: &crate::font::Fonts) -> Outcome {
        use super::step::Step;
        let Some((c, para, first, last, start, end)) = self.movable_block() else { return Outcome::Unchanged("no block") };
        let Some((dst, at)) = to.para_offset() else { return Outcome::Unchanged("no drop place") };
        if dst == para && start <= at && at <= end {
            return Outcome::Unchanged("dropped on the block");
        }
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no block") };
        let moved: Vec<Inline> = p.inlines[first..=last].to_vec();
        let unmarked: Vec<Inline> = p.inlines.iter().filter(|i| !matches!(i, Inline::ControlStart(x) | Inline::ControlEnd(x) if *x == c)).cloned().collect();
        let at = if dst == para && at > end { Unit(at.0 - (end.0 - start.0)) } else { at };
        let mut tx = self.begin("Move Object");
        let ok = self.step(&mut tx, Step::Restyle { para, inlines: unmarked }).is_ok()
            && (start == end || self.step(&mut tx, Step::Text { para, range: start..end, with: Vec::new(), edges: Default::default() }).is_ok())
            && self.step(&mut tx, Step::Text { para: dst, range: at..at, with: moved, edges: Default::default() }).is_ok();
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("move failed");
        }
        let out = self.commit(tx, None, fonts);
        if let Some((para, offset, _)) = self.block_picture(c) {
            self.selection = Selection::Object(super::selection::ObjectRef::Inline { para, offset });
        }
        out
    }

    /// The buttons of the selected input block, when the affordances are on.
    pub(super) fn selected_block_actions(&self) -> Vec<crate::api::command::PlaceholderAction> {
        let Some(c) = self.selected_block().filter(|_| self.view_settings.placeholder_ui) else { return Vec::new() };
        let tag = self.doc.controls.get(&c).and_then(|k| k.props.tag.clone());
        let key = self.block_picture(c).and_then(|(_, _, i)| self.doc.placeholder_keys.get(&i).cloned());
        self.looks.find(ElementId::Control(c), tag.as_deref(), key.as_deref()).map(|r| r.actions.clone()).unwrap_or_default()
    }

    /// Remembers that the selection is a whole repeating section's, which a row selection doesn't otherwise say.
    pub(super) fn note_selected(&mut self, e: ElementId) {
        self.affordances.selected = match self.target_of(e) {
            Some(t @ Target::RepeatingSection(_)) => Some((t, self.selection)),
            Some(Target::Field(_) | Target::Control(_) | Target::SubTemplate(_) | Target::InputBlock(_) | Target::IfField(_)) | None => None,
        };
    }

    /// The rects of units `start..end` of one paragraph on `pages`, one per line.
    fn para_rects(&mut self, para: ParaId, start: Unit, end: Unit, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for l in geo.lines.iter().filter(|l| l.shows(para)) {
                let (Some(first), Some(last)) = (l.first_of(para), l.last_of(para)) else { continue };
                if last < start || first > end {
                    continue;
                }
                let (Some(x0), Some(x1)) = (l.x_of(para, start.max(first)), l.x_of(para, end.min(last))) else { continue };
                if x1 > x0 + 0.01 {
                    out.push((page, [x0, l.top, x1, l.bottom]));
                }
            }
        }
        out
    }

    /// A target's rects on `pages`, one per line, each cut to its text's height.
    fn target_rects(&mut self, t: Target, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let run = match self.mark_where(t) {
            Some(Where::Inline { para, start, end, .. }) => Some((para, start, end)),
            _ => None,
        };
        self.rects_of(t, run, pages)
    }

    /// The rects of `run` (one paragraph's units), else of the target's range.
    fn rects_of(&mut self, t: Target, run: Option<Run>, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let (lo, hi, rects) = match run {
            // Within one paragraph: its lines' own stops, with no position comparisons.
            Some((para, start, end)) => (Some((para, start)), Some((para, end)), self.para_rects(para, start, end, pages)),
            None => {
                let Some(w) = self.mark_where(t) else { return Vec::new() };
                let Some((a, b)) = self.element_range(&w) else { return Vec::new() };
                (a.para_offset(), b.para_offset(), self.range_rects(a, b, pages))
            }
        };
        let mut out = Vec::with_capacity(rects.len());
        for (page, r) in rects {
            let band = self.pages.page(page, &self.index).and_then(|geo| {
                let l = geo.lines.iter().find(|l| (l.top - r[1]).abs() < 0.01 && (l.bottom - r[3]).abs() < 0.01)?;
                l.ink
                    .iter()
                    .filter(|(para, u0, u1, _, _)| {
                        let after_start = lo.is_none_or(|(pa, ua)| *para != pa || u1.0 >= ua.0);
                        let before_end = hi.is_none_or(|(pb, ub)| *para != pb || u0.0 < ub.0);
                        l.shows(*para) && after_start && before_end
                    })
                    .fold(None, |m: Option<(f64, f64)>, &(_, _, _, top, bottom)| {
                        Some(m.map_or((top, bottom), |(t0, b0)| (t0.min(top), b0.max(bottom))))
                    })
            });
            out.push(match band {
                Some((top, bottom)) => (page, [r[0], top.max(r[1]), r[2], bottom.min(r[3])]),
                None => (page, r),
            });
        }
        out
    }

    fn state_of(&self, t: Target) -> FieldState {
        if self.selected_target() == Some(t) {
            FieldState::Selected
        } else if self.unmapped(t) {
            FieldState::Unmapped
        } else if self.hovered(t) {
            FieldState::Hover
        } else {
            FieldState::Rest
        }
    }

    fn hovered(&self, t: Target) -> bool {
        self.affordances.hovered == Some(t) || self.affordances.highlight == Some(t.element())
    }

    /// A merge field whose name the data source lacks.
    fn unmapped(&self, t: Target) -> bool {
        matches!(t, Target::Field(_))
            && !self.affordances.unmapped.is_empty()
            && self.describe_text(t).1.is_some_and(|k| self.affordances.unmapped.contains(&k))
    }

    pub(super) fn set_unmapped_fields(&mut self, u: crate::api::command::UnmappedFields) -> Outcome {
        self.affordances.unmapped = u.names.into_iter().collect();
        Outcome::Moved
    }

    /// An IF field shown as its result, not inside another field: its paragraph and span.
    fn if_span(&self, f: FieldId) -> Option<(ParaId, crate::docx::fields::FieldSpan)> {
        let Where::Inline { para, first, .. } = self.locate(ElementId::Field(f))? else { return None };
        let p = self.index.para(&self.doc, para)?;
        let Inline::FieldBegin { kind: Some(FieldKind::If), view: FieldView::Result, .. } = &p.inlines[first] else { return None };
        let nested = crate::docx::fields::spans(&p.inlines).into_iter().any(|o| o.begin < first && first < o.end);
        (!nested).then(|| crate::docx::fields::span_at(&p.inlines, first)).flatten().map(|s| (para, s))
    }

    /// An IF's tab: `IF {Fee} > 0 · else: nothing`, its nested fields by name.
    fn if_text(&self, f: FieldId) -> Option<String> {
        let (para, s) = self.if_span(f)?;
        let p = self.index.para(&self.doc, para)?;
        let (mut code, mut nested, mut depth) = (String::new(), String::new(), 0usize);
        for i in &p.inlines[s.begin + 1..s.code_end()] {
            match i {
                Inline::FieldBegin { .. } => depth += 1,
                Inline::FieldSep { .. } => depth += 0,
                Inline::FieldEnd { .. } => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let name = crate::docx::fields::parse(&nested).args.first().cloned().unwrap_or_default();
                        code.push_str(&format!("{{{name}}}"));
                        nested.clear();
                    }
                }
                Inline::Text(t, _) if depth == 0 => code.push_str(t),
                Inline::Text(t, _) if depth == 1 => nested.push_str(t),
                _ => {}
            }
        }
        let args = crate::docx::fields::parse(&code).args;
        let condition = args.iter().take(3).cloned().collect::<Vec<_>>().join(" ");
        let otherwise = args.get(4).filter(|x| !x.is_empty()).cloned().unwrap_or_else(|| "nothing".into());
        Some(format!("IF {condition} \u{b7} else: {otherwise}"))
    }

    /// The IF fields shown on `page`: their tint, and their tab while hovered or selected.
    fn if_views(&mut self, page: usize) -> Vec<AffordanceView> {
        let u = self.view_settings.u();
        let Some(geo) = self.pages.page(page, &self.index) else { return Vec::new() };
        let mut seen = HashSet::new();
        let mut ifs = Vec::new();
        for para in geo.lines.iter().flat_map(|l| l.paras.iter().copied()) {
            if !seen.insert(para) {
                continue;
            }
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            for s in crate::docx::fields::spans(&p.inlines) {
                if let Inline::FieldBegin { id, kind: Some(FieldKind::If), .. } = &p.inlines[s.begin]
                    && self.if_span(*id).is_some()
                {
                    ifs.push(*id);
                }
            }
        }
        let mut out = Vec::new();
        for f in ifs {
            let target = Target::IfField(f);
            let rects: Vec<[f64; 4]> = self.target_rects(target, page..page + 1).into_iter().map(|r| r.1).collect();
            let Some(&first) = rects.first() else { continue };
            let state = self.state_of(target);
            let tab = matches!(state, FieldState::Hover | FieldState::Selected).then(|| self.if_text(f)).flatten().map(|text| {
                let w = (mono_width(&text, 11.0) + 12.0) * u;
                Tab { rect: [first[0], first[1] - 16.0 * u, first[0] + w, first[1]], icon: None, text }
            });
            out.push(AffordanceView::IfRegion { page, target, rects, tab, state });
        }
        out
    }

    /// The placeholder the selection covers exactly, when the affordances are on: an input block in any mode, the
    /// others in design mode.
    pub(crate) fn selected_target(&self) -> Option<Target> {
        if !self.view_settings.placeholder_ui {
            return None;
        }
        if let Some(t) = self.affordances.frame_selected {
            return t;
        }
        if let Some(c) = self.selected_block() {
            return Some(Target::InputBlock(c));
        }
        if !self.affordances_on() || matches!(self.selection, Selection::Caret(_)) {
            return None;
        }
        if let Some((t, sel)) = self.affordances.selected
            && sel == self.selection
        {
            return Some(t);
        }
        let ctx = super::elements::context(&self.doc, &self.index, &self.selection);
        match *ctx.first()? {
            (e, true) => self.target_of(e).filter(|t| !matches!(t, Target::RepeatingSection(_))),
            (_, false) => None,
        }
    }

    /// What the affordances draw on `pages`: nothing unless they're on, and only shown sub-templates' marks outside
    /// design mode.
    pub(super) fn affordance_views(&mut self, pages: std::ops::Range<usize>) -> Vec<AffordanceView> {
        if !self.view_settings.placeholder_ui {
            return Vec::new();
        }
        self.affordances.frame_selected = Some(self.selected_target());
        let out = self.views_on(pages);
        self.affordances.frame_selected = None;
        out
    }

    fn views_on(&mut self, pages: std::ops::Range<usize>) -> Vec<AffordanceView> {
        let mut out = Vec::new();
        for page in pages {
            out.extend(self.sub_template_views(page));
            out.extend(self.block_views(page));
            if !self.affordances_on() {
                continue;
            }
            let mut fields = Vec::new();
            for (t, run) in self.page_targets(page) {
                let on: Vec<[f64; 4]> = self.rects_of(t, run, page..page + 1).into_iter().map(|r| r.1).collect();
                if on.is_empty() {
                    continue;
                }
                // A field starts (ends) on this page when one of its lines here stops at its first (last) unit.
                let ends = match run {
                    Some((para, a, b)) => Some((Some((para, a)), Some((para, b)))),
                    None => self.mark_where(t).and_then(|w| self.element_range(&w)).map(|(a, b)| (a.para_offset(), b.para_offset())),
                };
                let geo = self.pages.page(page, &self.index);
                let stops_at = |at: Option<(crate::layout::ParaId, Unit)>| {
                    at.is_none_or(|(para, unit)| {
                        geo.as_ref().is_none_or(|g| g.lines.iter().any(|l| l.stops.iter().any(|s| s.para == para && s.unit == unit)))
                    })
                };
                let (starts_here, ends_here) = match ends {
                    Some((a, b)) => (stops_at(a), stops_at(b)),
                    None => (true, true),
                };
                let n = on.len();
                let segments = on
                    .iter()
                    .enumerate()
                    .map(|(k, r)| Segment {
                        rect: *r,
                        round_left: k == 0 && starts_here,
                        round_right: k + 1 == n && ends_here,
                        gap_left: false,
                        gap_right: false,
                    })
                    .collect();
                let state = self.state_of(t);
                fields.push(AffordanceView::Field { page, target: t, segments, state, on_tint: false });
            }
            separate(&mut fields);
            let regions = self.section_views(page);
            let tints: Vec<[f64; 4]> =
                regions.iter().flat_map(|r| if let AffordanceView::Region { rows, .. } = r { rows.clone() } else { Vec::new() }).collect();
            let overlaps = |a: &[f64; 4], b: &[f64; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
            for f in fields.iter_mut() {
                if let AffordanceView::Field { segments, on_tint, .. } = f {
                    *on_tint = segments.iter().any(|s| tints.iter().any(|r| overlaps(&s.rect, r)));
                }
            }
            let ifs = self.if_views(page);
            let if_tints: Vec<[f64; 4]> =
                ifs.iter().flat_map(|r| if let AffordanceView::IfRegion { rects, .. } = r { rects.clone() } else { Vec::new() }).collect();
            for f in fields.iter_mut() {
                if let AffordanceView::Field { segments, on_tint, .. } = f {
                    *on_tint |= segments.iter().any(|s| if_tints.iter().any(|r| overlaps(&s.rect, r)));
                }
            }
            out.extend(regions);
            out.extend(ifs);
            out.extend(fields);
        }
        out
    }

    /// The sub-template references on `page`, as laid out: each card's or shown body's box, page points.
    fn page_expansions(&self, page: usize) -> Vec<([f64; 4], std::rc::Rc<crate::layout::expansion::Expansion>)> {
        let snapshot = self.layout.snapshot();
        let Some((frag, _)) = snapshot.page(page) else { return Vec::new() };
        let mut out = Vec::new();
        for child in frag.children.iter() {
            let crate::layout::fragment::PageChild::Body(b) = child else { continue };
            let (bx, by) = b.offset;
            crate::layout::traverse::lines_in(&b.children, &mut |_, l, dx, dy| {
                for (r, e) in l.line.expansions.iter() {
                    let (x0, y0) = (r[0] + dx + l.x + bx, r[1] + dy + l.y + by);
                    out.push(([x0, y0, x0 + e.size.0, y0 + e.size.1], e.clone()));
                }
            });
        }
        out
    }

    /// A sub-template's category and name as the versions are keyed: its part's, else the first rule's category.
    fn version_of(&self, r: &super::sub_templates::Reference) -> (String, Option<&crate::api::command::SubTemplateVersion>) {
        let part = r.part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize));
        let category = part.map(|b| b.category.clone()).or_else(|| self.sub_template_rules().first().map(|x| x.category.clone())).unwrap_or_default();
        let name = part.map_or(r.name.clone(), |b| b.name.clone());
        let v = self.affordances.versions.get(&(category.clone(), name));
        (category, v)
    }

    /// The cards and shown bodies' marks on `page`.
    fn sub_template_views(&mut self, page: usize) -> Vec<AffordanceView> {
        let u = self.view_settings.u();
        let mut out = Vec::new();
        for (rect, e) in self.page_expansions(page) {
            let Some(r) = self.references.by_field.get(&e.field).cloned() else { continue };
            let target = Target::SubTemplate(e.field);
            let (_, version) = self.version_of(&r);
            let (version, newer) = (version.map(|v| v.version.clone()), version.and_then(|v| v.newer.clone()));
            if e.content.is_empty() {
                let card = [rect[0], rect[1] + super::affordances::CARD_GAP, rect[2], rect[3] - super::affordances::CARD_GAP];
                let fields = r.part.map_or(0, |k| {
                    super::elements::enumerate(&self.doc, &self.index, &super::elements::Filter::default())
                        .iter()
                        .filter(|x| x.story == super::position::StoryId::BuildingBlock(k))
                        .count()
                });
                let (resolution, line) = match &r.resolution {
                    Resolution::Resolved => {
                        let v = version.map(|v| format!(" \u{b7} {v}")).unwrap_or_default();
                        (CardState::Resolved, format!("sub-template{v} \u{b7} {fields} fields"))
                    }
                    Resolution::Missing => (CardState::Missing, "not found".into()),
                    Resolution::NearMatch(n) => (CardState::NearMatch, format!("did you mean {n}?")),
                    Resolution::Cycle => (CardState::Cycle, "contains itself".into()),
                    Resolution::TooDeep | Resolution::NotAlone | Resolution::NotInBody => (CardState::TooDeep, "nested too deep".into()),
                };
                let expand = (resolution == CardState::Resolved).then(|| {
                    let w = (mono_width("Expand", 11.0) + 12.0) * u;
                    let mid = (card[1] + card[3]) / 2.0;
                    [card[2] - 8.0 * u - w, mid - 9.0 * u, card[2] - 8.0 * u, mid + 9.0 * u]
                });
                let state = self.state_of(target);
                out.push(AffordanceView::Card { page, target, rect: card, name: r.name.clone(), line, resolution, expand, state });
            } else if e.band > 0.0 {
                let x = rect[0] - 12.0 * u;
                let bar = [x, rect[1], x + 2.0 * u, rect[3]];
                let pieces = [
                    (Some(Part::Collapse), r.name.clone()),
                    (None, format!("{} \u{b7} read-only \u{b7} ", version.map(|v| format!(" \u{b7} {v}")).unwrap_or_default())),
                    (Some(Part::Open), "Open \u{2197}".to_string()),
                ];
                let newer = newer.map(|n| [(None, " \u{b7} ".to_string()), (Some(Part::Update), n)]);
                let mut at = x + (6.0 + 14.0) * u;
                let (mut parts, mut text) = (Vec::new(), String::new());
                let h = 16.0 * u;
                for (part, s) in pieces.into_iter().chain(newer.into_iter().flatten()) {
                    let w = mono_width(&s, 11.0) * u;
                    if let Some(p) = part {
                        parts.push((p, [at, rect[1], at + w, rect[1] + h]));
                    }
                    text.push_str(&s);
                    at += w;
                }
                let tab = Tab { rect: [x, rect[1], at + 6.0 * u, rect[1] + h], icon: Some(Icon::Copy), text };
                out.push(AffordanceView::BodyMark { page, target, bar, tab, parts });
            }
        }
        out
    }

    /// The picture placeholders on `page` as input blocks.
    fn block_views(&mut self, page: usize) -> Vec<AffordanceView> {
        let u = self.view_settings.u();
        let Some(geo) = self.pages.page(page, &self.index) else { return Vec::new() };
        let mut all: Vec<ControlId> = self.doc.controls.iter().filter(|(_, k)| k.props.kind == ControlKind::Picture).map(|(c, _)| *c).collect();
        all.sort_unstable();
        let mut out = Vec::new();
        for c in all {
            let Some((para, unit, image)) = self.block_picture(c) else { continue };
            let Some(rect) = self.inline_box(&geo, para, unit) else { continue };
            let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
            let key = self.doc.placeholder_keys.get(&image).cloned().unwrap_or_default();
            let rule = self.looks.find(ElementId::Control(c), props.tag.as_deref(), Some(&key)).cloned();
            let (look_label, icon, picture) = match rule.as_ref().map(|r| &r.look) {
                Some(super::merge::Look::Box(l)) => (l.label.clone(), l.icon.clone(), None),
                Some(super::merge::Look::Image(b)) => (None, None, Some(b.clone())),
                None => (None, None, None),
            };
            let label = look_label.or(props.alias.clone()).unwrap_or_else(|| key.clone());
            let target = Target::InputBlock(c);
            let hovered = self.affordances.hovered == Some(target) || self.affordances.highlight == Some(target.element());
            let state = if self.selected_block() == Some(c) {
                BlockState::Selected
            } else if hovered {
                BlockState::Hover
            } else {
                BlockState::Rest
            };
            let actions = rule.as_ref().map(|r| r.actions.clone()).unwrap_or_default();
            let buttons = if state == BlockState::Selected { buttons(&actions, rect, u) } else { Vec::new() };
            let hint = rule.as_ref().and_then(|r| r.hint.clone()).unwrap_or_else(|| "Click to add".into());
            let base = rule.as_ref().and_then(|r| r.base);
            out.push(AffordanceView::Block(Box::new(Block { page, target, rect, base, label, icon, image: picture, hint, buttons, state })));
        }
        out
    }

    /// A press on a card's Expand or a shown body's tab: show, collapse, open, or ask for the newer version.
    pub(super) fn affordance_click(&mut self, x: f64, y: f64, fonts: &crate::font::Fonts) -> bool {
        if !self.view_settings.placeholder_ui {
            return false;
        }
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let inside = |r: &[f64; 4]| r[0] <= px && px <= r[2] && r[1] <= py && py <= r[3];
        // An input block's button: an activation naming it (never in design mode).
        if self.mode != super::Mode::Design {
            let pressed = self.block_views(page).into_iter().find_map(|v| {
                let AffordanceView::Block(b) = v else { return None };
                let Target::InputBlock(c) = b.target else { return None };
                let action = b.buttons.iter().find(|x| inside(&x.rect)).map(|x| x.id.clone())?;
                let image = self.block_picture(c)?.2;
                Some((self.doc.placeholder_keys.get(&image).cloned().unwrap_or_default(), action))
            });
            if let Some((key, action)) = pressed {
                self.events.push(super::input::Notice::Placeholder { key, action: Some(action) });
                return true;
            }
        }
        let hit = self.sub_template_views(page).into_iter().find_map(|v| match v {
            AffordanceView::Card { target: Target::SubTemplate(f), expand: Some(r), .. } if inside(&r) => Some((f, Part::Expand)),
            AffordanceView::BodyMark { target: Target::SubTemplate(f), parts, .. } => parts.into_iter().find(|(_, r)| inside(r)).map(|(p, _)| (f, p)),
            AffordanceView::Field { .. }
            | AffordanceView::Region { .. }
            | AffordanceView::Card { .. }
            | AffordanceView::BodyMark { .. }
            | AffordanceView::Block(_)
            | AffordanceView::IfRegion { .. } => None,
        });
        let Some((f, part)) = hit else { return false };
        let id = super::merge::api_id(ElementId::Field(f));
        match part {
            Part::Expand | Part::Collapse => {
                let expanded = part == Part::Expand;
                self.set_sub_template_view(crate::api::command::SubTemplateView { ids: Some(vec![id]), expanded });
            }
            Part::Open => {
                if let Some(k) = self.references.by_field.get(&f).and_then(|r| r.part) {
                    self.enter_sub_template(k, Some(f));
                }
            }
            Part::Update => {
                let Some(r) = self.references.by_field.get(&f).cloned() else { return false };
                let (category, _) = self.version_of(&r);
                let name = r.part.and_then(|k| self.doc.glossary.as_ref()?.parts.get(k as usize)).map_or(r.name.clone(), |b| b.name.clone());
                self.events.push(super::input::Notice::SubTemplateUpdate { name, category });
            }
        }
        true
    }

    pub(super) fn set_sub_template_versions(&mut self, v: crate::api::command::SubTemplateVersions) -> Outcome {
        let first = self.sub_template_rules().first().map(|r| r.category.clone()).unwrap_or_default();
        self.affordances.versions =
            v.versions.into_iter().map(|x| ((x.category.clone().unwrap_or_else(|| first.clone()), x.name.clone()), x)).collect();
        Outcome::Moved
    }

    /// The repeating sections with content on `page`, each with the sections around it (innermost first).
    fn sections_on(&mut self, page: usize) -> Vec<(ControlId, Where, Vec<ControlId>)> {
        let mut all: Vec<ControlId> =
            self.doc.controls.iter().filter(|(_, k)| k.props.kind == ControlKind::RepeatingSection).map(|(c, _)| *c).collect();
        all.sort_unstable();
        let mut out = Vec::new();
        for c in all {
            let Some(w) = self.locate(ElementId::Control(c)) else { continue };
            let Some((a, _)) = self.element_range(&w) else { continue };
            let around: Vec<ControlId> = super::elements::context(&self.doc, &self.index, &Selection::Caret(a))
                .into_iter()
                .filter_map(|(e, _)| if let ElementId::Control(x) = e { Some(x) } else { None })
                .filter(|x| *x != c && self.doc.controls.get(x).is_some_and(|k| k.props.kind == ControlKind::RepeatingSection))
                .collect();
            if !self.section_rects(&w, page).1.is_empty() {
                out.push((c, w, around));
            }
        }
        out
    }

    /// A section's rows on `page` (none for a section of paragraphs), and everything of it there: rows, or lines.
    fn section_rects(&mut self, w: &Where, page: usize) -> (Vec<[f64; 4]>, Vec<[f64; 4]>) {
        match w {
            Where::Rows { table, range } => {
                let Some(geo) = self.pages.page(page, &self.index) else { return (Vec::new(), Vec::new()) };
                let rows: Vec<[f64; 4]> =
                    geo.rows.iter().filter(|r| r.table == *table && range.contains(&r.row) && !r.repeat).map(|r| r.rect).collect();
                (rows.clone(), rows)
            }
            _ => match self.element_range(w) {
                Some((a, b)) => (Vec::new(), self.range_rects(a, b, page..page + 1).into_iter().map(|r| r.1).collect()),
                None => (Vec::new(), Vec::new()),
            },
        }
    }

    /// The repeating sections' bars, tabs and row tints on `page`.
    fn section_views(&mut self, page: usize) -> Vec<AffordanceView> {
        let u = self.view_settings.u();
        let sections = self.sections_on(page);
        let mut lefts: HashMap<ControlId, f64> = HashMap::new();
        let mut spans: Vec<(Vec<[f64; 4]>, [f64; 4])> = Vec::new();
        for (c, w, _) in &sections {
            let (rows, all) = self.section_rects(w, page);
            let span =
                all.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |m, r| [m[0].min(r[0]), m[1].min(r[1]), m[2].max(r[2]), m[3].max(r[3])]);
            lefts.insert(*c, span[0]);
            spans.push((rows, span));
        }
        let mut out = Vec::new();
        let tables: HashSet<ControlId> = sections.iter().filter(|(_, w, _)| matches!(w, Where::Rows { .. })).map(|(c, ..)| *c).collect();
        for ((c, w, around), (rows, span)) in sections.iter().zip(spans) {
            // Nested sections stack outward from the outermost one's edge; beside a table, clear of its row
            // handles (22 px).
            let left = around.iter().filter_map(|a| lefts.get(a)).fold(span[0], |m, x| m.min(*x));
            let gap = if tables.contains(c) || around.iter().any(|a| tables.contains(a)) { 28.0 } else { 12.0 };
            let x = left - (gap + 8.0 * around.len() as f64) * u;
            let bar = [x, span[1], x + 2.0 * u, span[3]];
            let first_page = self.element_range(w).and_then(|(a, _)| self.caret_at(a)).map(|cv| cv.page);
            let tab = first_page.is_none_or(|p| p == page).then(|| {
                let props = self.doc.controls.get(c).map(|k| k.props.clone()).unwrap_or_default();
                let title = props.alias.or(props.tag).unwrap_or_else(|| "Repeating section".into());
                let nested = sections.iter().filter(|(_, _, a)| a.contains(c)).count();
                let text = if nested > 0 { format!("each {title} \u{b7} {} levels", nested + 1) } else { format!("each {title}") };
                let (width, h) = ((6.0 + 10.0 + 4.0 + mono_width(&text, 11.0) + 6.0) * u, 16.0 * u);
                let banded = match w {
                    Where::Rows { range, .. } => range.start == 0,
                    _ => true,
                };
                let rect = if banded { [x, span[1] - h, x + width, span[1]] } else { [x - width - 2.0 * u, span[1], x - 2.0 * u, span[1] + h] };
                Tab { rect, icon: Some(Icon::Repeat), text }
            });
            let target = Target::RepeatingSection(*c);
            out.push(AffordanceView::Region { page, target, bar, tab, rows, state: self.state_of(target) });
        }
        out
    }

    /// A target's label, key, code and type text.
    fn describe_text(&self, t: Target) -> (String, Option<String>, Option<String>, String) {
        let none = || (String::new(), None, None, String::new());
        match t {
            Target::Field(f) => {
                let Some(Where::Inline { para, first, .. }) = self.locate(ElementId::Field(f)) else { return none() };
                let Some(p) = self.index.para(&self.doc, para) else { return none() };
                let Some(s) = crate::docx::fields::span_at(&p.inlines, first) else { return none() };
                let code = crate::docx::fields::code_text(&p.inlines, s).trim().to_string();
                let parsed = crate::docx::fields::parse(&code);
                let label = match &p.inlines[first] {
                    Inline::FieldBegin { label: Some(l), .. } => l.to_string(),
                    _ => crate::docx::fields::result_text(&p.inlines, s),
                };
                let type_text = self.affordances.field_types.get(&code).cloned().unwrap_or(parsed.name);
                (label, parsed.args.first().cloned(), Some(code), type_text)
            }
            Target::IfField(f) => {
                let Some((para, s)) = self.if_span(f) else { return none() };
                let Some(p) = self.index.para(&self.doc, para) else { return none() };
                let code = crate::docx::fields::code_text(&p.inlines, s).trim().to_string();
                (crate::docx::fields::result_text(&p.inlines, s), None, Some(code), "IF".into())
            }
            Target::InputBlock(c) => {
                let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
                let key = self.block_picture(c).and_then(|(_, _, i)| self.doc.placeholder_keys.get(&i).cloned());
                (props.alias.or_else(|| key.clone()).unwrap_or_default(), key, None, "input block".into())
            }
            Target::SubTemplate(f) => {
                let Some(r) = self.references.by_field.get(&f) else { return none() };
                let code = match self.locate(ElementId::Field(f)) {
                    Some(Where::Inline { para, first, .. }) => self.index.para(&self.doc, para).and_then(|p| {
                        let s = crate::docx::fields::span_at(&p.inlines, first)?;
                        Some(crate::docx::fields::code_text(&p.inlines, s).trim().to_string())
                    }),
                    _ => None,
                };
                (r.name.clone(), Some(r.name.clone()), code, "sub-template".into())
            }
            Target::Control(c) | Target::RepeatingSection(c) => {
                let Some(k) = self.doc.controls.get(&c) else { return none() };
                let props = &k.props;
                let section = props.kind == ControlKind::RepeatingSection;
                let fallback = if section { "Repeating section" } else { "" };
                let label = props.alias.clone().or_else(|| props.tag.clone()).unwrap_or_else(|| fallback.into());
                let key = props.tag.clone().or_else(|| props.alias.clone());
                let kind = if section { "repeating section" } else { control_kind_name(&props.kind).unwrap_or_default() };
                (label, key, None, kind.to_string())
            }
        }
    }

    /// The pages a target's text shows on, from its start's to its end's.
    fn target_pages(&mut self, t: Target) -> std::ops::Range<usize> {
        let ends = self.mark_where(t).and_then(|w| self.element_range(&w));
        let pages = ends.map(|(a, b)| (self.caret_at(a).map(|c| c.page), self.caret_at(b).map(|c| c.page)));
        match pages {
            Some((Some(a), Some(b))) => a.min(b)..a.max(b) + 1,
            _ => 0..self.pages.len(),
        }
    }

    /// A target described for the host, anchored where it shows.
    /// The hovered field mark's tooltip, for the host to show: its key and type text, over its last line.
    pub(super) fn placeholder_tooltip(&mut self) -> Option<super::view::Tooltip> {
        let t = self.affordances.hovered.filter(|t| self.view_settings.placeholder_ui && matches!(t, Target::Field(_) | Target::Control(_)))?;
        let d = self.describe(t)?;
        let key = d.key.unwrap_or(d.label);
        let text = if d.type_text.is_empty() { key } else { format!("{key} \u{b7} {}", d.type_text) };
        Some(super::view::Tooltip { text, page: d.anchor.0, rect: d.anchor.1 })
    }

    pub(crate) fn describe(&mut self, t: Target) -> Option<Described> {
        if let Target::InputBlock(c) = t {
            let (para, unit, image) = self.block_picture(c)?;
            let page = self
                .caret_at(super::position::Position::text(
                    self.index.para_slot(para).map_or(super::position::StoryId::Body, |s| self.index.story(s.container)),
                    para,
                    unit,
                ))?
                .page;
            let geo = self.pages.page(page, &self.index)?;
            let rect = self.inline_box(&geo, para, unit)?;
            let props = self.doc.controls.get(&c).map(|k| k.props.clone()).unwrap_or_default();
            let key = self.doc.placeholder_keys.get(&image).cloned();
            let label = props.alias.or_else(|| key.clone()).unwrap_or_default();
            return Some(Described { target: t, label, key, code: None, type_text: "input block".into(), unmapped: false, anchor: (page, rect) });
        }
        if let Target::SubTemplate(_) = t {
            let (label, key, code, type_text) = self.describe_text(t);
            let pages = self.target_pages(t);
            let anchor = pages.into_iter().find_map(|p| {
                self.sub_template_views(p).into_iter().find_map(|v| match v {
                    AffordanceView::Card { target, rect, .. } if target == t => Some((p, rect)),
                    AffordanceView::BodyMark { target, tab, .. } if target == t => Some((p, tab.rect)),
                    AffordanceView::Field { .. }
                    | AffordanceView::Region { .. }
                    | AffordanceView::Card { .. }
                    | AffordanceView::BodyMark { .. }
                    | AffordanceView::Block(_)
                    | AffordanceView::IfRegion { .. } => None,
                })
            })?;
            return Some(Described { target: t, label, key, code, type_text, unmapped: false, anchor });
        }
        if let Target::RepeatingSection(_) = t {
            let (label, key, _, type_text) = self.describe_text(t);
            let tab = self.target_pages(t).find_map(|p| {
                self.section_views(p).into_iter().find_map(|v| match v {
                    AffordanceView::Region { target, tab: Some(tab), .. } if target == t => Some((p, tab.rect)),
                    AffordanceView::Field { .. }
                    | AffordanceView::Region { .. }
                    | AffordanceView::Card { .. }
                    | AffordanceView::BodyMark { .. }
                    | AffordanceView::Block(_)
                    | AffordanceView::IfRegion { .. } => None,
                })
            })?;
            return Some(Described { target: t, label, key, code: None, type_text, unmapped: false, anchor: tab });
        }
        let pages = self.target_pages(t);
        let rects = self.target_rects(t, pages);
        let anchor = match t {
            Target::Field(_) => *rects.last()?,
            Target::Control(_) | Target::RepeatingSection(_) | Target::SubTemplate(_) | Target::InputBlock(_) | Target::IfField(_) => {
                let &(page, r0) = rects.first()?;
                let union =
                    rects.iter().filter(|r| r.0 == page).fold(r0, |u, (_, r)| [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])]);
                (page, union)
            }
        };
        let (label, key, code, type_text) = self.describe_text(t);
        let unmapped = matches!(t, Target::Field(_)) && key.as_ref().is_some_and(|k| self.affordances.unmapped.contains(k));
        Some(Described { target: t, label, key, code, type_text, unmapped, anchor })
    }

    pub(super) fn set_field_types(&mut self, t: crate::api::command::FieldTypes) -> Outcome {
        if !t.merge.unwrap_or(false) {
            self.affordances.field_types.clear();
        }
        self.affordances.field_types.extend(t.types.into_iter().map(|(code, text)| (code.trim().to_string(), text)));
        Outcome::Moved
    }

    pub(super) fn highlight_element(&mut self, id: Option<&crate::api::value::ElementId>) -> Outcome {
        let Some(id) = id else {
            self.affordances.highlight = None;
            return Outcome::Moved;
        };
        match super::elements::parse_id(&id.0).and_then(|e| self.target_of(e)) {
            Some(t) => {
                self.affordances.highlight = Some(t.element());
                Outcome::Moved
            }
            None => Outcome::invalid("not a placeholder"),
        }
    }

    /// Every placeholder in document order: the body, then headers, footers and notes, as the element list orders them.
    fn all_targets(&self) -> Vec<Target> {
        super::elements::enumerate(&self.doc, &self.index, &super::elements::Filter::default())
            .into_iter()
            .filter_map(|e| self.target_of(e.id))
            .collect()
    }

    /// Selects the placeholder after (or before) the selected one, wrapping at the ends.
    pub(super) fn select_placeholder(&mut self, forward: bool, fonts: &crate::font::Fonts) -> Outcome {
        if !self.affordances_on() {
            return Outcome::unavailable("placeholders aren't drawn");
        }
        let all = self.all_targets();
        let Some(&first) = all.first() else { return Outcome::Unchanged("no placeholders") };
        let here = self.selected_target().and_then(|t| all.iter().position(|&x| x == t));
        let next = match (here, forward) {
            (Some(k), true) => all[(k + 1) % all.len()],
            (Some(k), false) => all[(k + all.len() - 1) % all.len()],
            (None, true) => first,
            (None, false) => all[all.len() - 1],
        };
        let id = crate::api::value::ElementId(super::elements::id_str(next.element()));
        let out = self.select_element(&id, true, fonts);
        self.note_selected(next.element());
        out
    }

    /// Esc with a placeholder selected: the caret goes after it.
    pub(super) fn collapse_after_placeholder(&mut self) -> bool {
        if self.selected_target().is_none() {
            return false;
        }
        let Some((_, b)) = self.selection.ordered(&self.index) else { return false };
        self.set_selection(Selection::Caret(b));
        true
    }

    /// The room kept above each repeating section for its tab, in design mode with the affordances on: before a
    /// section's first paragraph, or before its table when it starts at the table's first row or holds the whole
    /// table. A section starting at a later row has none: its tab goes beside the row.
    pub(super) fn design_bands(&self) -> (HashMap<ParaId, f64>, HashMap<TableId, f64>) {
        let (mut paras, mut tables) = (HashMap::new(), HashMap::new());
        if !self.affordances_on() {
            return (paras, tables);
        }
        for (c, k) in &self.doc.controls {
            if k.props.kind != ControlKind::RepeatingSection {
                continue;
            }
            match self.locate(ElementId::Control(*c)) {
                Some(Where::Rows { table, range }) if range.start == 0 => {
                    tables.insert(table, BAND);
                }
                Some(Where::Blocks { container, range }) => match self.index.blocks(&self.doc, container).and_then(|b| b.get(range.start)) {
                    Some(crate::docx::Block::Para(p)) => {
                        paras.insert(ParaId(p.id), BAND);
                    }
                    Some(crate::docx::Block::Table(t)) => {
                        tables.insert(TableId(t.id), BAND);
                    }
                    None => {}
                },
                Some(Where::Rows { .. } | Where::Inline { .. } | Where::Sym { .. }) | None => {}
            }
        }
        (paras, tables)
    }

    /// What the pointer is over, and the region it's in: a frame is drawn when either changes.
    pub fn affordance_hover(&self) -> (Option<Target>, Option<ControlId>) {
        (self.affordances.hovered.filter(|_| self.view_settings.placeholder_ui), self.hover_region)
    }

    /// The placeholder the pointer is over, from the marks drawn on its page.
    pub(super) fn hover_target(&mut self, x: f64, y: f64) {
        let page = self.stack().page_at(x, y).filter(|_| self.view_settings.placeholder_ui);
        let Some((page, px, py)) = page else {
            self.affordances.hovered = None;
            return;
        };
        self.affordances.hovered = self.affordance_at(page, px, py);
    }

    /// The placeholder drawn at a page point: a field's mark, else a tab.
    fn affordance_at(&mut self, page: usize, px: f64, py: f64) -> Option<Target> {
        let inside = |r: &[f64; 4]| r[0] <= px && px <= r[2] && r[1] <= py && py <= r[3];
        let views = self.affordance_views(page..page + 1);
        views.iter().rev().find_map(|v| match v {
            AffordanceView::Field { target, segments, .. } => segments.iter().any(|s| inside(&s.rect)).then_some(*target),
            AffordanceView::Region { target, tab: Some(t), .. } => inside(&t.rect).then_some(*target),
            AffordanceView::Card { target, rect, .. } => inside(rect).then_some(*target),
            AffordanceView::BodyMark { target, tab, .. } => inside(&tab.rect).then_some(*target),
            AffordanceView::Block(b) => inside(&b.rect).then_some(b.target),
            AffordanceView::IfRegion { target, rects, .. } => rects.iter().any(inside).then_some(*target),
            AffordanceView::Region { tab: None, .. } => None,
        })
    }

    /// A press on a repeating section's tab selects the section.
    pub(super) fn tab_click(&mut self, x: f64, y: f64, fonts: &crate::font::Fonts) -> bool {
        let Some((page, px, py)) = self.stack().page_at(x, y).filter(|_| self.affordances_on()) else { return false };
        self.ensure_page(page, fonts);
        let Some(t @ Target::RepeatingSection(_)) = self.affordance_at(page, px, py) else { return false };
        let id = crate::api::value::ElementId(super::elements::id_str(t.element()));
        let selected = self.select_element(&id, false, fonts) == Outcome::Moved;
        self.note_selected(t.element());
        selected
    }
}

/// A paragraph's units `start..end`.
type Run = (ParaId, Unit, Unit);

/// A `HYPERLINK` field: a link, not a placeholder.
fn is_link(inlines: &[Inline], s: crate::docx::fields::FieldSpan) -> bool {
    crate::docx::fields::code_text(inlines, s).trim_start().get(..9).is_some_and(|w| w.eq_ignore_ascii_case("HYPERLINK"))
}

/// Marks of different fields that touch on a line are drawn apart.
fn separate(views: &mut [AffordanceView]) {
    let edges: Vec<(Target, f64, f64, f64)> = views
        .iter()
        .flat_map(|v| match v {
            AffordanceView::Field { target, segments, .. } => segments.iter().map(|s| (*target, s.rect[1], s.rect[0], s.rect[2])).collect::<Vec<_>>(),
            AffordanceView::Region { .. }
            | AffordanceView::Card { .. }
            | AffordanceView::BodyMark { .. }
            | AffordanceView::Block(_)
            | AffordanceView::IfRegion { .. } => Vec::new(),
        })
        .collect();
    let touches = |t: Target, top: f64, x: f64| {
        edges.iter().any(|&(o, o_top, x0, x1)| o != t && (o_top - top).abs() < 2.0 && ((x0 - x).abs() < 0.01 || (x1 - x).abs() < 0.01))
    };
    for v in views.iter_mut() {
        let AffordanceView::Field { target, segments, .. } = v else { continue };
        for s in segments.iter_mut() {
            s.gap_left = touches(*target, s.rect[1], s.rect[0]);
            s.gap_right = touches(*target, s.rect[1], s.rect[2]);
        }
    }
}

/// A selected block's buttons, centred in it in a row: 22 px tall, 6 px apart.
fn buttons(actions: &[crate::api::command::PlaceholderAction], rect: [f64; 4], u: f64) -> Vec<Button> {
    let widths: Vec<f64> = actions.iter().map(|a| (mono_width(&a.label, 11.0) + 20.0) * u).collect();
    let total = widths.iter().sum::<f64>() + 6.0 * u * actions.len().saturating_sub(1) as f64;
    let (mut x, mid) = ((rect[0] + rect[2]) / 2.0 - total / 2.0, (rect[1] + rect[3]) / 2.0);
    let mut out = Vec::new();
    for (a, w) in actions.iter().zip(widths) {
        out.push(Button {
            id: a.id.clone(),
            label: a.label.clone(),
            primary: a.primary == Some(true),
            rect: [x, mid - 11.0 * u, x + w, mid + 11.0 * u],
        });
        x += w + 6.0 * u;
    }
    out
}
