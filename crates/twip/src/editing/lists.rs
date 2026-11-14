//! Lists

use std::rc::Rc;

use crate::docx::numbering::Counters;
use crate::docx::props::RunProps;
use crate::docx::write::props::Props;
use crate::docx::{Block, ListLabel};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::selection::Selection;
use super::step::{Step, para_len};
use super::{Editor, Tx};

impl Editor {
    pub(super) fn list_item_start(&self) -> Option<(ParaId, (i64, i64))> {
        let Selection::Caret(c) = self.selection else { return None };
        let (para, off) = c.para_offset()?;
        let p = self.index.para(&self.doc, para)?;
        (off == Unit(0)).then_some(())?;
        p.style.num.filter(|(id, _)| *id != 0).map(|n| (para, n))
    }

    pub(super) fn repara(&mut self, tx: &mut Tx, para: ParaId, edit: &dyn Fn(&mut Props, &str)) -> bool {
        let Some(res) = self.doc.resolver.clone() else { return false };
        let cx = self.cx();
        let Some(p) = self.index.para(&self.doc, para) else { return false };
        let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
        edit(&mut pr, &cx.w);
        let region = self.region_of_para(para, &res);
        let r = res.para_from_xml(pr.xml().as_deref(), region.as_ref(), &cx);
        let (mark, list) = (p.mark.clone(), p.list.clone());
        tx.stale_paras.push(para);
        self.step(tx, Step::Para { para, style: Rc::new(r.style), mark, list }).is_ok()
    }

    pub(super) fn list_enter(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let (para, _) = self.list_item_start()?;
        if self.index.para(&self.doc, para).map(para_len) != Some(Unit(0)) {
            return None;
        }
        let mut tx = self.begin("Numbering");
        let ok = self.repara(&mut tx, para, &|p, w| {
            p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"0\"/><{w}numId {w}val=\"0\"/></{w}numPr>"));
            p.set("ind", format!("<{w}ind {w}left=\"0\" {w}firstLine=\"0\"/>"));
        });
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("numbering failed"));
        }
        Some(self.commit(tx, None, fonts))
    }

    pub(super) fn list_backspace(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let Selection::Caret(c) = self.selection else { return None };
        let (para, off) = c.para_offset()?;
        if off != Unit(0) {
            return None;
        }
        let p = self.index.para(&self.doc, para)?;
        let twips = |pt: f64| (pt * 20.0).round() as i64;
        let left = p.style.ind_left;
        let mut tx = self.begin("Paragraph Formatting");
        let ok = if p.style.num.is_some_and(|(id, _)| id != 0) {
            let l = twips(left);
            self.repara(&mut tx, para, &|p, w| {
                p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"0\"/><{w}numId {w}val=\"0\"/></{w}numPr>"));
                p.set("ind", format!("<{w}ind {w}left=\"{l}\" {w}firstLine=\"0\"/>"));
            })
        } else {
            let direct_ind = p.style.direct.0.as_deref().is_some_and(|x| x.contains("ind "));
            if !(left > 0.0 && p.style.ind_first == 0.0 && direct_ind) {
                self.abort(tx);
                return None;
            }
            let l = twips((((left - 0.01) / 18.0).floor() * 18.0).max(0.0));
            self.repara(&mut tx, para, &|p, w| p.set("ind", format!("<{w}ind {w}left=\"{l}\" {w}firstLine=\"0\"/>")))
        };
        if !ok {
            self.abort(tx);
            return Some(Outcome::Unchanged("formatting failed"));
        }
        Some(self.commit(tx, None, fonts))
    }

    fn list_items(&self, paras: &[ParaId]) -> Vec<(ParaId, (i64, i64))> {
        let num = |&para: &ParaId| Some((para, self.index.para(&self.doc, para)?.style.num.filter(|(id, _)| *id != 0)?));
        paras.iter().filter_map(num).collect()
    }

    pub(super) fn tab_changes_level(&mut self) -> bool {
        if self.list_item_start().is_some() {
            return true;
        }
        if !matches!(self.selection, Selection::Text { .. }) {
            return false;
        }
        let paras = self.selected_paras();
        paras.len() > 1 && self.list_items(&paras).len() == paras.len()
    }

    pub(super) fn list_level(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        let paras = self.selected_paras();
        let items = self.list_items(&paras);
        if items.is_empty() {
            return Outcome::Unchanged("not in a list");
        }
        let mut tx = self.begin("Numbering");
        let mut changed = false;
        for (para, (num, ilvl)) in items {
            let to = if back { ilvl - 1 } else { ilvl + 1 };
            if !(0..=8).contains(&to) {
                continue;
            }
            if !self
                .repara(&mut tx, para, &|p, w| p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{to}\"/><{w}numId {w}val=\"{num}\"/></{w}numPr>")))
            {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
            changed = true;
        }
        if !changed {
            self.abort(tx);
            return Outcome::Unchanged("no level there");
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn toggle_list(&mut self, bullet: bool, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let paras = self.selected_paras();
        let Some(&first) = paras.first() else { return Outcome::Unchanged("no paragraph") };
        let kind = |e: &Self, para: ParaId| -> Option<(bool, (i64, i64))> {
            let p = e.index.para(&e.doc, para)?;
            let (id, ilvl) = p.style.num.filter(|(id, _)| *id != 0)?;
            let r = e.doc.resolver.as_ref()?;
            let level = r.numbering.level(id, ilvl, &r.styles)?;
            Some((level.fmt == "bullet", (id, ilvl)))
        };
        let label = if bullet { "Bullets" } else { "Numbering" };
        if paras.iter().all(|&p| kind(self, p).is_some_and(|(b, _)| b == bullet)) {
            let mut tx = self.begin(label);
            for &para in &paras {
                let Some((_, (id, ilvl))) = kind(self, para) else { continue };
                let lvl = res.numbering.level(id, ilvl, &res.styles);
                let left = lvl.and_then(|l| l.ppr.ind_left).unwrap_or(36.0) + lvl.and_then(|l| l.ppr.ind_first).unwrap_or(-18.0).min(0.0);
                let direct = self.index.para(&self.doc, para).and_then(|p| p.style.direct.0.clone());
                let has_direct_num = direct.as_deref().is_some_and(|d| d.contains("numPr"));
                let ok = self.repara(&mut tx, para, &|p, w| {
                    if has_direct_num {
                        p.remove("numPr");
                    } else {
                        p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"0\"/><{w}numId {w}val=\"0\"/></{w}numPr>"));
                    }
                    if p.get("pStyle").is_some_and(|s| s.contains("\"ListParagraph\"")) {
                        p.remove("pStyle");
                    }
                    p.set("ind", format!("<{w}ind {w}left=\"{}\"/>", (left * 20.0).round() as i64));
                });
                if !ok {
                    self.abort(tx);
                    return Outcome::Unchanged("numbering failed");
                }
            }
            return self.commit(tx, None, fonts);
        }
        let before = self.index.para_slot(first).and_then(|s| {
            let blocks = self.index.blocks(&self.doc, s.container)?;
            match blocks.get(s.block.checked_sub(1)?)? {
                Block::Para(q) => Some(ParaId(q.id)),
                Block::Table(_) => None,
            }
        });
        let mut tx = self.begin(label);
        let (num, ilvl) = match before.and_then(|q| kind(self, q)).filter(|(b, _)| *b == bullet) {
            Some((_, n)) => n,
            None => (self.new_list(&mut tx, bullet), 0),
        };
        let list_style = res.styles.by_id.contains_key("ListParagraph");
        for &para in &paras {
            let ok = self.repara(&mut tx, para, &|p, w| {
                p.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{ilvl}\"/><{w}numId {w}val=\"{num}\"/></{w}numPr>"));
                p.remove("ind");
                if list_style && p.get("pStyle").is_none() {
                    p.set("pStyle", format!("<{w}pStyle {w}val=\"ListParagraph\"/>"));
                }
            });
            if !ok {
                self.abort(tx);
                return Outcome::Unchanged("numbering failed");
            }
        }
        self.commit(tx, None, fonts)
    }

    fn new_list(&mut self, tx: &mut Tx, bullet: bool) -> i64 {
        let Some(res) = self.doc.resolver.clone() else { return 0 };
        let abstract_id = res.numbering.abstracts.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.abstract_id)).max().unwrap_or(-1) + 1;
        let num_id = res.numbering.nums.keys().copied().chain(self.doc.added_lists.iter().map(|l| l.num_id)).max().unwrap_or(0) + 1;
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
        let abstract_xml = format!(
            r#"<w:abstractNum xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:abstractNumId="{abstract_id}"><w:multiLevelType w:val="hybridMultilevel"/>{levels}</w:abstractNum>"#
        );
        let part = format!(
            r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{abstract_xml}<w:num xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:numId="{num_id}"><w:abstractNumId w:val="{abstract_id}"/></w:num></w:numbering>"#
        );
        let parsed = crate::docx::numbering::Numbering::parse(Some(&part), &res.styles.colours);
        let mut r = (*res).clone();
        r.numbering.abstracts.extend(parsed.abstracts);
        r.numbering.nums.extend(parsed.nums);
        let mut added = self.doc.added_lists.clone();
        added.push(crate::docx::AddedList { abstract_id, num_id, abstract_xml });
        if self.step(tx, Step::Resolver { resolver: Some(Rc::new(r)), added }).is_err() {
            return 0;
        }
        num_id
    }

    pub(super) fn renumber(&mut self, tx: &mut Tx) {
        for (para, list) in self.labels_changed(tx.relabel) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let (style, mark) = (p.style.clone(), p.mark.clone());
            let _ = self.step(tx, Step::Para { para, style, mark, list });
        }
    }

    /// The labels that change: those whose text changed, or every one when `all` (a list's definition changed).
    pub(super) fn labels_changed(&self, all: bool) -> Vec<(ParaId, Option<ListLabel>)> {
        let Some(res) = self.doc.resolver.clone() else { return Vec::new() };
        if res.numbering.nums.is_empty() {
            return Vec::new();
        }
        let cx = self.cx();
        let mut counters = Counters::default();
        let mut changes: Vec<(ParaId, Option<ListLabel>)> = Vec::new();
        let mut order: Vec<ParaId> = Vec::new();
        fn walk(blocks: &[Block], out: &mut Vec<ParaId>) {
            for b in blocks {
                match b {
                    Block::Para(p) => out.push(ParaId(p.id)),
                    Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut order));
        for para in order {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let label = match p.style.num {
                Some((id, ilvl)) if id != 0 => {
                    let from_style = !p.style.direct.0.as_deref().is_some_and(|x| x.contains("numId"));
                    if res.label_shows(id, ilvl, from_style, p.style.style_id.as_deref()) {
                        counters.next(&res.numbering, &res.styles, id, ilvl).map(|text| (id, ilvl, text))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let want = match (&label, &p.list) {
                (None, None) => continue,
                (Some((_, _, text)), Some(have)) if *text == have.text && !all => continue,
                (None, Some(_)) => None,
                (Some((id, ilvl, text)), _) => {
                    let Some(level) = res.numbering.level(*id, *ilvl, &res.styles).cloned() else { continue };
                    let region = self.region_of_para(para, &res);
                    let base = res.para_from_xml(p.style.direct.0.as_deref(), region.as_ref(), &cx).base;
                    let mark = p
                        .mark
                        .direct
                        .0
                        .as_deref()
                        .and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el), &res.styles.colours)))
                        .unwrap_or_default();
                    let mut lp = res.run_props(&base, &mark);
                    lp.merge(&level.rpr);
                    let picture = res.numbering.bullet_image(&level);
                    let label = ListLabel {
                        text: text.clone(),
                        run: res.run_style(&lp),
                        jc: level.jc.clone(),
                        suff: level.suff.clone(),
                        picture,
                        legacy: level.legacy,
                    };
                    if p.list.as_ref() == Some(&label) {
                        continue;
                    }
                    Some(label)
                }
            };
            changes.push((para, want));
        }
        changes
    }
}
