//! List definitions: the levels of the selection's list as the Define dialogs show them, and the edits that define
//! them, set numbering values, and adjust indents. Each edits the numbering part's XML and reads it again.

use std::rc::Rc;

use crate::api::command::{
    FollowNumber, LevelAlignment, ListApplyTo, ListDefinitionChange, ListIndents, ListLevel, NumberFormat, NumberingMode, NumberingValue,
};
use crate::api::value::{Points, StyleId};
use crate::docx::numbering::{Level, Numbering};
use crate::docx::props::TabKind;
use crate::docx::write::props::{Cx, Props, esc};
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Editor;
use super::commands::Outcome;
use super::step::Step;

/// CT_Lvl's children in schema order.
const LVL: &[&str] = &["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff", "lvlText", "lvlPicBulletId", "legacy", "lvlJc", "pPr", "rPr"];
/// CT_AbstractNum's children in schema order.
const ABSTRACT: &[&str] = &["nsid", "multiLevelType", "tmpl", "name", "styleLink", "numStyleLink", "lvl"];

/// A level as the API shows it.
pub fn list_level(l: &Level) -> ListLevel {
    let left = l.ppr.ind_left.unwrap_or(0.0);
    let first = l.ppr.ind_first.unwrap_or(0.0);
    ListLevel {
        format: NumberFormat::of_val(&l.fmt),
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
    }
}

/// A `w:lvl` for level `k` set to `new`: `old` (its XML and its API view) edited where they differ, so what the API
/// doesn't show is kept.
fn level_xml(k: usize, old: Option<(&str, &ListLevel)>, new: &ListLevel, pic: Option<Option<i64>>, cx: &Cx) -> String {
    let w = &cx.w;
    let mut p = Props::parse(old.map(|o| o.0), "lvl", cx);
    p.reorder(LVL);
    if old.is_none() {
        p = Props::parse(Some(&format!("<{w}lvl {w}ilvl=\"{k}\"><{w}start {w}val=\"1\"/></{w}lvl>")), "lvl", cx);
        p.reorder(LVL);
    }
    let was = old.map(|o| o.1.clone()).unwrap_or_else(|| ListLevel { start: 1, ..ListLevel::default() });
    let val = |name: &str, v: &str| format!("<{w}{name} {w}val=\"{}\"/>", esc(v));
    if old.is_none() || was.start != new.start {
        p.set("start", val("start", &new.start.to_string()));
    }
    if old.is_none() || was.format != new.format {
        p.set("numFmt", val("numFmt", new.format.val()));
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
        match new.tab {
            Some(t) => ppr.set("tabs", format!("<{w}tabs><{w}tab {w}val=\"num\" {w}pos=\"{}\"/></{w}tabs>", twips(t))),
            None => ppr.remove("tabs"),
        }
        if let Some(x) = ppr.xml() {
            p.set("pPr", x);
        }
    }
    if old.is_none() || was.font != new.font {
        let mut rpr = Props::parse(p.get("rPr"), "rPr", cx);
        match &new.font {
            Some(f) => {
                let f = esc(f);
                rpr.set("rFonts", format!("<{w}rFonts {w}ascii=\"{f}\" {w}hAnsi=\"{f}\" {w}hint=\"default\"/>"));
            }
            None => rpr.remove("rFonts"),
        }
        match rpr.xml() {
            Some(x) => p.set("rPr", x),
            None => p.remove("rPr"),
        }
    }
    p.xml().unwrap_or_default()
}

/// A `w:abstractNum` with its levels from `levels` (edited from `old`'s where it has them).
/// A level's picture bullet change: `None` keeps it, `Some(None)` removes it, `Some(Some(id))` sets it.
pub(super) type PicChange = Option<Option<i64>>;

pub(super) fn abstract_xml(id: i64, old: Option<&str>, levels: &[ListLevel], name: Option<&str>, pics: &[PicChange], cx: &Cx) -> String {
    let w = &cx.w;
    let blank = format!("<{w}abstractNum {w}abstractNumId=\"{id}\"><{w}multiLevelType {w}val=\"hybridMultilevel\"/></{w}abstractNum>");
    let was_id = old.and_then(|o| attr_of(o, "abstractNumId"));
    // A copy has the new id and is a new list: its nsid goes.
    let renamed = match (old, &was_id) {
        (Some(o), Some(was)) => o.replacen(&format!("abstractNumId=\"{was}\""), &format!("abstractNumId=\"{id}\""), 1),
        _ => blank,
    };
    let mut a = Props::parse(Some(&renamed), "abstractNum", cx);
    a.reorder(ABSTRACT);
    if was_id.is_some_and(|was| was != id.to_string()) {
        a.remove("nsid");
        a.remove("tmpl");
    }
    match name {
        Some("") => a.remove("name"),
        Some(n) => a.set("name", format!("<{w}name {w}val=\"{}\"/>", esc(n))),
        None => {}
    }
    let parsed = old.and_then(|o| cx.parse(o, |text, el| crate::docx::numbering::parse_abstract(el, text, &Default::default()))).flatten();
    for (k, new) in levels.iter().enumerate().take(9) {
        let raw = a.kids.iter().position(|(n, x)| n == "lvl" && attr_of(x, "ilvl").as_deref() == Some(&k.to_string()));
        let old_level = parsed.as_ref().and_then(|(_, abs)| abs.levels.get(k).cloned().flatten()).map(|l| list_level(&l));
        let xml = level_xml(k, raw.map(|i| a.kids[i].1.as_str()).zip(old_level.as_ref()), new, pics.get(k).copied().flatten(), cx);
        match raw {
            Some(i) => a.kids[i].1 = xml,
            None => a.add("lvl", xml),
        }
    }
    a.xml().unwrap_or_default()
}

/// An attribute's value in a start tag or element, by its local name.
fn attr_of(xml: &str, local: &str) -> Option<String> {
    let at =
        xml.find(&format!(":{local}=\"")).map(|i| i + local.len() + 3).or_else(|| xml.find(&format!(" {local}=\"")).map(|i| i + local.len() + 3))?;
    Some(xml[at..at + xml[at..].find('"')?].to_string())
}

impl Editor {
    /// The selection's list: its first list paragraph, `numId` and level.
    pub(super) fn selection_list(&mut self) -> Option<(ParaId, i64, i64)> {
        self.selected_paras().into_iter().find_map(|para| {
            let (num, ilvl) = self.index.para(&self.doc, para)?.style.num.filter(|(id, _)| *id != 0)?;
            Some((para, num, ilvl))
        })
    }

    /// The nine levels of `num` as the API shows them, with their picture bullets.
    pub fn list_levels(&self, num: i64) -> Option<Vec<ListLevel>> {
        let res = self.doc.resolver.as_ref()?;
        let levels = res.numbering.levels(num, &res.styles)?;
        Some(
            levels
                .iter()
                .map(|l| {
                    let Some(l) = l else { return ListLevel { start: 1, ..ListLevel::default() } };
                    let mut v = list_level(l);
                    v.picture = l
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

    /// The list paragraphs in document order: each paragraph's id and `(numId, ilvl)`.
    pub(super) fn list_paras(&self) -> Vec<(ParaId, i64, i64)> {
        fn walk(blocks: &[crate::docx::Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => out.push(ParaId(p.id)),
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut order = Vec::new();
        self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut order));
        order
            .into_iter()
            .filter_map(|p| {
                let (num, ilvl) = self.index.para(&self.doc, p)?.style.num.filter(|(id, _)| *id != 0)?;
                Some((p, num, ilvl))
            })
            .collect()
    }

    /// Edits the numbering table in one step, then gives the paragraphs `to` the `numId` in `assign`; every list
    /// paragraph is formatted and labelled again.
    pub(super) fn edit_numbering(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut Numbering, &Cx, &crate::docx::props::ThemeColours) -> Option<()>,
        assign: &[(ParaId, i64)],
        fresh: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let list_style = fresh && res.styles.by_id.contains_key("ListParagraph");
        let mut next = (*res).clone();
        let cx = next.numbering.cx();
        if edit(&mut next.numbering, &cx, &res.styles.colours).is_none() {
            return Outcome::Unchanged("the list didn't change");
        }
        let mut tx = self.begin(label);
        let added = self.doc.added_lists.clone();
        if self.step(&mut tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the list didn't change");
        }
        for &(para, num) in assign {
            let ilvl = if fresh { 0 } else { self.index.para(&self.doc, para).and_then(|p| p.style.num).map_or(0, |(_, l)| l.max(0)) };
            let ok = self.repara(&mut tx, para, &|p, w| {
                p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{ilvl}\"/><{w}numId {w}val=\"{num}\"/></{w}numPr>"));
                if list_style && p.get("pStyle").is_none() {
                    p.set("pStyle", format!("<{w}pStyle {w}val=\"ListParagraph\"/>"));
                }
            });
            if !ok {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
        }
        let lists: Vec<ParaId> = self.list_paras().into_iter().map(|(p, _, _)| p).collect();
        tx.stale_paras.extend(lists);
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
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let paras = self.selected_paras();
        let current = self.selection_list().map(|(_, n, _)| n);
        let (pics, new_pics) = self.picture_changes(&d.levels, current);
        let Some((first, num, _)) = self.selection_list() else {
            // Outside a list: a new one, for the selected paragraphs.
            let (abstract_id, num_id) = res.numbering.free_ids();
            let assign: Vec<(ParaId, i64)> = paras.iter().map(|&p| (p, num_id)).collect();
            let levels = complete(&d.levels);
            return self.edit_numbering(
                "Define List",
                |n, cx, colours| {
                    add_pictures(n, &new_pics, cx);
                    let a = abstract_xml(abstract_id, None, &levels, d.name.as_deref(), &pics, cx);
                    n.set_abstract_xml(a, colours)?;
                    let w = &cx.w;
                    n.set_num_xml(format!("<{w}num {w}numId=\"{num_id}\"><{w}abstractNumId {w}val=\"{abstract_id}\"/></{w}num>"), colours)?;
                    Some(())
                },
                &assign,
                true,
                fonts,
            );
        };
        let Some(abstract_id) = res.numbering.abstract_of(num, &res.styles) else { return Outcome::Unchanged("no list") };
        let old = res.numbering.abstracts.get(&abstract_id).map(|a| a.raw.clone());
        match d.apply_to {
            ListApplyTo::WholeList => self.edit_numbering(
                "Define List",
                |n, cx, colours| {
                    add_pictures(n, &new_pics, cx);
                    let a = abstract_xml(abstract_id, old.as_deref(), &d.levels, d.name.as_deref(), &pics, cx);
                    n.set_abstract_xml(a, colours).map(|_| ())
                },
                &[],
                false,
                fonts,
            ),
            to => {
                let (new_abstract, new_num) = res.numbering.free_ids();
                let assign: Vec<(ParaId, i64)> = match to {
                    ListApplyTo::ThisPointForward => self.list_from(first, num).into_iter().map(|p| (p, new_num)).collect(),
                    _ => paras
                        .iter()
                        .filter(|&&p| self.index.para(&self.doc, p).is_some_and(|q| q.style.num.is_some_and(|(n, _)| n == num)))
                        .map(|&p| (p, new_num))
                        .collect(),
                };
                self.edit_numbering(
                    "Define List",
                    |n, cx, colours| {
                        add_pictures(n, &new_pics, cx);
                        let a = abstract_xml(new_abstract, old.as_deref(), &d.levels, d.name.as_deref(), &pics, cx);
                        n.set_abstract_xml(a, colours)?;
                        let w = &cx.w;
                        n.set_num_xml(format!("<{w}num {w}numId=\"{new_num}\"><{w}abstractNumId {w}val=\"{new_abstract}\"/></{w}num>"), colours)?;
                        Some(())
                    },
                    &assign,
                    false,
                    fonts,
                )
            }
        }
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
        for l in levels.iter_mut().take(k) {
            l.picture = None;
        }
        self.set_list_definition(ListDefinitionChange { levels, apply_to: ListApplyTo::WholeList, name: None }, fonts)
    }
}

/// Adds picture bullets: each a `w:numPicBullet` of its image, which the writer gives its part.
fn add_pictures(n: &mut Numbering, pics: &[(i64, usize)], cx: &Cx) {
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
        n.pic_bullets.insert(id, crate::docx::numbering::PicBullet { raw, rel: Some(rel), image: Some(image) });
    }
}

/// Nine levels: `levels`, then the default ones after them.
pub(super) fn complete(levels: &[ListLevel]) -> Vec<ListLevel> {
    (0..9)
        .map(|k| {
            levels.get(k).cloned().unwrap_or_else(|| ListLevel {
                format: [NumberFormat::Decimal, NumberFormat::LowerLetter, NumberFormat::LowerRoman][k % 3],
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
