//! Tab stops: the ruler's and the Tabs dialog's commands over the selected paragraphs, and the document's default
//! spacing. A paragraph keeps the stops its style gives it; its own `w:tabs` adds stops and clears inherited ones.

use std::collections::HashMap;

use crate::api::command::{AddTabStop, MoveTabStop};
use crate::api::services::TabStopInfo;
use crate::api::value::{Points, TabAlignment, TabLeader};
use crate::docx::props::{Leader, Tab, TabKind};
use crate::docx::write::props::Props;
use crate::font::Fonts;
use crate::layout::ParaId;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::step::Step;

/// How close a position must be to a stop to name it, points.
const NEAR: f64 = 0.5;
/// The longest length a stop or the default spacing takes, 22 in.
const MAX: f64 = 1584.0;

/// A length rounded to the twips the file holds.
fn twips(pt: f64) -> f64 {
    (pt * 20.0).round() / 20.0
}

fn same(a: &Tab, b: &Tab) -> bool {
    (a.pos - b.pos).abs() < 0.025 && a.kind == b.kind && a.leader == b.leader
}

/// A stop as the ruler model shows it.
fn info(t: &Tab, inherited: bool) -> TabStopInfo {
    let alignment = match t.kind {
        TabKind::Center => TabAlignment::Center,
        TabKind::Right => TabAlignment::Right,
        TabKind::Decimal => TabAlignment::Decimal,
        TabKind::Bar => TabAlignment::Bar,
        TabKind::Left | TabKind::Num | TabKind::Clear => TabAlignment::Left,
    };
    let leader = match t.leader {
        Leader::None => TabLeader::None,
        Leader::Dot => TabLeader::Dots,
        Leader::Hyphen => TabLeader::Dashes,
        Leader::Underscore => TabLeader::Underline,
        Leader::MiddleDot => TabLeader::MiddleDot,
    };
    TabStopInfo { position: Points(t.pos), alignment, leader, inherited }
}

fn tab(a: &AddTabStop) -> Tab {
    let kind = match a.alignment.unwrap_or_default() {
        TabAlignment::Left => TabKind::Left,
        TabAlignment::Center => TabKind::Center,
        TabAlignment::Right => TabKind::Right,
        TabAlignment::Decimal => TabKind::Decimal,
        TabAlignment::Bar => TabKind::Bar,
    };
    // shortcut: a heavy leader is kept as an underline, the only heavy-looking leader the model has.
    let leader = match a.leader.unwrap_or_default() {
        TabLeader::None => Leader::None,
        TabLeader::Dots => Leader::Dot,
        TabLeader::Dashes => Leader::Hyphen,
        TabLeader::Underline | TabLeader::Heavy => Leader::Underscore,
        TabLeader::MiddleDot => Leader::MiddleDot,
    };
    Tab { pos: twips(a.position.0), kind, leader }
}

/// A paragraph's own `w:tabs`: the stops it adds, and a clearing stop for each inherited one it drops.
fn own_xml(own: &[Tab], w: &str) -> String {
    let tabs: String = own
        .iter()
        .map(|t| {
            let val = match t.kind {
                TabKind::Left => "left",
                TabKind::Center => "center",
                TabKind::Right => "right",
                TabKind::Decimal => "decimal",
                TabKind::Bar => "bar",
                TabKind::Num => "num",
                TabKind::Clear => "clear",
            };
            let leader = match t.leader {
                Leader::None => "",
                Leader::Dot => "dot",
                Leader::Hyphen => "hyphen",
                Leader::Underscore => "underscore",
                Leader::MiddleDot => "middleDot",
            };
            let leader = if leader.is_empty() { String::new() } else { format!(" {w}leader=\"{leader}\"") };
            format!("<{w}tab {w}val=\"{val}\"{leader} {w}pos=\"{}\"/>", (t.pos * 20.0).round() as i64)
        })
        .collect();
    format!("<{w}tabs>{tabs}</{w}tabs>")
}

impl Editor {
    /// The stops a paragraph has without its own `w:tabs`: its style's, its list's and its table style's.
    fn inherited_tabs(&self, para: ParaId) -> Vec<Tab> {
        let (Some(res), Some(p)) = (self.doc.resolver.clone(), self.index.para(&self.doc, para)) else { return Vec::new() };
        let cx = self.cx();
        let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
        pr.remove("tabs");
        let region = self.region_of_para(para, &res);
        res.para_from_xml(pr.xml().as_deref(), region.as_ref(), &cx).style.tabs
    }

    /// A paragraph's tab stops in position order, each saying whether it is inherited.
    pub(crate) fn tab_stops(&self, para: ParaId) -> Vec<TabStopInfo> {
        let Some(p) = self.index.para(&self.doc, para) else { return Vec::new() };
        let base = self.inherited_tabs(para);
        p.style.tabs.iter().filter(|t| t.kind != TabKind::Clear).map(|t| info(t, base.iter().any(|b| same(b, t)))).collect()
    }

    /// The paragraphs the ruler shows: the selection's (the first one's values, and whether the others differ).
    pub(crate) fn ruler_paras(&mut self) -> Vec<ParaId> {
        self.selected_paras()
    }

    /// Page `i`'s geometry as laid out now: the ruler's cells and rows.
    pub(crate) fn page_geometry(&mut self, i: usize) -> Option<std::rc::Rc<crate::editing::geometry::PageGeometry>> {
        let snapshot = self.layout.snapshot();
        self.pages.sync(&snapshot);
        self.pages.page(i, &self.index)
    }

    /// Each selected paragraph's stops changed by `edit` (false: this paragraph has nothing to change), written as
    /// its own stops over the inherited ones. Refused with `none` when no paragraph changes.
    fn edit_tabs(&mut self, label: &'static str, none: &str, edit: &dyn Fn(&mut Vec<Tab>) -> bool, fonts: &Fonts) -> Outcome {
        let mut own: HashMap<u64, Vec<Tab>> = HashMap::new();
        for para in self.selected_paras() {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut tabs: Vec<Tab> = p.style.tabs.iter().filter(|t| t.kind != TabKind::Clear).copied().collect();
            if !edit(&mut tabs) {
                continue;
            }
            let base = self.inherited_tabs(para);
            // An inherited stop no stop stands at any more is cleared; a stop the style doesn't give is the paragraph's.
            let mut mine: Vec<Tab> = base
                .iter()
                .filter(|b| !tabs.iter().any(|t| (t.pos - b.pos).abs() < 0.025))
                .map(|b| Tab { kind: TabKind::Clear, leader: Leader::None, ..*b })
                .collect();
            mine.extend(tabs.iter().filter(|t| !base.iter().any(|b| same(b, t))));
            mine.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            own.insert(p.id, mine);
        }
        if own.is_empty() {
            return Outcome::unavailable(none);
        }
        self.format_paras(
            label,
            &|pr, cx, p| match own.get(&p.id) {
                Some(mine) if mine.is_empty() => pr.remove("tabs"),
                Some(mine) => pr.set("tabs", own_xml(mine, &cx.w)),
                None => {}
            },
            fonts,
        )
    }

    pub(in crate::editing) fn add_tab_stop(&mut self, a: AddTabStop, fonts: &Fonts) -> Outcome {
        if !(0.0..=MAX).contains(&a.position.0) {
            return Outcome::invalid("the position must be from 0 to 1584");
        }
        let new = tab(&a);
        self.edit_tabs(
            "Add Tab Stop",
            "no paragraph is selected",
            &|tabs| {
                tabs.retain(|t| (t.pos - new.pos).abs() >= NEAR);
                tabs.push(new);
                tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                true
            },
            fonts,
        )
    }

    pub(in crate::editing) fn move_tab_stop(&mut self, m: MoveTabStop, fonts: &Fonts) -> Outcome {
        if ![m.from.0, m.to.0].iter().all(|v| (0.0..=MAX).contains(v)) {
            return Outcome::invalid("the positions must be from 0 to 1584");
        }
        let to = twips(m.to.0);
        self.edit_tabs(
            "Move Tab Stop",
            "no tab stop is there",
            &|tabs| {
                let Some(k) = tabs.iter().position(|t| (t.pos - m.from.0).abs() < NEAR) else { return false };
                let moved = Tab { pos: to, ..tabs.remove(k) };
                tabs.retain(|t| (t.pos - to).abs() >= NEAR);
                tabs.push(moved);
                tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                true
            },
            fonts,
        )
    }

    pub(in crate::editing) fn delete_tab_stop(&mut self, at: Points, fonts: &Fonts) -> Outcome {
        self.edit_tabs(
            "Delete Tab Stop",
            "no tab stop is there",
            &|tabs| {
                let n = tabs.len();
                tabs.retain(|t| (t.pos - at.0).abs() >= NEAR);
                tabs.len() < n
            },
            fonts,
        )
    }

    pub(in crate::editing) fn clear_tab_stops(&mut self, fonts: &Fonts) -> Outcome {
        self.edit_tabs(
            "Clear Tab Stops",
            "there are no tab stops",
            &|tabs| {
                let had = !tabs.is_empty();
                tabs.clear();
                had
            },
            fonts,
        )
    }

    pub(in crate::editing) fn set_default_tab_stop(&mut self, spacing: Points, fonts: &Fonts) -> Outcome {
        if !(1.0..=MAX).contains(&spacing.0) {
            return Outcome::invalid("the spacing must be from 1 to 1584");
        }
        let v = twips(spacing.0);
        if (self.doc.settings.default_tab - v).abs() < 0.025 {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Default Tab Stops");
        if self.step(&mut tx, Step::DefaultTab(v)).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the setting can't change");
        }
        self.commit(tx, None, fonts)
    }
}
