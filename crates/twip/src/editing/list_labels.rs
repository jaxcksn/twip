//! List number selection

use crate::api::command::Command;
use crate::docx::props::RunProps;
use crate::docx::resolve::Resolver;
use crate::docx::write::props::{Cx, Props, RPR};
use crate::docx::{Run, RunStyle};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::{Place, Position};
use super::selection::Selection;

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
        (num == (hold.label.list, hold.label.level)).then_some(&hold.label)
    }

    /// Selects the numbers of `para`'s list at its level, as a click on its number does: the caret goes to the
    /// paragraph's start. False when it isn't numbered or bulleted.
    pub fn select_list_label(&mut self, para: ParaId) -> bool {
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let Some((list, level)) = p.style.num.filter(|(n, _)| *n != 0) else { return false };
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
                let numbered = self.index.para(&self.doc, para).is_some_and(|p| p.list.is_some() && p.style.num == Some((l.list, l.level)));
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
        let Some((list, level)) = p.style.num.filter(|(n, _)| *n != 0 && p.list.is_some()) else {
            return Outcome::unavailable("the paragraph isn't numbered");
        };
        let base = self.label_base(para, p, &res, &self.cx());
        let ncx = res.numbering.cx();
        let level = level.clamp(0, 8);
        let Some(m) = res.numbering.nums.get(&list) else { return Outcome::unavailable("no such list") };
        let own = m.level_overrides.contains_key(&level);
        let Some(abstract_id) = res.numbering.abstract_of(list, &res.styles) else { return Outcome::unavailable("no such list") };
        let abstract_raw = res.numbering.abstracts.get(&abstract_id).map(|a| a.raw.clone()).unwrap_or_default();
        let raw = if own { m.raw.clone() } else { abstract_raw.clone() };
        // A level other lists share stays theirs: this list's formatting goes in an override of its own.
        let write_own = own || res.numbering.nums.keys().any(|&n| n != list && res.numbering.abstract_of(n, &res.styles) == Some(abstract_id));
        let Some(lvl) = level_in(&raw, level, own, &ncx) else { return Outcome::unavailable("the list doesn't define that level") };
        let mut lp = Props::parse(Some(&lvl), "lvl", &ncx);
        let rpr = lp.get("rPr").map(str::to_string);
        self.label_target = Some(Box::new(LabelTarget { cx: ncx.clone(), rpr: rpr.clone(), base }));
        for c in commands {
            if let Outcome::Refused(e) = self.execute(c, fonts) {
                self.label_target = None;
                return Outcome::Refused(e);
            }
        }
        let now = self.label_target.take().and_then(|t| t.rpr);
        if now == rpr {
            return Outcome::Unchanged("already so");
        }
        match now {
            Some(x) => lp.set("rPr", x),
            None => lp.remove("rPr"),
        }
        let Some(lvl) = lp.xml() else { return Outcome::Unchanged("no level") };
        let replaced = with_level(if write_own { &m.raw } else { &raw }, level, write_own, &lvl, &ncx);
        self.edit_numbering(
            "Font",
            |n, _, colours| {
                if write_own { n.set_num_xml(replaced, colours).map(|_| ()) } else { n.set_abstract_xml(replaced, colours).map(|_| ()) }
            },
            &[],
            false,
            fonts,
        )
    }

    /// Runs a character formatting command's edit on the number formatting being formatted ([`LabelTarget`]).
    pub(super) fn format_label_part(
        &mut self,
        edit: &dyn Fn(&mut Props, &str, &RunStyle),
        settle: &dyn Fn(&mut Props, &str, &RunStyle) -> bool,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(t) = self.label_target.as_deref_mut() else { return Outcome::Unchanged("no list number") };
        let w = t.cx.w.clone();
        let mut p = Props::parse(t.rpr.as_deref(), "rPr", &t.cx);
        edit(&mut p, &w, &t.run(&res, t.rpr.as_deref()));
        let new = t.run(&res, p.xml().as_deref());
        settle(&mut p, &w, &new);
        p.reorder(RPR);
        t.rpr = p.xml();
        Outcome::Moved
    }
}

/// A list level's number formatting while formatting commands edit it.
pub(super) struct LabelTarget {
    /// The numbering part's names, which its `w:rPr` is written with.
    pub(super) cx: Cx,
    /// The level's `w:rPr`, as the commands so far leave it.
    rpr: Option<String>,
    /// What the number is formatted over where it was clicked ([`Editor::label_base`]).
    base: RunProps,
}

impl LabelTarget {
    /// The number as it draws with `rpr` as its level's formatting.
    fn run(&self, res: &Resolver, rpr: Option<&str>) -> Run {
        let own = rpr.and_then(|x| self.cx.parse(x, |_, el| RunProps::parse(Some(el), &res.styles.colours))).unwrap_or_default();
        let mut lp = self.base.clone();
        lp.merge(&own);
        res.run_style(&lp)
    }
}

/// Level `k`'s `w:lvl` in a `w:abstractNum`, or in a `w:num`'s `w:lvlOverride` (`over`).
fn level_in(raw: &str, k: i64, over: bool, cx: &Cx) -> Option<String> {
    let parent = Props::parse(Some(raw), if over { "num" } else { "abstractNum" }, cx);
    let ilvl = |x: &str| cx.parse(x, |_, n| crate::docx::xml::attr(n, "ilvl").and_then(|v| v.parse::<i64>().ok())).flatten();
    let (name, x) = parent.kids.iter().find(|(n, x)| n == if over { "lvlOverride" } else { "lvl" } && ilvl(x) == Some(k))?;
    if name == "lvlOverride" { Props::parse(Some(x), "lvlOverride", cx).get("lvl").map(str::to_string) } else { Some(x.clone()) }
}

/// The `w:abstractNum` (or `w:num`, `over`) with level `k`'s `w:lvl` replaced by `lvl`; a `w:num` without an override
/// of that level gets one, in level order.
fn with_level(raw: &str, k: i64, over: bool, lvl: &str, cx: &Cx) -> String {
    let mut parent = Props::parse(Some(raw), if over { "num" } else { "abstractNum" }, cx);
    let ilvl = |x: &str| cx.parse(x, |_, n| crate::docx::xml::attr(n, "ilvl").and_then(|v| v.parse::<i64>().ok())).flatten();
    let mut found = false;
    for (name, x) in parent.kids.iter_mut() {
        if ilvl(x) != Some(k) {
            continue;
        }
        if name == "lvlOverride" && over {
            let mut o = Props::parse(Some(x), "lvlOverride", cx);
            o.set("lvl", lvl.to_string());
            *x = o.xml().unwrap_or_default();
            found = true;
        } else if name == "lvl" && !over {
            *x = lvl.to_string();
        }
    }
    if over && !found {
        let w = &cx.w;
        let at = parent.kids.iter().position(|(n, x)| n == "lvlOverride" && ilvl(x) > Some(k)).unwrap_or(parent.kids.len());
        parent.kids.insert(at, ("lvlOverride".to_string(), format!("<{w}lvlOverride {w}ilvl=\"{k}\">{lvl}</{w}lvlOverride>")));
    }
    parent.xml().unwrap_or_else(|| raw.to_string())
}
