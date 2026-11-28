//! Font rewrite

use crate::api::command::{CommandResult, ReplaceFont, ReplaceScope};
use crate::docx::char_props::RunXml;
use crate::docx::font_table::FontRename;
use crate::docx::{Inline, RunStyle};
use crate::font::Fonts;
use crate::format::{CharEdit, CharProps, FontRef, Prop};
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::format::with_run;
use super::step::Step;
use crate::docx::run_xml;

/// Rewrites `from` to `to` in `p`, a run's authored properties: each script class whose own font names `from`, and,
/// when `inherited`, each whose family is `from` through a style or the theme (it gets `to` as its own, in place of its
/// theme font). Returns whether any changed.
fn patch(e: &mut CharEdit, run: &RunStyle, r: &ReplaceFont, inherited: bool) -> bool {
    use crate::format::ScriptClass as S;
    let mut changed = false;
    for (slot, resolved) in [(S::Latin, &run.ascii), (S::LatinExtended, &run.h_ansi), (S::EastAsian, &run.east_asia), (S::Complex, &run.cs)] {
        let f = &mut e.fonts;
        let own = match slot {
            S::Latin => &mut f.latin,
            S::LatinExtended => &mut f.latin_extended,
            S::EastAsian => &mut f.east_asian,
            S::Complex => &mut f.complex,
        };
        let named = matches!(own, Some(FontRef::Family(n)) if n.eq_ignore_ascii_case(&r.from));
        let through = inherited && !named && resolved.eq_ignore_ascii_case(&r.from);
        if named || through {
            *own = Some(FontRef::Family(r.to.as_str().into()));
            e.part(Prop::Font(slot));
            changed = true;
        }
    }
    changed
}

/// `run`'s authored properties, to edit.
fn authored(run: &RunStyle) -> CharEdit {
    CharEdit::new(run.direct.props().map_or_else(CharProps::default, |p| (**p).clone()))
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
        if self.refuse_locked_content() {
            return Outcome::Unchanged("locked");
        }
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
                    changed += u32::from(patch(&mut authored(run), run, r, true));
                }
            }
        }
        if changed == 0 {
            return Outcome::Unchanged("nothing matched");
        }
        let outcome = self.format_runs_typed(
            "Replace Font",
            &|p, run| {
                if styled(run, r) {
                    patch(p, run, r, true);
                }
            },
            fonts,
        );
        self.made = Some(CommandResult::FontReplaced { changed, skipped: 0 });
        outcome
    }

    fn replace_font_in_document(&mut self, r: &ReplaceFont, filtered: bool, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.definitions.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let (mut changed, mut skipped) = (0u32, 0u32);
        let mut steps: Vec<Step> = Vec::new();
        let locked = |c: &crate::docx::ControlId| self.doc.controls.get(c).is_some_and(|k| !k.props.lock.editable());
        let paras: Vec<ParaId> = self.index.para_ids().collect();
        for para in paras {
            let (Some(again), Some(p)) = (self.para_again(para, None, &res, &cx), self.index.para(&self.doc, para)) else { continue };
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
                let mut props = authored(&p.mark);
                if patch(&mut props, &p.mark, r, filtered) && tally(1, para_locked) {
                    let direct = p.mark.direct.edit(props);
                    let mark = run_xml::again(&res, again.mark_base(), &p.mark, direct);
                    steps.push(Step::Para { para, style: p.style.clone(), mark, list: p.list.clone() });
                }
            }
            // Inlines without units (anchored objects, comment references) change in place, before the others.
            let (first, mut zero): (usize, Option<Vec<Inline>>) = (steps.len(), None);
            let mut open = Vec::new();
            let mut at = 0u32;
            for (k, i) in p.inlines.iter().enumerate() {
                match i {
                    Inline::ControlStart(c) => open.push(*c),
                    Inline::ControlEnd(c) => open.retain(|x| x != c),
                    _ => {}
                }
                let units = i.units() as u32;
                let range = Unit(at)..Unit(at + units);
                let base = again.base(at);
                at += units;
                let Some(run) = i.run() else { continue };
                let own = styled(run, r);
                let restyled = own
                    .then(|| {
                        let mut props = authored(run);
                        patch(&mut props, run, r, filtered).then(|| run_xml::again(&res, base, run, run.direct.edit(props)))
                    })
                    .flatten();
                let symbol = matches!(i, Inline::Sym { font, .. } if own && font.eq_ignore_ascii_case(&r.from));
                let ruby = match i {
                    Inline::Ruby(ruby, _) => {
                        let edit = super::format::ruby_edit(|p, run| styled(run, r) && patch(p, run, r, filtered));
                        super::format::ruby_again(ruby, base, Some((&edit, true)), &res, &cx)
                    }
                    _ => None,
                };
                if restyled.is_none() && !symbol && ruby.is_none() {
                    continue;
                }
                let mut with = with_run(i, restyled.as_ref().unwrap_or(run));
                match (&mut with, ruby) {
                    (Inline::Sym { font, .. }, _) if symbol => *font = r.to.clone(),
                    (Inline::Ruby(old, _), Some(n)) => *old = n,
                    _ => {}
                }
                let locked_here = para_locked || open.iter().any(locked);
                if tally(1, locked_here) {
                    match units {
                        0 => zero.get_or_insert_with(|| p.inlines.clone())[k] = with,
                        _ => steps.push(Step::Text { para, range, with: vec![with], edges: Default::default() }),
                    }
                }
            }
            if let Some(inlines) = zero {
                steps.insert(first, Step::Restyle { para, inlines });
            }
        }
        // The tables below the text: styles, document defaults, numbering, the theme and the font table.
        if !filtered {
            let rename = FontRename { from: r.from.clone(), to: r.to.clone() };
            let mut renamed = (*res).clone();
            let mut n = renamed.rename_fonts(&rename);
            n += u32::from(self.doc.font_descriptions.contains_key(&r.from.to_lowercase()));
            if n > 0 {
                changed += n;
                let mut renames = self.docx.font_renames.clone();
                renames.push(rename);
                steps.push(Step::Fonts { definitions: Some(std::rc::Rc::new(renamed)), renames });
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
