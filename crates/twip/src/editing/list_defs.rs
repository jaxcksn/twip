//! List definitions: the levels of the selection's list as the Define dialogs show them, and the edits that define
//! them, set numbering values, and adjust indents. Each edits the typed definitions, which the numbering part's
//! holders write over the elements they were read from.

use std::rc::Rc;

use crate::api::command::{
    FollowNumber, LevelAlignment, ListApplyTo, ListDefinitionChange, ListIndents, ListLevel, NumberFormat, NumberingMode, NumberingValue,
};
use crate::api::value::{AutoOr, Color, HighlightColor, NoneOr, Points, StyleId, UnderlineStyle};
use crate::document::format::{Leader, TabKind};
use crate::document::paragraph::{ListJc, ListSuffix};
use crate::docx::definitions::{Definitions, TypedDefs};
use crate::docx::list_xml::ListXml;
use crate::docx::list_xml::{ListDefDirect, ListDirect};
use crate::docx::props::ThemeColours;
use crate::docx::style_xml::StyleDef;
use crate::docx::style_xml::StyleXml;
use crate::docx::write::props::Cx;
use crate::font::Fonts;
use crate::format::{
    CharProps, FontHint, FontRef, LevelOverride, LevelProp, List, ListChoice, ListDef, ListDefId, ListDefProp, ListLevel as Level, ListProp, Literal,
    MultiLevelType, Origin, OverrideProp, ParaProp, ParaProps, ParaSet, Prop, ScriptClass, ScriptFonts, Set, StyleSet, TabStop, Toggle,
    UnderlineProps,
};
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::step::Step;
use crate::document::paragraph::{ListId, ListRef};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod xml;

/// A list level's number or bullet formatting of its own (`None`: not set, so the paragraph's applies). The font is
/// the level's [`ListLevel::font`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LabelFormat {
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// The underline.
    pub underline: Option<UnderlineStyle>,
    /// The size.
    pub size: Option<Points>,
    /// The colour, or automatic.
    pub color: Option<AutoOr<Color>>,
    /// The highlight, or none.
    pub highlight: Option<NoneOr<HighlightColor>>,
}

impl LabelFormat {
    /// The formatting a level's `w:rPr` gives its number.
    pub fn of(r: &crate::docx::props::RunProps) -> LabelFormat {
        use crate::docx::props::toggle;
        let flag = |bit: u16| (r.toggles_set & bit != 0).then_some(r.toggles & bit != 0);
        LabelFormat {
            bold: flag(toggle::B),
            italic: flag(toggle::I),
            underline: r.underline.map(|u| super::state::underline_style(Some(u))),
            size: r.size.map(Points),
            color: r.color.map(|c| c.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)))),
            highlight: r.highlight.map(super::state::highlight_color),
        }
    }

    /// A level's label properties `c` with the formatting that differs from `was` set or cleared, each property it
    /// sets noted whole in `set`.
    fn apply(&self, was: &LabelFormat, c: &mut CharProps, set: &mut Vec<Set>) {
        let mut whole = |prop: Prop| set.push(Set { prop, whole: true });
        if self.bold != was.bold {
            toggle(c, self.bold, [Toggle::BOLD, Toggle::BOLD_COMPLEX]);
            whole(Prop::Toggle(Toggle::BOLD));
            whole(Prop::Toggle(Toggle::BOLD_COMPLEX));
        }
        if self.italic != was.italic {
            toggle(c, self.italic, [Toggle::ITALIC, Toggle::ITALIC_COMPLEX]);
            whole(Prop::Toggle(Toggle::ITALIC));
            whole(Prop::Toggle(Toggle::ITALIC_COMPLEX));
        }
        if self.underline != was.underline {
            c.underline = self.underline.map(|u| UnderlineProps { style: super::commands::underline(u), color: None });
            whole(Prop::Underline);
        }
        if self.size != was.size {
            let half = self.size.map(|s| (s.0 * 2.0).round() / 2.0);
            (c.size, c.complex_size) = (half, half);
            whole(Prop::Size);
            whole(Prop::ComplexSize);
        }
        if self.color != was.color {
            c.color = self.color.map(|v| match v {
                AutoOr::Auto => crate::format::Color::Auto,
                AutoOr::Value(v) => crate::format::Color::Rgb(v.0 & 0xFF_FFFF),
            });
            whole(Prop::Color);
        }
        if self.highlight != was.highlight {
            c.highlight = self.highlight.map(|h| match h {
                NoneOr::Value(h) => Some(super::commands::highlight(h)),
                NoneOr::None => None,
            });
            whole(Prop::Highlight);
        }
    }
}

/// `c` with on/off properties `ts` on, off, or not set.
fn toggle(c: &mut CharProps, on: Option<bool>, ts: [Toggle; 2]) {
    for t in ts {
        match on {
            Some(v) => c.toggles.put(t, v),
            None => {
                c.toggles.set &= !t.0;
                c.toggles.on &= !t.0;
            }
        }
    }
}

/// A list level in full: what [`ListLevel`] shows, with its number style (whatever [`ListLevel::format`] can name)
/// and its number's own formatting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelDefinition {
    /// The level. Setting a level, `format` here is the one written.
    pub level: ListLevel,
    /// The number style.
    pub format: NumberFormat,
    /// The number's own formatting.
    pub label: LabelFormat,
}

impl LevelDefinition {
    /// An API level in full: its number style, and its number's own formatting (absent: not set).
    pub fn of_level(level: &ListLevel) -> LevelDefinition {
        let label = LabelFormat {
            bold: level.bold,
            italic: level.italic,
            underline: level.underline,
            size: level.size,
            color: level.color,
            highlight: level.highlight,
        };
        LevelDefinition { level: level.clone(), format: level.format.clone(), label }
    }

    /// Refused: an unwritable number style, a size out of 1 to 1638 pt.
    pub fn check(&self) -> Result<(), crate::api::error::Error> {
        self.check_changed(None)
    }

    /// [`LevelDefinition::check`] of what differs from `was`: a number style or size it was read with stays.
    fn check_changed(&self, was: Option<&LevelDefinition>) -> Result<(), crate::api::error::Error> {
        if was.is_none_or(|w| w.format != self.format) && !self.format.writable() {
            return Err(crate::api::error::Error::invalid("a number style is 1 to 64 ASCII letters and digits"));
        }
        match self.label.size.filter(|_| was.is_none_or(|w| w.label.size != self.label.size)) {
            Some(s) => super::commands::bounds::within("the size", s.0, 1.0, 1638.0),
            None => Ok(()),
        }
    }
}

/// A level as the API shows it, a theme colour's value as `colours` resolve it.
pub fn list_level(l: &Level, colours: &ThemeColours) -> ListLevel {
    level_definition(l, colours).level
}

/// A level in full, a theme colour's value as `colours` resolve it.
pub fn level_definition(l: &Level, colours: &ThemeColours) -> LevelDefinition {
    use crate::docx::list_xml::{format_name, jc_name, suffix_name};
    let left = l.para.indent_left.unwrap_or(0.0);
    let first = l.para.indent_first.unwrap_or(0.0);
    let format = NumberFormat::of_val(format_name(l.format.as_ref()));
    let rpr = crate::docx::char_props::to_run_props(&l.char, colours);
    let label = LabelFormat::of(&rpr);
    let level = ListLevel {
        format: format.clone(),
        text: l.text.as_deref().unwrap_or_default().to_string(),
        start: l.start.unwrap_or(0).max(0) as u32,
        alignment: match jc_name(l.jc.as_ref()) {
            "center" => LevelAlignment::Center,
            "right" | "end" => LevelAlignment::Right,
            _ => LevelAlignment::Left,
        },
        follow: match suffix_name(l.suffix.as_ref()) {
            "space" => FollowNumber::Space,
            "nothing" => FollowNumber::Nothing,
            _ => FollowNumber::Tab,
        },
        tab: l.para.tabs.iter().find(|t| t.kind != TabKind::Clear).map(|t| Points(t.pos)),
        number_position: Points(left + first),
        text_indent: Points(left),
        restart_after: l.restart.map(|r| r.clamp(0, 9) as u8),
        legal: l.legal.unwrap_or(false),
        linked_style: l.style.as_deref().map(|s| StyleId(s.to_string())),
        font: match &rpr.ascii {
            Some(crate::docx::props::FontRef::Name(n)) => Some(n.clone()),
            _ => None,
        },
        picture: None,
        bold: label.bold,
        italic: label.italic,
        underline: label.underline,
        size: label.size,
        color: label.color,
        highlight: label.highlight,
    };
    LevelDefinition { level, format, label }
}

/// The number format a level's number style names.
fn number_format(f: &NumberFormat) -> Literal<crate::format::NumberFormat> {
    let v = f.val();
    crate::format::NumberFormat::ALL
        .iter()
        .find(|k| crate::docx::list_xml::format_name(Some(&Literal::Known(**k))) == v)
        .map_or_else(|| Literal::Other(v.into()), |k| Literal::Known(*k))
}

/// A level set to `def`: `old` (the level and its view) edited where they differ, so what the view doesn't show is
/// kept, with what the edit set; with no `old`, a new level.
fn level(old: Option<(&Level, &LevelDefinition)>, def: &LevelDefinition, pic: PicChange) -> (Level, Vec<LevelProp>) {
    let fresh = old.is_none();
    let blank = LevelDefinition { level: ListLevel { start: 1, ..ListLevel::default() }, ..LevelDefinition::default() };
    let was_def = old.map_or(&blank, |o| o.1);
    let mut l = old.map_or_else(Level::default, |o| o.0.clone());
    let (was, new) = (&was_def.level, &def.level);
    let mut set = Vec::new();
    if fresh || was.start != new.start {
        l.start = Some(i64::from(new.start));
        set.push(LevelProp::Start);
    }
    if fresh || was_def.format != def.format {
        (l.format, l.format_code) = (Some(number_format(&def.format)), None);
        set.push(LevelProp::Format);
    }
    if fresh || was.restart_after != new.restart_after {
        l.restart = new.restart_after.map(i64::from);
        set.push(LevelProp::Restart);
    }
    if fresh || was.linked_style != new.linked_style {
        l.style = new.linked_style.as_ref().map(|s| s.0.as_str().into());
        set.push(LevelProp::Style);
    }
    if fresh || was.legal != new.legal {
        l.legal = new.legal.then_some(true);
        set.push(LevelProp::Legal);
    }
    if fresh || was.follow != new.follow {
        l.suffix = match new.follow {
            FollowNumber::Tab => None,
            FollowNumber::Space => Some(Literal::Known(ListSuffix::Space)),
            FollowNumber::Nothing => Some(Literal::Known(ListSuffix::Nothing)),
        };
        set.push(LevelProp::Suffix);
    }
    if fresh || was.text != new.text {
        l.text = Some(new.text.as_str().into());
        set.push(LevelProp::Text);
    }
    if let Some(p) = pic {
        l.pic_bullet = p;
        set.push(LevelProp::PicBullet);
    }
    if fresh || was.alignment != new.alignment {
        l.jc = Some(Literal::Known(match new.alignment {
            LevelAlignment::Left => ListJc::Left,
            LevelAlignment::Center => ListJc::Center,
            LevelAlignment::Right => ListJc::Right,
        }));
        set.push(LevelProp::Jc);
    }
    if fresh || (was.number_position, was.text_indent, was.tab) != (new.number_position, new.text_indent, new.tab) {
        // The indents alone: the start indent, and the first line's hanging or further in
        let twips = super::lists::twips;
        let hanging = new.text_indent.0 - new.number_position.0;
        l.para.indent_left = Some(twips(new.text_indent.0));
        l.para.indent_right = None;
        l.para.indent_first = Some(if hanging >= 0.0 { -twips(hanging) } else { twips(-hanging) });
        for prop in [ParaProp::IndentLeft, ParaProp::IndentRight, ParaProp::IndentFirst] {
            set.push(LevelProp::Para(ParaSet { prop, whole: false }));
        }
        if (fresh || was.tab != new.tab) && level_tab(&mut l.para, new.tab) {
            set.push(LevelProp::Para(ParaSet { prop: ParaProp::Tabs, whole: true }));
        }
    }
    if fresh || was.font != new.font || was_def.label != def.label {
        let mut chars = Vec::new();
        if fresh || was.font != new.font {
            let f = new.font.as_deref().map(|f| FontRef::Family(f.into()));
            l.char.fonts = ScriptFonts { latin: f.clone(), latin_extended: f, east_asian: None, complex: None };
            l.char.hint = new.font.is_some().then_some(FontHint::Default);
            for c in [ScriptClass::Latin, ScriptClass::LatinExtended, ScriptClass::EastAsian, ScriptClass::Complex] {
                chars.push(Set { prop: Prop::Font(c), whole: true });
            }
            chars.push(Set { prop: Prop::Hint, whole: true });
        }
        def.label.apply(&was_def.label, &mut l.char, &mut chars);
        set.extend(chars.into_iter().map(LevelProp::Char));
    }
    (l, set)
}

/// `p`'s tab stops with the level's own (the first that isn't a clear, as [`list_level`] reads it) at `tab`: moved
/// with its kind and leader, added as a list tab, or removed. Its other stops stay. False when there was none to
/// remove.
fn level_tab(p: &mut ParaProps, tab: Option<Points>) -> bool {
    let own = p.tabs.iter().position(|t| t.kind != TabKind::Clear);
    let pos = |t: Points| super::lists::twips(t.0);
    match (own, tab) {
        (Some(i), Some(t)) => p.tabs[i].pos = pos(t),
        (None, Some(t)) => p.tabs.push(TabStop { pos: pos(t), kind: TabKind::Num, leader: Leader::None }),
        (Some(i), None) => {
            p.tabs.remove(i);
        }
        (None, None) => return false,
    }
    true
}

/// A level's picture bullet change: `None` keeps it, `Some(None)` removes it, `Some(Some(id))` sets it.
pub(super) type PicChange = Option<Option<i64>>;

/// A level a list defines in its override: the level, and that level in full.
pub(super) type Override = Option<(Rc<Level>, LevelDefinition)>;

/// The levels an edit sets.
#[derive(Clone, Copy)]
pub(super) enum Levels<'a> {
    /// As the API gives them ([`LevelDefinition::of_level`]).
    Shown(&'a [ListLevel]),
    /// In full.
    Full(&'a [LevelDefinition]),
}

impl Levels<'_> {
    fn len(self) -> usize {
        match self {
            Levels::Shown(l) => l.len(),
            Levels::Full(l) => l.len(),
        }
    }

    /// Level `k` as it is to be, over `was`.
    fn at(self, k: usize) -> Option<LevelDefinition> {
        match self {
            Levels::Shown(l) => l.get(k).map(LevelDefinition::of_level),
            Levels::Full(l) => l.get(k).cloned(),
        }
    }
}

/// Definition `id` with its levels from `levels` (edited from `old`'s where it has them), a new one with none. A
/// level `over` names is the list's: a copy (a new id) takes it as edited, else it is left to [`overridden`].
pub(super) fn definition(
    id: i64,
    old: Option<&ListDefDirect>,
    levels: Levels,
    name: Option<&str>,
    pics: &[PicChange],
    over: &[Override],
    colours: &ThemeColours,
) -> ListDefDirect {
    let new = || ListDef { multi_level_type: Some(Literal::Known(MultiLevelType::HybridMultilevel)), ..ListDef::default() };
    let mut d = old.map_or_else(new, |o| (**o.get()).clone());
    // A copy has the new id and is a new list: its nsid goes, and it isn't a list style's definition.
    let copy = old.is_some_and(|o| o.get().id.0 != id);
    (d.id, d.origin) = (ListDefId(id), Origin::Document);
    if copy {
        (d.nsid, d.template, d.style_link) = (None, None, None);
    }
    match name {
        Some("") => d.name = None,
        Some(n) => d.name = Some(n.into()),
        None => {}
    }
    let mut set = Vec::new();
    // A copy keeps the list's own levels past the ones supplied too: it has no list of its own to hold them.
    for k in 0..if copy { 9 } else { levels.len().min(9) } {
        let over = over.get(k).and_then(Option::as_ref);
        if over.is_some() && !copy {
            continue;
        }
        let own = d.levels[k].clone().filter(|_| over.is_none()).map(|l| {
            let v = level_definition(&l, colours);
            (l, v)
        });
        let base = over.or(own.as_ref()).map(|(l, v)| (&**l, v));
        let (l, s) = match (levels.at(k), over) {
            (Some(new), _) => level(base, &new, pics.get(k).copied().flatten()),
            (None, Some((l, _))) => ((**l).clone(), Vec::new()),
            (None, None) => continue,
        };
        // A level from the list's override is written anew here
        let ilvl = k as u8;
        match over {
            Some(_) => set.push(ListDefProp::Level(ilvl, LevelProp::Whole)),
            None => set.extend(s.into_iter().map(|p| ListDefProp::Level(ilvl, p))),
        }
        d.levels[k] = Some(Rc::new(l));
    }
    match old {
        Some(o) => o.edited(d, &set),
        None => ListDefDirect::typed(Rc::new(d)),
    }
}

/// List `old` with the levels it overrides edited to `levels`; `None` when it overrides none of them.
fn overridden(old: &ListDirect, levels: Levels, pics: &[PicChange], over: &[Override]) -> Option<ListDirect> {
    let mut n = (**old.get()).clone();
    let mut set = Vec::new();
    let mut changed = false;
    for o in &mut n.overrides {
        let Some(Some((l, view))) = usize::try_from(o.level).ok().and_then(|k| over.get(k)) else { continue };
        let Some(new) = levels.at(o.level as usize) else { continue };
        let (l, s) = level(Some((l, view)), &new, pics.get(o.level as usize).copied().flatten());
        o.def = Some(Rc::new(l));
        set.extend(s.into_iter().map(|p| ListProp::Override(o.level, OverrideProp::Level(p))));
        changed = true;
    }
    changed.then(|| old.edited(n, &set))
}

/// A new list `id` numbered with definition `def`, each `(level, value)` of `starts` a level's start.
pub(super) fn new_list(id: i64, def: i64, starts: &[(i64, i64)]) -> ListDirect {
    let overrides = starts.iter().map(|&(level, start)| LevelOverride { level, start: Some(start), def: None, foreign: None }).collect();
    ListDirect::typed(Rc::new(List { id: ListId(id), def: Some(ListDefId(def)), overrides, origin: Origin::Document, foreign: None }))
}

/// The holder of definition `id`: the document's, else (a glossary one) its typed definition.
pub(super) fn def_holder(t: &TypedDefs, id: ListDefId) -> Option<ListDefDirect> {
    t.numbering.defs.get(&id).cloned().or_else(|| Some(ListDefDirect::typed(t.cascade.lists.defs.get(&id)?.clone())))
}

/// The holder of list `id`, as [`def_holder`].
pub(super) fn list_holder(t: &TypedDefs, id: ListId) -> Option<ListDirect> {
    t.numbering.lists.get(&id).cloned().or_else(|| Some(ListDirect::typed(t.cascade.lists.lists.get(&id)?.clone())))
}

/// The holder of style `id`, as [`def_holder`].
pub(super) fn style_holder(t: &TypedDefs, id: &str) -> Option<StyleDef> {
    t.styles.styles.get(id).cloned().or_else(|| Some(StyleDef::typed(t.cascade.styles.get(id)?.clone())))
}

/// Style `s` numbered with `to`'s list at its level (level 0 written as none), or with no list of its own.
pub(super) fn style_numbered(s: &StyleDef, to: Option<(i64, usize)>) -> StyleDef {
    let mut v = (**s.get()).clone();
    let para = &mut v.format.para;
    (para.list, para.list_level) = match to {
        Some((num, k)) => (Some(ListChoice::In(ListId(num))), (k != 0).then_some(k as i64)),
        None => (None, None),
    };
    s.edited(v, &[StyleSet::Para(None, ParaSet { prop: ParaProp::List, whole: true })])
}

impl Editor {
    /// The selection's list: its first list paragraph, `numId` and level.
    pub(crate) fn selection_list(&mut self) -> Option<(ParaId, i64, i64)> {
        self.selected_paras().into_iter().find_map(|para| {
            let ListRef { list: ListId(num), level: ilvl } =
                self.index.para(&self.doc, para)?.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?;
            Some((para, num, ilvl))
        })
    }

    /// The nine levels of `num` as the API shows them, with their picture bullets.
    pub fn list_levels(&self, num: i64) -> Option<Vec<ListLevel>> {
        Some(self.list_level_definitions(num)?.into_iter().map(|d| d.level).collect())
    }

    /// The nine levels of `num` in full, with their picture bullets.
    pub fn list_level_definitions(&self, num: i64) -> Option<Vec<LevelDefinition>> {
        let c = self.doc.definitions.as_ref()?.cascade();
        let levels = c.levels(ListId(num))?;
        Some(
            levels
                .iter()
                .map(|l| {
                    let Some(l) = l else { return LevelDefinition { level: ListLevel { start: 1, ..ListLevel::default() }, ..Default::default() } };
                    let mut v = level_definition(l, &c.colours);
                    v.level.picture = l
                        .pic_bullet
                        .and_then(|id| c.lists.pictures.get(&id))
                        .and_then(|p| p.image)
                        .and_then(|i| self.doc.images.get(i))
                        .map(|img| crate::api::value::Bytes(img.to_vec()));
                    v
                })
                .collect(),
        )
    }

    /// The list of the paragraph the selection's focus is in, in full: what a host shows as the current list and
    /// its levels, without a dialog. `None` outside a list.
    pub fn list_definition(&self) -> Option<ListDefinition> {
        let para = self.selection.focus().and_then(|p| p.para_offset()).map(|(p, _)| p)?;
        let ListRef { list: ListId(num), level: ilvl } =
            self.index.para(&self.doc, para)?.style.num.filter(|ListRef { list: ListId(n), level: _ }| *n != 0)?;
        self.list_definition_of(num, ilvl)
    }

    /// List `num` in full, with `ilvl` its level shown as current.
    pub fn list_definition_of(&self, num: i64, ilvl: i64) -> Option<ListDefinition> {
        let c = self.doc.definitions.as_ref()?.cascade();
        let abs = c.lists.defs.get(&c.abstract_of(ListId(num))?)?;
        let own = c.lists.defs.get(&c.lists.lists.get(&ListId(num))?.def.unwrap_or(crate::format::ListDefId(-1)));
        let levels = self.list_levels(num)?;
        let level = ilvl.clamp(0, 8) as u8;
        let text = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
        Some(ListDefinition {
            id: num,
            name: text(&abs.name).filter(|n| !n.is_empty()),
            style: own.and_then(|a| text(&a.num_style_link)).or_else(|| text(&abs.style_link)).map(StyleId),
            bullet: levels.get(level as usize).is_some_and(|l| l.format == NumberFormat::Bullet),
            level,
            levels,
        })
    }

    /// The editable list paragraphs in the order lists number through them: each paragraph's id and `(numId, ilvl)`.
    pub(super) fn list_paras(&self) -> Vec<(ParaId, i64, i64)> {
        super::lists::numbered_paras(&self.doc, &self.index)
            .into_iter()
            .filter_map(|(p, _)| {
                let ListRef { list: ListId(num), level: ilvl } = p.style.num.filter(|ListRef { list: ListId(id), level: _ }| *id != 0)?;
                let para = ParaId(p.id);
                self.index.para_slot(para).is_some().then_some((para, num, ilvl))
            })
            .collect()
    }

    /// Edits the numbering table in one step, then gives the paragraphs `to` the `numId` in `assign`; every list
    /// paragraph is formatted and labelled again.
    pub(super) fn edit_numbering(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut crate::docx::definitions::NumberingEdits, &Cx, &ThemeColours) -> Option<()>,
        assign: &[(ParaId, i64)],
        fresh: bool,
        fonts: &Fonts,
    ) -> Outcome {
        self.edit_lists(
            label,
            |r, cx| {
                let colours = r.cascade().colours;
                edit(&mut r.numbering_edits(), cx, &colours).map(|_| Vec::new())
            },
            assign,
            fresh,
            fonts,
        )
    }

    /// [`Self::edit_numbering`] with the styles too: `edit` answers the styles it changed, whose paragraphs are
    /// formatted again as well.
    fn edit_lists(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut Definitions, &Cx) -> Option<Vec<String>>,
        assign: &[(ParaId, i64)],
        fresh: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        let list_style = fresh && res.cascade().styles.styles.contains_key("ListParagraph");
        let mut next = (*res).clone();
        let cx = next.typed().numbering.cx();
        let Some(styles) = edit(&mut next, &cx) else { return Outcome::Unchanged("the list didn't change") };
        let mut tx = self.begin(label);
        let added = self.docx.added_lists.clone();
        if self.step(&mut tx, Step::Definitions { definitions: Some(Rc::new(next)), added }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the list didn't change");
        }
        for &(para, num) in assign {
            let ilvl = if fresh {
                0
            } else {
                self.index.para(&self.doc, para).and_then(|p| p.style.num).map_or(0, |ListRef { list: ListId(_), level: l }| l.max(0))
            };
            let ok = self.repara(&mut tx, para, &|e| {
                super::lists::in_list(e, num, ilvl);
                if list_style && !(e.style.is_some() || super::lists::written(e, "pStyle")) {
                    e.style = Some("ListParagraph".into());
                    e.whole(crate::format::ParaProp::Style);
                }
            });
            if !ok {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
        }
        let lists: Vec<ParaId> = self.list_paras().into_iter().map(|(p, _, _)| p).collect();
        tx.stale_paras.extend(lists);
        if !styles.is_empty() {
            let styled = self.styled_paras(&styles);
            tx.stale_paras.extend(styled);
        }
        tx.relabel = true;
        self.commit(tx, None, fonts)
    }

    /// The paragraphs of `num` from `from` on, in document order.
    fn list_from(&self, from: ParaId, num: i64) -> Vec<ParaId> {
        let all = self.list_paras();
        let start = all.iter().position(|(p, _, _)| *p == from).unwrap_or(all.len());
        all[start..].iter().filter(|(_, n, _)| *n == num).map(|(p, _, _)| *p).collect()
    }

    /// The picture bullet changes of `levels` against the list `num`'s (none: a new list), and the bullets to add:
    /// their ids and images (new images queued with the transaction).
    fn picture_changes(&mut self, levels: &[ListLevel], num: Option<i64>) -> (Vec<PicChange>, Vec<(i64, crate::docx::ResourceId)>) {
        let now = num.and_then(|n| self.list_levels(n)).unwrap_or_default();
        let mut next_id = self.doc.definitions.as_ref().and_then(|r| r.cascade().lists.pictures.keys().max().copied()).map_or(0, |m| m + 1);
        let mut add = Vec::new();
        let changes = levels
            .iter()
            .enumerate()
            .map(|(k, l)| {
                let was = now.get(k).and_then(|x| x.picture.as_ref());
                match (&l.picture, was) {
                    (Some(a), Some(b)) if a.0 == b.0 => None,
                    (None, None) => None,
                    (None, Some(_)) => Some(None),
                    (Some(bytes), _) => {
                        let image = match self.doc.images.find(&bytes.0) {
                            Some(i) => i,
                            None => self.pending_image(bytes.0.clone()),
                        };
                        let id = next_id;
                        next_id += 1;
                        add.push((id, image));
                        Some(Some(id))
                    }
                }
            })
            .collect();
        (changes, add)
    }

    pub(super) fn set_list_definition(&mut self, d: ListDefinitionChange, fonts: &Fonts) -> Outcome {
        let levels = d.levels.iter().map(LevelDefinition::of_level).collect();
        self.set_list_levels(ListLevelsChange { levels, apply_to: d.apply_to, name: d.name }, fonts)
    }

    /// Defines the selection's list (or a new one) with levels in full: [`Command::SetListDefinition`] with every
    /// number style and the numbers' own formatting. Refused for a number style that can't be written or a size out
    /// of bounds.
    ///
    /// [`Command::SetListDefinition`]: crate::api::command::Command::SetListDefinition
    pub fn set_list_levels(&mut self, change: ListLevelsChange, fonts: &Fonts) -> Outcome {
        // What the edit sets is checked: a number style or size the list was read with stays as it is.
        let was = self.selection_list().and_then(|(_, n, _)| self.list_level_definitions(n)).unwrap_or_default();
        if let Some(e) = change.levels.iter().enumerate().find_map(|(k, l)| l.check_changed(was.get(k)).err()) {
            return Outcome::Refused(e);
        }
        if let Some(out) = self.refuse_like(&crate::api::command::Command::SetListDefinition(Default::default())) {
            return out;
        }
        let Some(res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        let mut defs = change.levels;
        defs.truncate(9);
        let mut d = ListDefinitionChange { levels: defs.iter().map(|l| l.level.clone()).collect(), apply_to: change.apply_to, name: change.name };
        let paras = self.selected_paras();
        let here = self.selection_list();
        let current = here.map(|(_, n, _)| n);
        let now = current.and_then(|n| self.list_levels(n)).unwrap_or_default();
        let whole = d.apply_to == ListApplyTo::WholeList;
        // Linked styles
        for (k, l) in d.levels.iter_mut().enumerate() {
            let was = now.get(k).and_then(|x| x.linked_style.as_ref());
            if l.linked_style.as_ref() == was {
                if !whole {
                    l.linked_style = None;
                }
                continue;
            }
            l.linked_style = l.linked_style.take().and_then(|s| linkable(&res.cascade().styles, &s.0)).map(StyleId);
        }
        let mut taken: Vec<StyleId> = Vec::new();
        for l in &mut d.levels {
            if l.linked_style.as_ref().is_some_and(|s| taken.contains(s)) {
                l.linked_style = None;
            } else if let Some(s) = &l.linked_style {
                taken.push(s.clone());
            }
        }
        let (pics, new_pics) = self.picture_changes(&d.levels, current);
        for (def, l) in defs.iter_mut().zip(&d.levels) {
            def.level = l.clone();
        }
        let Some((first, num, _)) = here else {
            // Outside a list: a new one, for the selected paragraphs.
            let (abstract_id, num_id) = res.cascade().lists.free_ids();
            let (abstract_id, num_id) = (abstract_id.0, num_id.0);
            let assign: Vec<(ParaId, i64)> = paras.iter().map(|&p| (p, num_id)).collect();
            let levels = complete_definitions(&defs);
            let links = links(&complete(&d.levels), &[], num_id);
            return self.edit_lists(
                "Define List",
                |r, cx| {
                    add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                    let colours = r.colours();
                    r.set_list_def(definition(abstract_id, None, Levels::Full(&levels), d.name.as_deref(), &pics, &[], &colours));
                    r.set_list(new_list(num_id, abstract_id, &[]));
                    link_styles(r, &links, abstract_id)
                },
                &assign,
                true,
                fonts,
            );
        };
        let typed = res.typed();
        let c = &*typed.cascade;
        let Some(abstract_id) = c.abstract_of(ListId(num)) else { return Outcome::Unchanged("no list") };
        let old = def_holder(&typed, abstract_id);
        let abstract_id = abstract_id.0;
        let over: Vec<Override> = (0..9)
            .map(|k| {
                let l = c.lists.lists.get(&ListId(num))?.override_of(k)?.def.clone()?;
                let v = level_definition(&l, &c.colours);
                Some((l, v))
            })
            .collect();
        if whole {
            let links = links(&d.levels, &now, num);
            let list = list_holder(&typed, ListId(num));
            return self.edit_lists(
                "Define List",
                |r, cx| {
                    add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                    let colours = r.colours();
                    r.set_list_def(definition(abstract_id, old.as_ref(), Levels::Full(&defs), d.name.as_deref(), &pics, &over, &colours));
                    if let Some(m) = list.as_ref().and_then(|l| overridden(l, Levels::Full(&defs), &pics, &over)) {
                        r.set_list(m);
                    }
                    link_styles(r, &links, abstract_id)
                },
                &[],
                false,
                fonts,
            );
        }
        let (new_abstract, new_num) = c.lists.free_ids();
        let (new_abstract, new_num) = (new_abstract.0, new_num.0);
        let assign: Vec<(ParaId, i64)> = match d.apply_to {
            ListApplyTo::ThisPointForward => self.list_from(first, num).into_iter().map(|p| (p, new_num)).collect(),
            _ => paras
                .iter()
                .filter(|&&p| {
                    self.index.para(&self.doc, p).is_some_and(|q| q.style.num.is_some_and(|ListRef { list: ListId(n), level: _ }| n == num))
                })
                .map(|&p| (p, new_num))
                .collect(),
        };
        let links = links(&d.levels, &[], new_num);
        // The list's start values go with its first paragraph
        let head = self.list_paras().into_iter().find(|(_, n, _)| *n == num).map(|(p, _, _)| p);
        let mut starts: Vec<(i64, i64)> = match c.lists.lists.get(&ListId(num)) {
            Some(m) if head.is_some_and(|h| assign.iter().any(|(p, _)| *p == h)) => {
                m.overrides.iter().filter(|o| (0..9).contains(&o.level)).filter_map(|o| Some((o.level, o.start?))).collect()
            }
            _ => Vec::new(),
        };
        starts.sort_unstable();
        self.edit_lists(
            "Define List",
            |r, cx| {
                add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                let colours = r.colours();
                r.set_list_def(definition(new_abstract, old.as_ref(), Levels::Full(&defs), d.name.as_deref(), &pics, &over, &colours));
                r.set_list(new_list(new_num, new_abstract, &starts));
                link_styles(r, &links, new_abstract)
            },
            &assign,
            false,
            fonts,
        )
    }

    pub(super) fn set_numbering_value(&mut self, v: NumberingValue, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        let Some((first, num, ilvl)) = self.selection_list() else { return Outcome::unavailable("the selection isn't in a list") };
        let c = res.cascade();
        let Some(abstract_id) = c.abstract_of(ListId(num)).map(|a| a.0) else { return Outcome::Unchanged("no list") };
        let start = c.level(ListId(num), ilvl).map_or(1, |l| l.start.unwrap_or(0).max(0) as u32);
        let following = self.list_from(first, num);
        let new_num = c.lists.free_ids().1.0;
        match v.mode {
            NumberingMode::StartNew => {
                let value = if v.value == 0 { start } else { v.value };
                let assign: Vec<(ParaId, i64)> = following.iter().map(|&p| (p, new_num)).collect();
                self.edit_numbering(
                    "Set Numbering Value",
                    |n, _, _| {
                        n.set_list(new_list(new_num, abstract_id, &[(ilvl, i64::from(value))]));
                        Some(())
                    },
                    &assign,
                    false,
                    fonts,
                )
            }
            NumberingMode::Continue => {
                // The previous list of the same kind (bullets or numbers).
                let bullet =
                    |n: i64, l: i64| c.level(ListId(n), l).is_some_and(|x| crate::docx::list_xml::format_name(x.format.as_ref()) == "bullet");
                let kind = bullet(num, ilvl);
                let all = self.list_paras();
                let at = all.iter().position(|(p, _, _)| *p == first).unwrap_or(0);
                let Some(&(_, prev, _)) = all[..at].iter().rev().find(|(_, n, l)| *n != num && bullet(*n, *l) == kind) else {
                    return Outcome::unavailable("no previous list to continue");
                };
                if !v.advance {
                    let assign: Vec<(ParaId, i64)> = following.iter().map(|&p| (p, prev)).collect();
                    return self.edit_numbering("Continue Numbering", |_, _, _| Some(()), &assign, false, fonts);
                }
                // Advancing: a new list over the previous one's definition, starting at the value.
                let Some(prev_abstract) = c.abstract_of(ListId(prev)).map(|a| a.0) else { return Outcome::Unchanged("no list") };
                let assign: Vec<(ParaId, i64)> = following.iter().map(|&p| (p, new_num)).collect();
                let value = v.value.max(1);
                self.edit_numbering(
                    "Set Numbering Value",
                    |n, _, _| {
                        n.set_list(new_list(new_num, prev_abstract, &[(ilvl, i64::from(value))]));
                        Some(())
                    },
                    &assign,
                    false,
                    fonts,
                )
            }
        }
    }

    pub(super) fn adjust_list_indents(&mut self, i: ListIndents, fonts: &Fonts) -> Outcome {
        let Some((_, num, ilvl)) = self.selection_list() else { return Outcome::unavailable("the selection isn't in a list") };
        let Some(mut levels) = self.list_levels(num) else { return Outcome::Unchanged("no list") };
        let k = ilvl.clamp(0, 8) as usize;
        (levels[k].number_position, levels[k].text_indent, levels[k].follow, levels[k].tab) = (i.number_position, i.text_indent, i.follow, i.tab);
        // Only this level changes: the levels after it are left out.
        levels.truncate(k + 1);
        self.set_list_definition(ListDefinitionChange { levels, apply_to: ListApplyTo::WholeList, name: None }, fonts)
    }
}

/// Adds picture bullets: each a picture bullet of its image, which the writer gives its part.
fn add_pictures(n: &mut crate::docx::definitions::NumberingEdits, pics: &[(i64, crate::docx::ResourceId)], cx: &Cx) {
    for &(id, image) in pics {
        let rel = format!("{}{}", crate::docx::write::NEW_IMAGE, image.0);
        n.add_picture(id, crate::docx::list_xml::new_picture_xml(id, &rel, cx), rel, image);
    }
}

/// Nine levels in full: `levels`, then the default ones after them.
fn complete_definitions(levels: &[LevelDefinition]) -> Vec<LevelDefinition> {
    let shown: Vec<ListLevel> = levels.iter().map(|l| l.level.clone()).collect();
    complete(&shown).into_iter().enumerate().map(|(k, l)| levels.get(k).cloned().unwrap_or_else(|| LevelDefinition::of_level(&l))).collect()
}

pub use crate::api::styles::ListDefinition;

/// [`Editor::set_list_levels`]' options: [`ListDefinitionChange`] with its levels in full.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListLevelsChange {
    /// The levels, from the first: level `k` defines level `k`; levels past its end are kept.
    pub levels: Vec<LevelDefinition>,
    /// What they apply to.
    pub apply_to: ListApplyTo,
    /// The list's name; none keeps it, empty removes it.
    pub name: Option<String>,
}

/// Nine levels: `levels`, then the default ones after them.
pub fn complete(levels: &[ListLevel]) -> Vec<ListLevel> {
    (0..9)
        .map(|k| {
            levels.get(k).cloned().unwrap_or_else(|| ListLevel {
                format: [NumberFormat::Decimal, NumberFormat::LowerLetter, NumberFormat::LowerRoman][k % 3].clone(),
                text: format!("%{}.", k + 1),
                start: 1,
                alignment: if k % 3 == 2 { LevelAlignment::Right } else { LevelAlignment::Left },
                number_position: Points(36.0 * (k + 1) as f64 - if k % 3 == 2 { 9.0 } else { 18.0 }),
                text_indent: Points(36.0 * (k + 1) as f64),
                ..ListLevel::default()
            })
        })
        .collect()
}

/// A paragraph style a level links to: the style, and the list and level it is numbered with (none: unlinked).
type Link = (String, Option<(i64, usize)>);

/// The style links `levels` make or end against `was` (the list's levels before), for the list `num`: the ends
/// first.
fn links(levels: &[ListLevel], was: &[ListLevel], num: i64) -> Vec<Link> {
    let mut ends = Vec::new();
    let mut made = Vec::new();
    for (k, l) in levels.iter().enumerate().take(9) {
        let before = was.get(k).and_then(|w| w.linked_style.as_ref());
        if l.linked_style.as_ref() == before {
            continue;
        }
        ends.extend(before.map(|s| (s.0.clone(), None)));
        made.extend(l.linked_style.as_ref().map(|s| (s.0.clone(), Some((num, k)))));
    }
    ends.extend(made);
    ends
}

/// The document's paragraph style for a level's link: `id`, or the built-in style it names under its name.
pub(super) fn linkable(styles: &crate::format::StyleSheet, id: &str) -> Option<String> {
    let paragraph = |s: &&crate::format::Style| s.kind == crate::format::StyleKind::Paragraph;
    if let Some(s) = styles.get(id).map(|s| &**s).filter(paragraph) {
        return Some(s.id.to_string());
    }
    let name = crate::services::styles::builtin::built_in(id)?.name;
    styles
        .styles
        .values()
        .map(|e| &*e.style)
        .filter(paragraph)
        .find(|s| s.name.as_deref().unwrap_or_default().eq_ignore_ascii_case(name))
        .map(|s| s.id.to_string())
}

/// Makes and ends the style links of the list `abstract_id`: a linked style is numbered with its level, and no other
/// level of any list links to it; an unlinked one numbered with the list no longer is. Answers the styles changed.
fn link_styles(r: &mut Definitions, links: &[Link], abstract_id: i64) -> Option<Vec<String>> {
    let mut changed = Vec::new();
    for (id, to) in links {
        let typed = r.typed();
        let c = &*typed.cascade;
        let (Some(e), Some(style)) = (c.styles.styles.get(id.as_str()), style_holder(&typed, id)) else { continue };
        let own = crate::docx::para_props::num_id(c.format_of(e).para.list);
        if to.is_none() && own.and_then(|n| c.abstract_of(ListId(n))).map(|a| a.0) != Some(abstract_id) {
            continue;
        }
        changed.extend(r.set_style(style_numbered(&style, *to)).ok()?);
        let Some((num, k)) = to else { continue };
        let typed = r.typed();
        let lists = &typed.cascade.lists;
        let links_to = |l: &Option<Rc<Level>>| l.as_ref().and_then(|l| l.style.as_deref()) == Some(id.as_str());
        let others: Vec<ListDefDirect> = lists
            .defs
            .iter()
            .filter(|(a, abs)| abs.levels.iter().enumerate().any(|(j, l)| (a.0, j) != (abstract_id, *k) && links_to(l)))
            .filter_map(|(a, _)| Some(unlinked(&def_holder(&typed, *a)?, id, (a.0 == abstract_id).then_some(*k))))
            .collect();
        for d in others {
            r.set_list_def(d);
        }
        // The instances' own levels
        let nums: Vec<ListDirect> = lists
            .lists
            .iter()
            .filter(|(n, m)| m.overrides.iter().any(|o| (n.0, o.level) != (*num, *k as i64) && links_to(&o.def)))
            .filter_map(|(n, _)| Some(unlinked_list(&list_holder(&typed, *n)?, id, (n.0 == *num).then_some(*k as i64))))
            .collect();
        for n in nums {
            r.set_list(n);
        }
    }
    changed.sort();
    changed.dedup();
    Some(changed)
}

/// Definition `d` whose levels (but `keep`) no longer link to the style `id`.
fn unlinked(d: &ListDefDirect, id: &str, keep: Option<usize>) -> ListDefDirect {
    let mut v = (**d.get()).clone();
    let mut set = Vec::new();
    for (j, l) in v.levels.iter_mut().enumerate().filter(|(j, _)| keep != Some(*j)) {
        if let Some(l) = l.as_mut().filter(|l| l.style.as_deref() == Some(id)) {
            Rc::make_mut(l).style = None;
            set.push(ListDefProp::Level(j as u8, LevelProp::Style));
        }
    }
    d.edited(v, &set)
}

/// List `n` whose overriding levels (but `keep`) no longer link to the style `id`.
fn unlinked_list(n: &ListDirect, id: &str, keep: Option<i64>) -> ListDirect {
    let mut v = (**n.get()).clone();
    let mut set = Vec::new();
    for o in v.overrides.iter_mut().filter(|o| keep != Some(o.level)) {
        if let Some(l) = o.def.as_mut().filter(|l| l.style.as_deref() == Some(id)) {
            Rc::make_mut(l).style = None;
            set.push(ListProp::Override(o.level, OverrideProp::Level(LevelProp::Style)));
        }
    }
    n.edited(v, &set)
}
