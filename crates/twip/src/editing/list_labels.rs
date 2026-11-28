//! List number selection

use crate::api::command::Command;
use crate::docx::char_props::RunDirect;
use crate::docx::char_props::RunXml;
use crate::docx::list_xml::{ListDefDirect, ListDirect};
use crate::docx::resolvers::Resolvers;
use crate::docx::write::props::Cx;
use crate::docx::{Run, RunStyle};
use crate::font::Fonts;
use crate::format::{CharEdit, CharProps, LevelOverride, LevelProp, ListDefProp, ListLevel as Level, ListProp, OverrideProp, Set};
use crate::layout::{ParaId, Unit};
use std::rc::Rc;

use super::commands::Outcome;
use super::position::{Place, Position};
use super::selection::Selection;
use super::{Editor, list_defs};
use crate::document::paragraph::{ListId, ListRef};

/// The numbers (or bullets) of one level of a list, selected by a click on one of them: character formatting goes to
/// that level's number formatting, so every number of the level shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListLabelSelection {
    /// The paragraph whose number was clicked.
    pub para: ParaId,
    /// The list (`numId`).
    pub list: i64,
    /// The level, 0 to 8.
    pub level: i64,
}

/// A number selection and the caret it was made with: it holds while the selection is that caret.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LabelHold {
    label: ListLabelSelection,
    with: Selection,
}

impl LabelHold {
    /// Whether the number selection holds with `s` selected.
    pub(super) fn holds(&self, s: &Selection) -> bool {
        self.with == *s
    }
}

/// Whether a command formats characters: what a number selection takes.
pub(super) fn formats_characters(c: &Command) -> bool {
    use crate::api::command::CommandId as C;
    matches!(
        c.id(),
        C::SetBold
            | C::SetItalic
            | C::SetUnderline
            | C::SetStrikethrough
            | C::SetDoubleStrikethrough
            | C::SetSuperscript
            | C::SetSubscript
            | C::SetSmallCaps
            | C::SetAllCaps
            | C::SetHidden
            | C::SetFont
            | C::SetFontSize
            | C::GrowFont
            | C::ShrinkFont
            | C::SetFontColor
            | C::SetHighlight
            | C::SetCharacterSpacing
            | C::SetCharacterPosition
            | C::SetTypography
            | C::SetLanguage
            | C::SetCharacterEffects
            | C::SetTextEffects
            | C::SetUnderlineColor
            | C::SetTextBorder
    ) || matches!(c, Command::SetShading(s) if s.target == crate::api::command::ShadingTarget::Text)
}

impl Editor {
    /// The selected list numbers: set by a click on a number, held while the caret it put stays, and while the
    /// paragraph is still in that list at that level.
    pub fn list_label_selection(&self) -> Option<&ListLabelSelection> {
        let hold = self.list_label.as_ref().filter(|h| h.holds(&self.selection))?;
        let num = self.index.para(&self.doc, hold.label.para)?.style.num?;
        (num == ListRef { list: ListId(hold.label.list), level: hold.label.level }).then_some(&hold.label)
    }

    /// Selects the numbers of `para`'s list at its level, as a click on its number does: the caret goes to the
    /// paragraph's start. False when it isn't numbered or bulleted.
    pub fn select_list_label(&mut self, para: ParaId) -> bool {
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let Some(ListRef { list: ListId(list), level }) = p.style.num.filter(|ListRef { list: ListId(n), level: _ }| *n != 0) else { return false };
        if p.list.is_none() {
            return false;
        }
        let Some(slot) = self.index.para_slot(para) else { return false };
        let story = self.index.story(slot.container);
        let at = Position::text(story, para, Unit(0));
        self.set_selection(Selection::Caret(at));
        self.list_label = Some(LabelHold { label: ListLabelSelection { para, list, level }, with: self.selection });
        true
    }

    /// A press on a list number or bullet: selects the numbers of its level ([`Editor::select_list_label`]).
    pub(super) fn label_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(para) = self.label_under(x, y, fonts) else { return false };
        self.drag = None;
        self.select_list_label(para)
    }

    /// The paragraph whose number or bullet is under the point, where the caret could go.
    pub fn label_under(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<ParaId> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let (k, line) = geo.lines.iter().enumerate().find(|(_, l)| {
            let [lx, ly] = l.local([px, py]);
            !l.repeat && ly >= l.top && ly <= l.bottom && l.label.is_some_and(|(a, b)| lx >= a - 0.5 && lx <= b + 0.5)
        })?;
        let para = line.para()?;
        // The number is on the paragraph's first line.
        if geo.lines_of(para).next().map(|(i, _)| i) != Some(k) || self.index.para(&self.doc, para)?.list.is_none() {
            return None;
        }
        let hit = self.hit(x, y, fonts)?;
        matches!(hit.place, Place::Text { para: p, .. } if p == para).then_some(para)
    }

    /// The selected numbers' boxes on `pages`, to shade: every number of the list at the level.
    pub fn list_label_rects(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, super::view::Band)> {
        let Some(l) = self.list_label_selection().cloned() else { return Vec::new() };
        let mut out = Vec::new();
        for page in pages {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            // A paragraph's number is on its first line: the first that shows it, in order.
            let mut seen: std::collections::HashSet<ParaId> = std::collections::HashSet::new();
            for line in geo.lines.iter().filter(|l| !l.repeat) {
                let first = line.para().is_some_and(|p| !seen.contains(&p));
                seen.extend(line.paras.iter().copied());
                let (Some(para), Some((a, b)), true) = (line.para(), line.label, first) else { continue };
                let numbered = self
                    .index
                    .para(&self.doc, para)
                    .is_some_and(|p| p.list.is_some() && p.style.num == Some(ListRef { list: ListId(l.list), level: l.level }));
                if numbered {
                    out.push((page, line.strip(a, b)));
                }
            }
        }
        out
    }

    /// A command while numbers are selected: character formatting goes to their level ([`Editor::format_list_label`]);
    /// another character command (Clear Formatting, a character style) ends the number selection and acts on the text.
    pub(super) fn list_label_command(&mut self, cmd: &Command, fonts: &Fonts) -> Option<Outcome> {
        // Its caret left since: it ended then.
        if self.list_label.as_ref().is_some_and(|h| !h.holds(&self.selection)) {
            self.list_label = None;
        }
        if self.style_target.is_some() || self.label_target.is_some() || self.list_label.is_none() {
            return None;
        }
        if !formats_characters(cmd) {
            if crate::api::catalog::info(cmd.id()).is_some_and(|i| i.group == crate::api::catalog::CommandGroup::Character) {
                self.list_label = None;
            }
            return None;
        }
        let para = self.list_label_selection()?.para;
        Some(self.format_list_label(para, vec![cmd.clone()], fonts))
    }

    /// Runs character formatting commands on the number formatting of the list level `para` is at, as one undo step:
    /// the level the list defines itself when it does, else its definition's. Relative commands (Grow Font) and
    /// toggles start from the number as it draws in `para`.
    pub fn format_list_label(&mut self, para: ParaId, commands: Vec<Command>, fonts: &Fonts) -> Outcome {
        if let Some(c) = commands.iter().find(|c| !formats_characters(c)) {
            return Outcome::invalid(&format!("{:?} doesn't format a list's number", c.id()));
        }
        if self.mode == super::Mode::ReadOnly {
            return self.refuse_read_only();
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::unavailable("no such paragraph") };
        let Some(ListRef { list: ListId(list), level }) = p.style.num.filter(|ListRef { list: ListId(n), level: _ }| *n != 0 && p.list.is_some())
        else {
            return Outcome::unavailable("the paragraph isn't numbered");
        };
        let story = self.index.para_slot(para).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
        let at = super::position::Position::text(story, para, crate::layout::Unit(0));
        if self.refuse_locked_content_in(&at, &at) {
            return Outcome::Unchanged("locked");
        }
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::unavailable("no such paragraph") };
        let base = self.label_base(para, p, &res, &self.cx());
        let typed = res.typed();
        let c = &*typed.cascade;
        let ncx = typed.numbering.cx();
        let level = level.clamp(0, 8);
        let Some(m) = c.lists.lists.get(&ListId(list)) else { return Outcome::unavailable("no such list") };
        let own = m.override_of(level).and_then(|o| o.def.clone());
        let Some(abstract_id) = c.abstract_of(ListId(list)) else { return Outcome::unavailable("no such list") };
        // A level other lists share stays theirs: this list's formatting goes in an override of its own.
        let write_own = own.is_some() || c.lists.lists.keys().any(|&n| n != ListId(list) && c.abstract_of(n) == Some(abstract_id));
        let Some(lvl) = own.clone().or_else(|| c.lists.defs.get(&abstract_id)?.levels[level as usize].clone()) else {
            return Outcome::unavailable("the list doesn't define that level");
        };
        let (Some(def), Some(num)) = (list_defs::def_holder(&typed, abstract_id), list_defs::list_holder(&typed, ListId(list))) else {
            return Outcome::unavailable("no such list");
        };
        let start = (lvl.char != CharProps::default()).then(|| Rc::new(lvl.char.clone()));
        self.label_target = Some(Box::new(LabelTarget { cx: ncx.clone(), direct: RunDirect::typed(start), set: Vec::new(), base }));
        for c in commands {
            if let Outcome::Refused(e) = self.execute(c, fonts) {
                self.label_target = None;
                return Outcome::Refused(e);
            }
        }
        let Some(t) = self.label_target.take() else { return Outcome::Unchanged("already so") };
        let label = t.direct.props().map_or_else(CharProps::default, |p| (**p).clone());
        let edited = labelled(&lvl, label, &t.set, level, write_own.then_some(&num).ok_or(&def));
        let colours = c.colours;
        let same = match &edited {
            Edited::List(n) => n.xml(&colours, &ncx) == num.xml(&colours, &ncx),
            Edited::Def(d) => d.xml(&colours, &ncx) == def.xml(&colours, &ncx),
        };
        if same {
            return Outcome::Unchanged("already so");
        }
        self.edit_numbering(
            "Font",
            |n, _, _| {
                match edited {
                    Edited::List(m) => {
                        n.set_list(m);
                    }
                    Edited::Def(d) => {
                        n.set_list_def(d);
                    }
                }
                Some(())
            },
            &[],
            false,
            fonts,
        )
    }

    /// Runs a character formatting command's edit on the number formatting being formatted ([`LabelTarget`]).
    pub(super) fn format_label_part(
        &mut self,
        edit: &dyn Fn(&mut CharEdit, &RunStyle),
        settle: &dyn Fn(&mut CharEdit, &RunStyle) -> bool,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(t) = self.label_target.as_deref_mut() else { return Outcome::Unchanged("no list number") };
        let run = t.run(&res);
        let mut e = t.edit();
        edit(&mut e, &run);
        t.apply(e);
        let new = t.run(&res);
        let mut e = t.edit();
        if settle(&mut e, &new) {
            t.apply(e);
        }
        Outcome::Moved
    }
}

/// A list's definition or the list itself, as a label's formatting leaves it.
pub(super) enum Edited {
    /// The definition.
    Def(ListDefDirect),
    /// The list.
    List(ListDirect),
}

/// Level `level` (`lvl`) with the label properties `label`, which an edit that set `set` left: in the list's override
/// of it (`Ok`, one added where it has none) or in its definition (`Err`).
pub(super) fn labelled(lvl: &Level, label: CharProps, set: &[Set], level: i64, to: Result<&ListDirect, &ListDefDirect>) -> Edited {
    let mut l = lvl.clone();
    l.char = label;
    let l = Rc::new(l);
    let chars = set.iter().map(|s| LevelProp::Char(*s));
    match to {
        Ok(num) => {
            let mut n = (**num.get()).clone();
            match n.overrides.binary_search_by_key(&level, |o| o.level) {
                Ok(i) => n.overrides[i].def = Some(l),
                Err(i) => n.overrides.insert(i, LevelOverride { level, start: None, def: Some(l), foreign: None }),
            }
            let set: Vec<ListProp> = chars.map(|p| ListProp::Override(level, OverrideProp::Level(p))).collect();
            Edited::List(num.edited(n, &set))
        }
        Err(def) => {
            let mut d = (**def.get()).clone();
            d.levels[level as usize] = Some(l);
            let set: Vec<ListDefProp> = chars.map(|p| ListDefProp::Level(level as u8, p)).collect();
            Edited::Def(def.edited(d, &set))
        }
    }
}

/// A list level's number formatting while formatting commands edit it.
pub(super) struct LabelTarget {
    /// The numbering part's names, which the level is written with.
    pub(super) cx: Cx,
    /// The level's label properties, as the commands so far leave them.
    direct: RunDirect,
    /// What the commands so far set, in order.
    set: Vec<Set>,
    /// What the number is formatted over where it was clicked ([`Editor::label_base`]).
    base: CharProps,
}

impl LabelTarget {
    /// The number as it draws with the level's formatting as the commands so far leave it.
    fn run(&self, res: &Resolvers) -> Run {
        crate::docx::run_xml::label(res, &self.base, self.direct.props().map(|p| &**p))
    }

    /// An edit of the label properties as they are.
    fn edit(&self) -> CharEdit {
        CharEdit::new(self.direct.props().map_or_else(CharProps::default, |p| (**p).clone()))
    }

    /// The label properties `e` leaves, with what it set.
    fn apply(&mut self, e: CharEdit) {
        self.set.extend_from_slice(&e.set);
        self.direct = self.direct.edit(e);
    }
}
