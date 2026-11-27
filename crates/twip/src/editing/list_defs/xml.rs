//! The list edits as patches of the numbering and styles parts' XML, as they were made before the typed edits: what
//! the typed edits' writing is checked against.

use crate::api::command::{FollowNumber, LevelAlignment, ListLevel};
use crate::api::value::{AutoOr, NoneOr, Points};
use crate::docx::write::props::{Cx, Props, esc};

use super::{LabelFormat, LevelDefinition, Levels, PicChange, level_definition};

/// CT_Lvl's children in schema order.
const LVL: &[&str] = &["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff", "lvlText", "lvlPicBulletId", "legacy", "lvlJc", "pPr", "rPr"];
/// CT_AbstractNum's children in schema order.
const ABSTRACT: &[&str] = &["nsid", "multiLevelType", "tmpl", "name", "styleLink", "numStyleLink", "lvl"];

/// A level a `w:num` defines in its `w:lvlOverride`: its `w:lvl` and that level in full.
pub(super) type Override = Option<(String, LevelDefinition)>;

/// A level's `w:rPr` with the formatting that differs from `was` written or removed.
fn label_write(f: &LabelFormat, was: &LabelFormat, rpr: &mut Props, w: &str) {
    let flag = |rpr: &mut Props, on: Option<bool>, names: [&str; 2]| {
        for n in names {
            match on {
                Some(true) => rpr.set(n, format!("<{w}{n}/>")),
                Some(false) => rpr.set(n, format!("<{w}{n} {w}val=\"0\"/>")),
                None => rpr.remove(n),
            }
        }
    };
    if f.bold != was.bold {
        flag(rpr, f.bold, ["b", "bCs"]);
    }
    if f.italic != was.italic {
        flag(rpr, f.italic, ["i", "iCs"]);
    }
    if f.underline != was.underline {
        match f.underline {
            Some(u) => rpr.set("u", format!("<{w}u {w}val=\"{}\"/>", crate::docx::line_style::underline_val(super::super::commands::underline(u)))),
            None => rpr.remove("u"),
        }
    }
    if f.size != was.size {
        for n in ["sz", "szCs"] {
            match f.size {
                Some(s) => rpr.set(n, format!("<{w}{n} {w}val=\"{}\"/>", (s.0 * 2.0).round() as i64)),
                None => rpr.remove(n),
            }
        }
    }
    if f.color != was.color {
        match f.color {
            Some(AutoOr::Auto) => rpr.set("color", format!("<{w}color {w}val=\"auto\"/>")),
            Some(AutoOr::Value(c)) => rpr.set("color", format!("<{w}color {w}val=\"{:06X}\"/>", c.0 & 0xFF_FFFF)),
            None => rpr.remove("color"),
        }
    }
    if f.highlight != was.highlight {
        match f.highlight {
            Some(NoneOr::Value(h)) => rpr.set("highlight", format!("<{w}highlight {w}val=\"{}\"/>", super::super::commands::highlight_value(h))),
            Some(NoneOr::None) => rpr.set("highlight", format!("<{w}highlight {w}val=\"none\"/>")),
            None => rpr.remove("highlight"),
        }
    }
}

/// A `w:lvl` for level `k` set to `def`: `old` (its XML and its view) edited where they differ, so what the view
/// doesn't show is kept.
pub(super) fn level_xml(k: usize, old: Option<(&str, &LevelDefinition)>, def: &LevelDefinition, pic: PicChange, cx: &Cx) -> String {
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
        label_write(&def.label, &was_def.label, &mut rpr, w);
        match rpr.xml() {
            Some(x) => p.set("rPr", x),
            None => p.remove("rPr"),
        }
    }
    p.xml().unwrap_or_default()
}

/// `ppr`'s tab stops with the level's own at `tab`: moved with its kind and leader, added as a list tab, or removed.
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
    let parsed = old.and_then(|o| cx.parse(o, |_, el| crate::docx::list_xml::parse_def(el))).flatten();
    for k in 0..if copy { 9 } else { levels.len().min(9) } {
        let over = over.get(k).and_then(Option::as_ref);
        if over.is_some() && !copy {
            continue;
        }
        let raw = a.kids.iter().position(|(n, x)| n == "lvl" && attr_of(x, "ilvl").as_deref() == Some(&k.to_string()));
        let old_level = parsed.as_ref().and_then(|d| d.levels.get(k).cloned().flatten()).map(|l| level_definition(&l, colours));
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
pub(super) fn override_levels(raw: &str, cx: &Cx) -> [Option<String>; 9] {
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
pub(super) fn num_xml(raw: &str, levels: Levels, pics: &[PicChange], over: &[Override], cx: &Cx) -> Option<String> {
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
pub(super) fn attr_of(xml: &str, local: &str) -> Option<String> {
    let at =
        xml.find(&format!(":{local}=\"")).map(|i| i + local.len() + 3).or_else(|| xml.find(&format!(" {local}=\"")).map(|i| i + local.len() + 3))?;
    Some(xml[at..at + xml[at..].find('"')?].to_string())
}

/// A new `w:num` `num` numbered with `abstract_id`, each `(level, value)` of `starts` a level's start.
pub(super) fn num_new(num: i64, abstract_id: i64, starts: &[(i64, i64)], cx: &Cx) -> String {
    let w = &cx.w;
    let starts: String =
        starts.iter().map(|(k, v)| format!("<{w}lvlOverride {w}ilvl=\"{k}\"><{w}startOverride {w}val=\"{v}\"/></{w}lvlOverride>")).collect();
    format!("<{w}num {w}numId=\"{num}\"><{w}abstractNumId {w}val=\"{abstract_id}\"/>{starts}</{w}num>")
}

/// A `w:abstractNum` whose levels (but `keep`) no longer link to the style `id`.
pub(super) fn unlinked(raw: &str, id: &str, keep: Option<usize>, cx: &Cx) -> String {
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
pub(super) fn unlinked_num(raw: &str, id: &str, keep: Option<usize>, cx: &Cx) -> String {
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

/// A `w:style` numbered with `to`'s list at its level (level 0 written as none), or with no `w:numPr`.
pub(super) fn style_numbered(style: &str, to: Option<(i64, usize)>, cx: &Cx) -> String {
    let sw = &cx.w;
    edit_child(style, "pPr", cx, |p| match to {
        Some((num, 0)) => p.set("numPr", format!("<{sw}numPr><{sw}numId {sw}val=\"{num}\"/></{sw}numPr>")),
        Some((num, k)) => p.set("numPr", format!("<{sw}numPr><{sw}ilvl {sw}val=\"{k}\"/><{sw}numId {sw}val=\"{num}\"/></{sw}numPr>")),
        None => p.remove("numPr"),
    })
}

/// `raw` (a `w:style`) with its `child` edited in place, or taken out when `edit` leaves it empty.
fn edit_child(raw: &str, child: &str, cx: &Cx, edit: impl FnOnce(&mut Props)) -> String {
    let mut el = Props::parse(Some(raw), "style", cx);
    let mut props = Props::parse(el.get(child), child, cx);
    edit(&mut props);
    match props.xml() {
        Some(x) => el.set(child, x),
        None => el.remove(child),
    }
    el.reorder(crate::docx::write::props::STYLE);
    el.xml().unwrap_or_else(|| raw.to_string())
}

/// Level `k`'s `w:lvl` in a `w:abstractNum`, or in a `w:num`'s `w:lvlOverride` (`over`).
pub(super) fn level_in(raw: &str, k: i64, over: bool, cx: &Cx) -> Option<String> {
    let parent = Props::parse(Some(raw), if over { "num" } else { "abstractNum" }, cx);
    let ilvl = |x: &str| cx.parse(x, |_, n| crate::docx::xml::attr(n, "ilvl").and_then(|v| v.parse::<i64>().ok())).flatten();
    let (name, x) = parent.kids.iter().find(|(n, x)| n == if over { "lvlOverride" } else { "lvl" } && ilvl(x) == Some(k))?;
    if name == "lvlOverride" { Props::parse(Some(x), "lvlOverride", cx).get("lvl").map(str::to_string) } else { Some(x.clone()) }
}

/// The `w:abstractNum` (or `w:num`, `over`) with level `k`'s `w:lvl` replaced by `lvl`; a `w:num` without an override
/// of that level gets one, in level order.
pub(super) fn with_level(raw: &str, k: i64, over: bool, lvl: &str, cx: &Cx) -> String {
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

/// A level's `w:lvl` with its `w:rPr` replaced by `rpr`, or without one.
pub(super) fn with_rpr(lvl: &str, rpr: Option<String>, cx: &Cx) -> Option<String> {
    let mut lp = Props::parse(Some(lvl), "lvl", cx);
    match rpr {
        Some(x) => lp.set("rPr", x),
        None => lp.remove("rPr"),
    }
    lp.xml()
}

/// A level's `w:rPr`.
pub(super) fn rpr_of(lvl: &str, cx: &Cx) -> Option<String> {
    Props::parse(Some(lvl), "lvl", cx).get("rPr").map(str::to_string)
}

/// The nine `w:lvl`s of a new list of bullets or numbers.
pub(super) fn new_list_levels(bullet: bool) -> String {
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
    levels
}

/// The `w:lvl` of the single-level bullet list Word gives the List Bullet style, linked to style `style`.
pub(super) fn style_bullets_level(style: &str) -> String {
    format!(
        r#"<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:pStyle w:val="{}"/><w:lvlText w:val="{}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr><w:rPr><w:rFonts w:ascii="Symbol" w:hAnsi="Symbol" w:hint="default"/></w:rPr></w:lvl>"#,
        esc(style),
        '\u{F0B7}'
    )
}

/// A new list's `w:abstractNum` and `w:num` of `levels` (`w:lvl` elements), each binding the main namespace itself.
pub(super) fn added_list(abstract_id: i64, num_id: i64, multi: &str, levels: &str) -> (String, String) {
    (
        format!(
            r#"<w:abstractNum xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:abstractNumId="{abstract_id}"><w:multiLevelType w:val="{multi}"/>{levels}</w:abstractNum>"#
        ),
        format!(
            r#"<w:num xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:numId="{num_id}"><w:abstractNumId w:val="{abstract_id}"/></w:num>"#
        ),
    )
}

/// A list style's new `w:abstractNum`: `a`, a definition, with a `w:styleLink` to style `style` after its
/// multiLevelType.
pub(super) fn style_linked(mut a: String, style: &str, cx: &Cx) -> String {
    let w = &cx.w;
    let link = format!("<{w}styleLink {w}val=\"{}\"/>", esc(style));
    if let Some(at) = a.find(&format!("<{w}lvl ")) {
        a.insert_str(at, &link);
    }
    a
}

/// A new `w:numPicBullet` as the picture bullets were added.
pub(super) fn picture(id: i64, rel: &str, cx: &Cx) -> String {
    let w = &cx.w;
    format!(
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
    )
}
