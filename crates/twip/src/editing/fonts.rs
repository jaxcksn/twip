//! Font rewrite

use crate::api::command::{CommandResult, ReplaceFont, ReplaceScope};
use crate::docx::font_table::{FontRename, rename_tables};
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Inline, RunStyle};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::format::with_run;
use super::step::Step;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Family slots of `w:rFonts`: the name attribute, its theme attribute, and the resolved family.
fn slots(run: &RunStyle) -> [(&'static str, &'static str, &str); 4] {
    [
        ("ascii", "asciiTheme", &run.ascii),
        ("hAnsi", "hAnsiTheme", &run.h_ansi),
        ("eastAsia", "eastAsiaTheme", &run.east_asia),
        ("cs", "cstheme", &run.cs),
    ]
}

/// Rewrites `from` to `to` in `p`, a run's direct `w:rPr`: each slot that names `from`, and, when `inherited`, each
/// slot whose family is `from` through a style or the theme (it gets `to` as its own, without its theme attribute).
/// Returns whether any slot changed.
fn patch(p: &mut Props, run: &RunStyle, r: &ReplaceFont, inherited: bool, cx: &Cx) -> bool {
    let written: [(Option<String>, bool); 4] = p
        .get("rFonts")
        .and_then(|x| {
            cx.parse(x, |_, el| slots(run).map(|(name, theme, _)| (el.attribute((W, name)).map(str::to_string), el.attribute((W, theme)).is_some())))
        })
        .unwrap_or_default();
    let mut set: Vec<(&str, Option<String>)> = Vec::new();
    for ((name, theme, resolved), (own, themed)) in slots(run).into_iter().zip(written) {
        let named = !themed && own.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&r.from));
        let through = inherited && !named && resolved.eq_ignore_ascii_case(&r.from);
        if named || through {
            set.push((name, Some(r.to.clone())));
            if themed {
                set.push((theme, None));
            }
        }
    }
    if !set.is_empty() {
        p.attrs("rFonts", &set, cx);
    }
    !set.is_empty()
}

/// The run's style matches the rewrite's bold and italic filters.
fn styled(run: &RunStyle, r: &ReplaceFont) -> bool {
    r.bold.is_none_or(|b| b == run.bold) && r.italic.is_none_or(|i| i == run.italic)
}

impl Editor {
    pub(super) fn replace_font(&mut self, r: &ReplaceFont, fonts: &Fonts) -> Outcome {
        let filtered = r.bold.is_some() || r.italic.is_some();
        match r.scope {
            ReplaceScope::Selection => self.replace_font_in_selection(r, fonts),
            ReplaceScope::Document => self.replace_font_in_document(r, filtered, fonts),
        }
    }

    fn replace_font_in_selection(&mut self, r: &ReplaceFont, fonts: &Fonts) -> Outcome {
        self.made = Some(CommandResult::FontReplaced { changed: 0, skipped: 0 });
        if self.selection.is_collapsed() {
            return Outcome::Unchanged("no selection");
        }
        if self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let cx = self.cx();
        // What the rewrite would change, before a transaction exists: nothing to change leaves no undo step.
        let mut changed = 0;
        for (para, a, b) in self.selected_ranges().into_iter().filter(|x| x.1 < x.2) {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let mut at = 0u32;
            for i in &p.inlines {
                let n = i.units() as u32;
                let overlaps = n > 0 && at < b.0 && at + n > a.0;
                at += n;
                if let Some(run) = i.run().filter(|run| overlaps && styled(run, r)) {
                    changed += u32::from(patch(&mut Props::parse(run.direct.0.as_deref(), "rPr", &cx), run, r, true, &cx));
                }
            }
        }
        if changed == 0 {
            return Outcome::Unchanged("nothing matched");
        }
        let outcome = self.format_runs(
            "Replace Font",
            &|p, _, run| {
                if styled(run, r) {
                    patch(p, run, r, true, &cx);
                }
            },
            fonts,
        );
        self.made = Some(CommandResult::FontReplaced { changed, skipped: 0 });
        outcome
    }

    fn replace_font_in_document(&mut self, r: &ReplaceFont, filtered: bool, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let (mut changed, mut skipped) = (0u32, 0u32);
        let mut steps: Vec<Step> = Vec::new();
        let locked = |c: &crate::docx::ControlId| self.doc.controls.get(c).is_some_and(|k| !k.props.lock.editable());
        let paras: Vec<ParaId> = self.index.para_ids().collect();
        for para in paras {
            let (Some(base), Some(p)) = (self.run_base(para, &res, &cx), self.index.para(&self.doc, para)) else { continue };
            let para_locked = p.controls.iter().any(locked);
            let mut tally = |n: u32, locked_here: bool| {
                if locked_here {
                    skipped += n;
                    false
                } else {
                    changed += n;
                    n > 0
                }
            };
            if styled(&p.mark, r) {
                let mut props = Props::parse(p.mark.direct.0.as_deref(), "rPr", &cx);
                if patch(&mut props, &p.mark, r, filtered, &cx) && tally(1, para_locked) {
                    let mark = res.run_from_xml(&base, props.xml().as_deref(), &cx);
                    steps.push(Step::Para { para, style: p.style.clone(), mark, list: p.list.clone() });
                }
            }
            let mut open = Vec::new();
            let mut at = 0u32;
            for i in &p.inlines {
                match i {
                    Inline::ControlStart(c) => open.push(*c),
                    Inline::ControlEnd(c) => open.retain(|x| x != c),
                    _ => {}
                }
                let units = i.units() as u32;
                let range = Unit(at)..Unit(at + units);
                at += units;
                let Some(run) = i.run().filter(|run| styled(run, r)) else { continue };
                let mut props = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
                let patched = patch(&mut props, run, r, filtered, &cx);
                let new_run = if patched { res.run_from_xml(&base, props.xml().as_deref(), &cx) } else { run.clone() };
                let mut renamed = false;
                let mut with = with_run(i, &new_run);
                if let Inline::Sym { font, .. } = &mut with
                    && font.eq_ignore_ascii_case(&r.from)
                {
                    *font = r.to.clone();
                    renamed = true;
                }
                let locked_here = para_locked || open.iter().any(locked);
                if (patched || renamed) && tally(1, locked_here) {
                    steps.push(Step::Text { para, range, with: vec![with], edges: Default::default() });
                }
            }
        }
        // The tables below the text: styles, document defaults, numbering, the theme and the font table.
        if !filtered {
            let rename = FontRename { from: r.from.clone(), to: r.to.clone() };
            let mut renamed = (*res).clone();
            let mut n = rename_tables(&mut renamed, &rename);
            n += u32::from(self.doc.font_descriptions.contains_key(&r.from.to_lowercase()));
            if n > 0 {
                changed += n;
                let mut renames = self.doc.font_renames.clone();
                renames.push(rename);
                steps.push(Step::Fonts { resolver: Some(std::rc::Rc::new(renamed)), renames });
            }
        }
        self.made = Some(CommandResult::FontReplaced { changed, skipped });
        if steps.is_empty() {
            return Outcome::Unchanged("nothing matched");
        }
        let tables = steps.iter().any(|s| matches!(s, Step::Fonts { .. }));
        let mut tx = self.begin("Replace Font");
        for step in steps {
            if self.step(&mut tx, step).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("replacing the font failed");
            }
        }
        if tables {
            tx.stale_paras.extend(self.index.para_ids());
        }
        self.commit(tx, None, fonts)
    }
}
