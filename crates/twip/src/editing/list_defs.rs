//! List definitions: the levels of the selection's list as the Define dialogs show them, and the edits that define
//! them, set numbering values, and adjust indents. Each edits the numbering part's XML and reads it again.

use std::rc::Rc;

use crate::api::command::{
    FollowNumber, LevelAlignment, ListApplyTo, ListDefinitionChange, ListIndents, ListLevel, NumberFormat, NumberingMode, NumberingValue,
};
use crate::api::value::{AutoOr, Color, HighlightColor, NoneOr, Points, StyleId, UnderlineStyle};
use crate::docx::numbering::Level;
use crate::docx::props::TabKind;
use crate::docx::write::props::{Cx, Props, esc};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::step::Step;
use crate::document::paragraph::{ListId, ListRef};

/// CT_Lvl's children in schema order.
const LVL: &[&str] = &["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff", "lvlText", "lvlPicBulletId", "legacy", "lvlJc", "pPr", "rPr"];
/// CT_AbstractNum's children in schema order.
const ABSTRACT: &[&str] = &["nsid", "multiLevelType", "tmpl", "name", "styleLink", "numStyleLink", "lvl"];

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

    /// A level's `w:rPr` with the formatting that differs from `was` written or removed.
    fn write(&self, was: &LabelFormat, rpr: &mut Props, w: &str) {
        let flag = |rpr: &mut Props, on: Option<bool>, names: [&str; 2]| {
            for n in names {
                match on {
                    Some(true) => rpr.set(n, format!("<{w}{n}/>")),
                    Some(false) => rpr.set(n, format!("<{w}{n} {w}val=\"0\"/>")),
                    None => rpr.remove(n),
                }
            }
        };
        if self.bold != was.bold {
            flag(rpr, self.bold, ["b", "bCs"]);
        }
        if self.italic != was.italic {
            flag(rpr, self.italic, ["i", "iCs"]);
        }
        if self.underline != was.underline {
            match self.underline {
                Some(u) => rpr.set("u", format!("<{w}u {w}val=\"{}\"/>", crate::docx::line_style::underline_val(super::commands::underline(u)))),
                None => rpr.remove("u"),
            }
        }
        if self.size != was.size {
            for n in ["sz", "szCs"] {
                match self.size {
                    Some(s) => rpr.set(n, format!("<{w}{n} {w}val=\"{}\"/>", (s.0 * 2.0).round() as i64)),
                    None => rpr.remove(n),
                }
            }
        }
        if self.color != was.color {
            match self.color {
                Some(AutoOr::Auto) => rpr.set("color", format!("<{w}color {w}val=\"auto\"/>")),
                Some(AutoOr::Value(c)) => rpr.set("color", format!("<{w}color {w}val=\"{:06X}\"/>", c.0 & 0xFF_FFFF)),
                None => rpr.remove("color"),
            }
        }
        if self.highlight != was.highlight {
            match self.highlight {
                Some(NoneOr::Value(h)) => rpr.set("highlight", format!("<{w}highlight {w}val=\"{}\"/>", super::commands::highlight_value(h))),
                Some(NoneOr::None) => rpr.set("highlight", format!("<{w}highlight {w}val=\"none\"/>")),
                None => rpr.remove("highlight"),
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

/// A level as the API shows it.
pub fn list_level(l: &Level) -> ListLevel {
    level_definition(l).level
}

/// A level in full.
pub fn level_definition(l: &Level) -> LevelDefinition {
    let left = l.ppr.ind_left.unwrap_or(0.0);
    let first = l.ppr.ind_first.unwrap_or(0.0);
    let format = NumberFormat::of_val(&l.fmt);
    let label = LabelFormat::of(&l.rpr);
    let level = ListLevel {
        format: format.clone(),
        text: l.text.clone(),
        start: l.start.max(0) as u32,
        alignment: match l.jc.as_str() {
            "center" => LevelAlignment::Center,
            "right" | "end" => LevelAlignment::Right,
            _ => LevelAlignment::Left,
        },
        follow: match l.suff.as_str() {
            "space" => FollowNumber::Space,
            "nothing" => FollowNumber::Nothing,
            _ => FollowNumber::Tab,
        },
        tab: l.ppr.tabs.iter().find(|t| t.kind != TabKind::Clear).map(|t| Points(t.pos)),
        number_position: Points(left + first),
        text_indent: Points(left),
        restart_after: l.restart.map(|r| r.clamp(0, 9) as u8),
        legal: l.is_lgl,
        linked_style: l.pstyle.clone().map(StyleId),
        font: match &l.rpr.ascii {
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

/// A `w:lvl` for level `k` set to `def`: `old` (its XML and its view) edited where they differ, so what the view
/// doesn't show is kept.
fn level_xml(k: usize, old: Option<(&str, &LevelDefinition)>, def: &LevelDefinition, pic: Option<Option<i64>>, cx: &Cx) -> String {
    let w = &cx.w;
    let mut p = Props::parse(old.map(|o| o.0), "lvl", cx);
    p.reorder(LVL);
    if old.is_none() {
        p = Props::parse(Some(&format!("<{w}lvl {w}ilvl=\"{k}\"><{w}start {w}val=\"1\"/></{w}lvl>")), "lvl", cx);
        p.reorder(LVL);
    }
    let blank = LevelDefinition { level: ListLevel { start: 1, ..ListLevel::default() }, ..LevelDefinition::default() };
    let was_def = old.map_or(&blank, |o| o.1);
    let (was, new) = (&was_def.level, &def.level);
    let val = |name: &str, v: &str| format!("<{w}{name} {w}val=\"{}\"/>", esc(v));
    if old.is_none() || was.start != new.start {
        p.set("start", val("start", &new.start.to_string()));
    }
    if old.is_none() || was_def.format != def.format {
        p.set("numFmt", val("numFmt", def.format.val()));
    }
    if old.is_none() || was.restart_after != new.restart_after {
        match new.restart_after {
            Some(r) => p.set("lvlRestart", val("lvlRestart", &r.to_string())),
            None => p.remove("lvlRestart"),
        }
    }
    if old.is_none() || was.linked_style != new.linked_style {
        match &new.linked_style {
            Some(s) => p.set("pStyle", val("pStyle", &s.0)),
            None => p.remove("pStyle"),
        }
    }
    if old.is_none() || was.legal != new.legal {
        if new.legal { p.set("isLgl", format!("<{w}isLgl/>")) } else { p.remove("isLgl") }
    }
    if old.is_none() || was.follow != new.follow {
        match new.follow {
            FollowNumber::Tab => p.remove("suff"),
            FollowNumber::Space => p.set("suff", val("suff", "space")),
            FollowNumber::Nothing => p.set("suff", val("suff", "nothing")),
        }
    }
    if old.is_none() || was.text != new.text {
        p.set("lvlText", val("lvlText", &new.text));
    }
    match pic {
        Some(Some(id)) => p.set("lvlPicBulletId", val("lvlPicBulletId", &id.to_string())),
        Some(None) => p.remove("lvlPicBulletId"),
        None => {}
    }
    if old.is_none() || was.alignment != new.alignment {
        let jc = match new.alignment {
            LevelAlignment::Left => "left",
            LevelAlignment::Center => "center",
            LevelAlignment::Right => "right",
        };
        p.set("lvlJc", val("lvlJc", jc));
    }
    let twips = |v: Points| ((v.0 * 20.0).round() as i64).to_string();
    if old.is_none() || (was.number_position, was.text_indent, was.tab) != (new.number_position, new.text_indent, new.tab) {
        let mut ppr = Props::parse(p.get("pPr"), "pPr", cx);
        let hanging = new.text_indent.0 - new.number_position.0;
        let ind = if hanging >= 0.0 {
            format!("<{w}ind {w}left=\"{}\" {w}hanging=\"{}\"/>", twips(new.text_indent), twips(Points(hanging)))
        } else {
            format!("<{w}ind {w}left=\"{}\" {w}firstLine=\"{}\"/>", twips(new.text_indent), twips(Points(-hanging)))
        };
        ppr.set("ind", ind);
        if old.is_none() || was.tab != new.tab {
            level_tab(&mut ppr, new.tab, cx);
        }
        if let Some(x) = ppr.xml() {
            p.set("pPr", x);
        }
    }
    if old.is_none() || was.font != new.font || was_def.label != def.label {
        let mut rpr = Props::parse(p.get("rPr"), "rPr", cx);
        rpr.reorder(crate::docx::write::props::RPR);
        if old.is_none() || was.font != new.font {
            match &new.font {
                Some(f) => {
                    let f = esc(f);
                    rpr.set("rFonts", format!("<{w}rFonts {w}ascii=\"{f}\" {w}hAnsi=\"{f}\" {w}hint=\"default\"/>"));
                }
                None => rpr.remove("rFonts"),
            }
        }
        def.label.write(&was_def.label, &mut rpr, w);
        match rpr.xml() {
            Some(x) => p.set("rPr", x),
            None => p.remove("rPr"),
        }
    }
    p.xml().unwrap_or_default()
}

/// `ppr`'s tab stops with the level's own (the first that isn't a clear, as [`list_level`] reads it) at `tab`: moved
/// with its kind and leader, added as a list tab, or removed. Its other stops stay.
fn level_tab(ppr: &mut Props, tab: Option<Points>, cx: &Cx) {
    use crate::docx::write::props::attr_of;
    let w = &cx.w;
    let mut tabs = Props::parse(ppr.get("tabs"), "tabs", cx);
    let own = tabs.kids.iter().position(|(n, x)| n == "tab" && attr_of(x, "pos").is_some() && attr_of(x, "val").as_deref() != Some("clear"));
    let pos = |t: Points| (t.0 * 20.0).round() as i64;
    match (own, tab) {
        (Some(i), Some(t)) => {
            let x = &tabs.kids[i].1;
            let keep = |a: &str| attr_of(x, a).map(|v| format!(" {w}{a}=\"{}\"", esc(&v))).unwrap_or_default();
            tabs.kids[i].1 = format!("<{w}tab{}{} {w}pos=\"{}\"/>", keep("val"), keep("leader"), pos(t));
        }
        (None, Some(t)) => tabs.add("tab", format!("<{w}tab {w}val=\"num\" {w}pos=\"{}\"/>", pos(t))),
        (Some(i), None) => {
            tabs.kids.remove(i);
        }
        (None, None) => return,
    }
    match tabs.xml() {
        Some(x) => ppr.set("tabs", x),
        None => ppr.remove("tabs"),
    }
}

/// A level's picture bullet change: `None` keeps it, `Some(None)` removes it, `Some(Some(id))` sets it.
pub(super) type PicChange = Option<Option<i64>>;

/// A level a `w:num` defines in its `w:lvlOverride`: its `w:lvl` and that level in full.
pub(super) type Override = Option<(String, LevelDefinition)>;

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

/// A `w:abstractNum` with its levels from `levels` (edited from `old`'s where it has them). A level `over` names is
/// the `w:num`'s: a copy (a new id) takes it as edited, else it is left to [`num_xml`].
#[allow(clippy::too_many_arguments, reason = "the part's prefix and the theme its colours are read with")]
pub(super) fn abstract_xml(
    id: i64,
    old: Option<&str>,
    levels: Levels,
    name: Option<&str>,
    pics: &[PicChange],
    over: &[Override],
    cx: &Cx,
    colours: &crate::docx::props::ThemeColours,
) -> String {
    let w = &cx.w;
    let blank = format!("<{w}abstractNum {w}abstractNumId=\"{id}\"><{w}multiLevelType {w}val=\"hybridMultilevel\"/></{w}abstractNum>");
    let was_id = old.and_then(|o| attr_of(o, "abstractNumId"));
    // A copy has the new id and is a new list: its nsid goes, and it isn't a list style's definition.
    let renamed = match (old, &was_id) {
        (Some(o), Some(was)) => o.replacen(&format!("abstractNumId=\"{was}\""), &format!("abstractNumId=\"{id}\""), 1),
        _ => blank,
    };
    let mut a = Props::parse(Some(&renamed), "abstractNum", cx);
    a.reorder(ABSTRACT);
    let copy = was_id.is_some_and(|was| was != id.to_string());
    if copy {
        a.remove("nsid");
        a.remove("tmpl");
        a.remove("styleLink");
    }
    match name {
        Some("") => a.remove("name"),
        Some(n) => a.set("name", format!("<{w}name {w}val=\"{}\"/>", esc(n))),
        None => {}
    }
    // Read as the document's levels are, so a colour from its theme compares as unchanged.
    let parsed = old.and_then(|o| cx.parse(o, |text, el| crate::docx::numbering::parse_abstract(el, text, colours))).flatten();
    // A copy keeps the `w:num`'s own levels past the ones supplied too: it has no `w:num` of that list to hold them.
    for k in 0..if copy { 9 } else { levels.len().min(9) } {
        let over = over.get(k).and_then(Option::as_ref);
        if over.is_some() && !copy {
            continue;
        }
        let raw = a.kids.iter().position(|(n, x)| n == "lvl" && attr_of(x, "ilvl").as_deref() == Some(&k.to_string()));
        let old_level = parsed.as_ref().and_then(|(_, abs)| abs.levels.get(k).cloned().flatten()).map(|l| level_definition(&l));
        let base = match over {
            Some((x, v)) => Some((x.as_str(), v)),
            None => raw.map(|i| a.kids[i].1.as_str()).zip(old_level.as_ref()),
        };
        let xml = match (levels.at(k), over) {
            (Some(new), _) => level_xml(k, base, &new, pics.get(k).copied().flatten(), cx),
            (None, Some((x, _))) => x.clone(),
            (None, None) => continue,
        };
        match raw {
            Some(i) => a.kids[i].1 = xml,
            None => a.add("lvl", xml),
        }
    }
    a.xml().unwrap_or_default()
}

/// The levels a `w:num` defines in its `w:lvlOverride`s, by level.
fn override_levels(raw: &str, cx: &Cx) -> [Option<String>; 9] {
    let mut out: [Option<String>; 9] = Default::default();
    for (n, x) in &Props::parse(Some(raw), "num", cx).kids {
        let k = attr_of(x, "ilvl").and_then(|v| v.parse::<usize>().ok()).filter(|k| *k < 9);
        if let (true, Some(k)) = (n == "lvlOverride", k) {
            out[k] = Props::parse(Some(x), "lvlOverride", cx).get("lvl").map(str::to_string);
        }
    }
    out
}

/// A `w:num` with the levels it overrides edited to `levels`; `None` when it overrides none of them.
fn num_xml(raw: &str, levels: Levels, pics: &[PicChange], over: &[Override], cx: &Cx) -> Option<String> {
    let mut m = Props::parse(Some(raw), "num", cx);
    let mut changed = false;
    for (_, x) in m.kids.iter_mut().filter(|(n, _)| n == "lvlOverride") {
        let Some(k) = attr_of(x, "ilvl").and_then(|v| v.parse::<usize>().ok()) else { continue };
        let Some(Some((lvl, view))) = over.get(k) else { continue };
        let Some(new) = levels.at(k) else { continue };
        let mut o = Props::parse(Some(x), "lvlOverride", cx);
        o.set("lvl", level_xml(k, Some((lvl, view)), &new, pics.get(k).copied().flatten(), cx));
        *x = o.xml()?;
        changed = true;
    }
    changed.then(|| m.xml()).flatten()
}

/// An attribute's value in a start tag or element, by its local name.
fn attr_of(xml: &str, local: &str) -> Option<String> {
    let at =
        xml.find(&format!(":{local}=\"")).map(|i| i + local.len() + 3).or_else(|| xml.find(&format!(" {local}=\"")).map(|i| i + local.len() + 3))?;
    Some(xml[at..at + xml[at..].find('"')?].to_string())
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
        let res = self.doc.resolver.as_ref()?;
        let levels = res.numbering.levels(num, &res.styles)?;
        Some(
            levels
                .iter()
                .map(|l| {
                    let Some(l) = l else { return LevelDefinition { level: ListLevel { start: 1, ..ListLevel::default() }, ..Default::default() } };
                    let mut v = level_definition(l);
                    v.level.picture = l
                        .pic_bullet
                        .and_then(|id| res.numbering.pic_bullets.get(&id))
                        .and_then(|p| p.image)
                        .and_then(|i| self.doc.images.get(i))
                        .map(|img| crate::api::value::Bytes(img.clone()));
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
        let res = self.doc.resolver.as_ref()?;
        let abs = res.numbering.abstracts.get(&res.numbering.abstract_of(num, &res.styles)?)?;
        let own = res.numbering.abstracts.get(&res.numbering.nums.get(&num)?.abstract_id);
        let levels = self.list_levels(num)?;
        let level = ilvl.clamp(0, 8) as u8;
        Some(ListDefinition {
            id: num,
            name: abs.name.clone().filter(|n| !n.is_empty()),
            style: own.and_then(|a| a.num_style_link.clone()).or_else(|| abs.style_link.clone()).map(StyleId),
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
        edit: impl FnOnce(&mut crate::docx::resolvers::NumberingEdits, &Cx, &crate::docx::props::ThemeColours) -> Option<()>,
        assign: &[(ParaId, i64)],
        fresh: bool,
        fonts: &Fonts,
    ) -> Outcome {
        self.edit_lists(
            label,
            |r, cx| {
                let colours = r.styles.colours;
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
        edit: impl FnOnce(&mut crate::docx::resolvers::Resolvers, &Cx) -> Option<Vec<String>>,
        assign: &[(ParaId, i64)],
        fresh: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let list_style = fresh && res.styles.by_id.contains_key("ListParagraph");
        let mut next = (*res).clone();
        let cx = next.numbering.cx();
        let Some(styles) = edit(&mut next, &cx) else { return Outcome::Unchanged("the list didn't change") };
        next.compute_style_links();
        let mut tx = self.begin(label);
        let added = self.doc.added_lists.clone();
        if self.step(&mut tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_err() {
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
    fn picture_changes(&mut self, levels: &[ListLevel], num: Option<i64>) -> (Vec<PicChange>, Vec<(i64, usize)>) {
        let now = num.and_then(|n| self.list_levels(n)).unwrap_or_default();
        let mut next_id = self.doc.resolver.as_ref().and_then(|r| r.numbering.pic_bullets.keys().max().copied()).map_or(0, |m| m + 1);
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
                        let image = match self.doc.images.iter().position(|i| *i == bytes.0) {
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
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
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
            l.linked_style = l.linked_style.take().and_then(|s| linkable(&res.styles, &s.0)).map(StyleId);
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
            let (abstract_id, num_id) = res.numbering.free_ids();
            let assign: Vec<(ParaId, i64)> = paras.iter().map(|&p| (p, num_id)).collect();
            let levels = complete_definitions(&defs);
            let links = links(&complete(&d.levels), &[], num_id);
            return self.edit_lists(
                "Define List",
                |r, cx| {
                    add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                    let a = abstract_xml(abstract_id, None, Levels::Full(&levels), d.name.as_deref(), &pics, &[], cx, &r.styles.colours);
                    r.set_abstract_xml(a)?;
                    let w = &cx.w;
                    r.set_num_xml(format!("<{w}num {w}numId=\"{num_id}\"><{w}abstractNumId {w}val=\"{abstract_id}\"/></{w}num>"))?;
                    link_styles(r, &links, abstract_id, cx)
                },
                &assign,
                true,
                fonts,
            );
        };
        let Some(abstract_id) = res.numbering.abstract_of(num, &res.styles) else { return Outcome::Unchanged("no list") };
        let old = res.numbering.abstracts.get(&abstract_id).map(|a| a.raw.clone());
        let cx = res.numbering.cx();
        let num_raw = res.numbering.nums.get(&num).map(|m| m.raw.clone()).unwrap_or_default();
        let over: Vec<Override> = override_levels(&num_raw, &cx)
            .into_iter()
            .enumerate()
            .map(|(k, x)| Some((x?, level_definition(res.numbering.nums.get(&num)?.level_overrides.get(&(k as i64))?))))
            .collect();
        if whole {
            let links = links(&d.levels, &now, num);
            return self.edit_lists(
                "Define List",
                |r, cx| {
                    add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                    let a = abstract_xml(abstract_id, old.as_deref(), Levels::Full(&defs), d.name.as_deref(), &pics, &over, cx, &r.styles.colours);
                    r.set_abstract_xml(a)?;
                    if let Some(m) = num_xml(&num_raw, Levels::Full(&defs), &pics, &over, cx) {
                        r.set_num_xml(m)?;
                    }
                    link_styles(r, &links, abstract_id, cx)
                },
                &[],
                false,
                fonts,
            );
        }
        let (new_abstract, new_num) = res.numbering.free_ids();
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
        let mut starts: Vec<(i64, i64)> = match res.numbering.nums.get(&num) {
            Some(m) if head.is_some_and(|h| assign.iter().any(|(p, _)| *p == h)) => {
                m.start_overrides.iter().filter(|(k, _)| (0..9).contains(*k)).map(|(&k, &v)| (k, v)).collect()
            }
            _ => Vec::new(),
        };
        starts.sort_unstable();
        self.edit_lists(
            "Define List",
            |r, cx| {
                add_pictures(&mut r.numbering_edits(), &new_pics, cx);
                let a = abstract_xml(new_abstract, old.as_deref(), Levels::Full(&defs), d.name.as_deref(), &pics, &over, cx, &r.styles.colours);
                r.set_abstract_xml(a)?;
                let w = &cx.w;
                let starts: String = starts
                    .iter()
                    .map(|(k, v)| format!("<{w}lvlOverride {w}ilvl=\"{k}\"><{w}startOverride {w}val=\"{v}\"/></{w}lvlOverride>"))
                    .collect();
                r.set_num_xml(format!("<{w}num {w}numId=\"{new_num}\"><{w}abstractNumId {w}val=\"{new_abstract}\"/>{starts}</{w}num>"))?;
                link_styles(r, &links, new_abstract, cx)
            },
            &assign,
            false,
            fonts,
        )
    }

    pub(super) fn set_numbering_value(&mut self, v: NumberingValue, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some((first, num, ilvl)) = self.selection_list() else { return Outcome::unavailable("the selection isn't in a list") };
        let Some(abstract_id) = res.numbering.abstract_of(num, &res.styles) else { return Outcome::Unchanged("no list") };
        let start = res.numbering.level(num, ilvl, &res.styles).map_or(1, |l| l.start.max(0) as u32);
        let following = self.list_from(first, num);
        let (_, new_num) = res.numbering.free_ids();
        match v.mode {
            NumberingMode::StartNew => {
                let value = if v.value == 0 { start } else { v.value };
                let assign: Vec<(ParaId, i64)> = following.iter().map(|&p| (p, new_num)).collect();
                self.edit_numbering(
                    "Set Numbering Value",
                    |n, cx, colours| {
                        let w = &cx.w;
                        n.set_num_xml(
                            format!(
                                "<{w}num {w}numId=\"{new_num}\"><{w}abstractNumId {w}val=\"{abstract_id}\"/><{w}lvlOverride {w}ilvl=\"{ilvl}\"><{w}startOverride {w}val=\"{value}\"/></{w}lvlOverride></{w}num>"
                            ),
                            colours,
                        )
                        .map(|_| ())
                    },
                    &assign,
                    false,
                    fonts,
                )
            }
            NumberingMode::Continue => {
                // The previous list of the same kind (bullets or numbers).
                let bullet = |n: i64, l: i64| res.numbering.level(n, l, &res.styles).is_some_and(|x| x.fmt == "bullet");
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
                let Some(prev_abstract) = res.numbering.abstract_of(prev, &res.styles) else { return Outcome::Unchanged("no list") };
                let assign: Vec<(ParaId, i64)> = following.iter().map(|&p| (p, new_num)).collect();
                let value = v.value.max(1);
                self.edit_numbering(
                    "Set Numbering Value",
                    |n, cx, colours| {
                        let w = &cx.w;
                        n.set_num_xml(
                            format!(
                                "<{w}num {w}numId=\"{new_num}\"><{w}abstractNumId {w}val=\"{prev_abstract}\"/><{w}lvlOverride {w}ilvl=\"{ilvl}\"><{w}startOverride {w}val=\"{value}\"/></{w}lvlOverride></{w}num>"
                            ),
                            colours,
                        )
                        .map(|_| ())
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

/// Adds picture bullets: each a `w:numPicBullet` of its image, which the writer gives its part.
fn add_pictures(n: &mut crate::docx::resolvers::NumberingEdits, pics: &[(i64, usize)], cx: &Cx) {
    let w = &cx.w;
    for &(id, image) in pics {
        let rel = format!("{}{image}", crate::docx::write::NEW_IMAGE);
        let raw = format!(
            concat!(
                r#"<{w}numPicBullet xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" "#,
                r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" {w}numPicBulletId="{id}"><{w}pict>"#,
                r#"<v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f">"#,
                r#"<v:stroke joinstyle="miter"/><v:formulas><v:f eqn="if lineDrawn pixelLineWidth 0"/><v:f eqn="sum @0 1 0"/><v:f eqn="sum 0 0 @1"/>"#,
                r#"<v:f eqn="prod @2 1 2"/><v:f eqn="prod @3 21600 pixelWidth"/><v:f eqn="prod @3 21600 pixelHeight"/><v:f eqn="sum @0 0 1"/>"#,
                r#"<v:f eqn="prod @6 1 2"/><v:f eqn="prod @7 21600 pixelWidth"/><v:f eqn="sum @8 21600 0"/><v:f eqn="prod @7 21600 pixelHeight"/>"#,
                r#"<v:f eqn="sum @10 21600 0"/></v:formulas><v:path o:extrusionok="f" gradientshapeok="t" o:connecttype="rect"/>"#,
                r##"<o:lock v:ext="edit" aspectratio="t"/></v:shapetype><v:shape id="_x0000_i{shape}" type="#_x0000_t75" style="width:9pt;height:9pt" o:bullet="t">"##,
                r#"<v:imagedata r:id="{rel}" o:title=""/></v:shape></{w}pict></{w}numPicBullet>"#
            ),
            w = w,
            id = id,
            shape = 1025 + id,
            rel = rel,
        );
        n.add_picture(id, raw, rel, image);
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
pub(super) fn linkable(styles: &crate::docx::styles::Styles, id: &str) -> Option<String> {
    let paragraph = |s: &&crate::docx::styles::Style| s.kind == crate::docx::styles::StyleKind::Paragraph;
    if let Some(s) = styles.get(id).filter(paragraph) {
        return Some(s.id.clone());
    }
    let name = crate::services::styles::builtin::built_in(id)?.name;
    styles.by_id.values().filter(paragraph).find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.id.clone())
}

/// Makes and ends the style links of the list `abstract_id`: a linked style is numbered with its level, and no other
/// level of any list links to it; an unlinked one numbered with the list no longer is. Answers the styles changed.
fn link_styles(r: &mut crate::docx::resolvers::Resolvers, links: &[Link], abstract_id: i64, cx: &Cx) -> Option<Vec<String>> {
    let scx = r.styles.cx();
    let sw = scx.w.clone();
    let mut changed = Vec::new();
    for (id, to) in links {
        let Some(style) = r.styles.get(id) else { continue };
        if to.is_none() && style.base.ppr.num_id.and_then(|n| r.numbering.abstract_of(n, &r.styles)) != Some(abstract_id) {
            continue;
        }
        let raw = super::styles::edit_child(&style.raw, "pPr", &scx, |p| match to {
            Some((num, 0)) => p.set("numPr", format!("<{sw}numPr><{sw}numId {sw}val=\"{num}\"/></{sw}numPr>")),
            Some((num, k)) => p.set("numPr", format!("<{sw}numPr><{sw}ilvl {sw}val=\"{k}\"/><{sw}numId {sw}val=\"{num}\"/></{sw}numPr>")),
            None => p.remove("numPr"),
        });
        changed.extend(r.set_style_xml(raw).ok()?);
        let Some((num, k)) = to else { continue };
        let others: Vec<(i64, String)> = r
            .numbering
            .abstracts
            .iter()
            .filter(|(a, abs)| {
                abs.levels.iter().enumerate().any(|(j, l)| (**a, j) != (abstract_id, *k) && l.as_ref().and_then(|l| l.pstyle.as_ref()) == Some(id))
            })
            .map(|(a, abs)| (*a, abs.raw.clone()))
            .collect();
        for (a, raw) in others {
            let keep = (a == abstract_id).then_some(*k);
            r.set_abstract_xml(unlinked(&raw, id, keep, cx))?;
        }
        // The instances' own levels
        let nums: Vec<(i64, String)> = r
            .numbering
            .nums
            .iter()
            .filter(|(n, m)| m.level_overrides.iter().any(|(j, l)| (**n, *j) != (*num, *k as i64) && l.pstyle.as_deref() == Some(id)))
            .map(|(n, m)| (*n, m.raw.clone()))
            .collect();
        for (n, raw) in nums {
            let keep = (n == *num).then_some(*k);
            r.set_num_xml(unlinked_num(&raw, id, keep, cx))?;
        }
    }
    changed.sort();
    changed.dedup();
    Some(changed)
}

/// A `w:abstractNum` whose levels (but `keep`) no longer link to the style `id`.
fn unlinked(raw: &str, id: &str, keep: Option<usize>, cx: &Cx) -> String {
    let mut a = Props::parse(Some(raw), "abstractNum", cx);
    a.reorder(ABSTRACT);
    for (_, x) in a.kids.iter_mut().filter(|(n, _)| n == "lvl") {
        if keep.is_some_and(|k| attr_of(x, "ilvl").as_deref() == Some(&k.to_string())) {
            continue;
        }
        if let Some(l) = unlinked_level(x, id, cx) {
            *x = l;
        }
    }
    a.xml().unwrap_or_else(|| raw.to_string())
}

/// A `w:num` whose overriding levels (but `keep`) no longer link to the style `id`.
fn unlinked_num(raw: &str, id: &str, keep: Option<usize>, cx: &Cx) -> String {
    let mut m = Props::parse(Some(raw), "num", cx);
    for (_, x) in m.kids.iter_mut().filter(|(n, _)| n == "lvlOverride") {
        if keep.is_some_and(|k| attr_of(x, "ilvl").as_deref() == Some(&k.to_string())) {
            continue;
        }
        let mut o = Props::parse(Some(x), "lvlOverride", cx);
        if let Some(l) = o.get("lvl").and_then(|l| unlinked_level(l, id, cx)) {
            o.set("lvl", l);
            *x = o.xml().unwrap_or_default();
        }
    }
    m.xml().unwrap_or_else(|| raw.to_string())
}

/// A `w:lvl` without its `w:pStyle`; `None` when that isn't `id`.
fn unlinked_level(x: &str, id: &str, cx: &Cx) -> Option<String> {
    let mut l = Props::parse(Some(x), "lvl", cx);
    l.reorder(LVL);
    (l.get("pStyle").and_then(|p| attr_of(p, "val")).as_deref() == Some(id)).then(|| {
        l.remove("pStyle");
        l.xml().unwrap_or_default()
    })
}
