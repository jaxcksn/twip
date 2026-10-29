//! The fonts a session lays out with, and the document's families the library can't draw.

use std::collections::{BTreeSet, HashSet};

use super::Session;
use crate::api::event::Event;
use crate::api::fonts::FontLibrary;
use crate::api::state::FontsState;
use crate::api::warning::{Warning, WarningCode};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::layout::ParaId;

/// What the session last saw of its library, and what it reported.
#[derive(Default)]
pub(super) struct Watch {
    generation: u64,
    revision: u64,
    pub(super) state: FontsState,
    /// The families already reported unresolved for this document.
    warned: BTreeSet<String>,
    /// The faces the last check saw: an edit that adds none needs no check.
    faces: HashSet<(String, bool, bool)>,
}

impl Session {
    /// The library this session lays out with (its engine's).
    pub fn fonts(&self) -> &FontLibrary {
        &self.fonts
    }

    /// After the document was replaced: checks its families. Returns `FontsChanged` when the lists moved, and the
    /// warning naming the unresolved families, which the open reports.
    pub(super) fn fonts_opened(&mut self) -> (Vec<Event>, Option<Warning>) {
        let (generation, revision) = (self.fonts.generation(), self.fonts.fonts().revision());
        self.watch = Watch { generation, revision, state: std::mem::take(&mut self.watch.state), ..Watch::default() };
        let (event, warning) = self.check_fonts();
        (event.into_iter().collect(), warning)
    }

    /// Lays out again and reports the families that came or went, when the library changed since the session last
    /// looked. The engine calls it after a change to its library.
    pub(crate) fn sync_fonts(&mut self) {
        let generation = self.fonts.generation();
        if generation == self.watch.generation {
            return;
        }
        self.watch.generation = generation;
        let revision = self.fonts.fonts().revision();
        let relaid = revision != self.watch.revision;
        if relaid {
            self.watch.revision = revision;
            // shortcut: the whole document relays out on a font change; track families per page when this exceeds
            // 200 ms on the benchmark documents.
            self.editor.fonts_changed();
        }
        let (event, warning) = self.check_fonts();
        if relaid || event.is_some() || warning.is_some() {
            self.changed(warning.map(Event::Warning).into_iter().chain(event).collect());
        }
    }

    /// After an edit that touched `paras` (and tables, when `tables`): checks the document again when they name a face
    /// the last check didn't see (a font applied, a paste). Typing in a face the document has costs one paragraph.
    // shortcut: a face the edit removed from the document stays in the lists until the next full check (an open, a
    // library change, an edit that adds a face).
    pub(super) fn edited_fonts(&mut self, paras: impl IntoIterator<Item = ParaId>, tables: bool) -> (Option<Event>, Option<Warning>) {
        let (doc, index) = (self.editor.document(), self.editor.index());
        let mut touched = HashSet::new();
        paras.into_iter().filter_map(|id| index.para(doc, id)).for_each(|p| para_faces(p, &mut touched));
        let fresh = tables || touched.iter().any(|&(f, b, i)| !self.watch.faces.contains(&(f.to_string(), b, i)));
        if fresh { self.check_fonts() } else { (None, None) }
    }

    /// The document's families against the library: the new lists, `FontsChanged` when they moved, and a warning for
    /// the families unresolved for the first time.
    pub(super) fn check_fonts(&mut self) -> (Option<Event>, Option<Warning>) {
        let faces = faces(self.editor.document());
        self.watch.faces = faces.iter().map(|&(f, b, i)| (f.to_string(), b, i)).collect();
        let state = self.fonts.check(faces);
        let fresh: Vec<String> = state.unresolved.iter().filter(|f| !self.watch.warned.contains(*f)).cloned().collect();
        let warning = (!fresh.is_empty()).then(|| Warning {
            code: WarningCode::FontUnresolved,
            message: format!("no font for {}; another family draws it", fresh.join(", ")),
            count: fresh.len() as u32,
            location: None,
        });
        self.watch.warned.extend(fresh);
        let event = (state != self.watch.state).then(|| Event::FontsChanged { pending: state.pending.clone(), unresolved: state.unresolved.clone() });
        self.watch.state = state;
        (event, warning)
    }
}

/// The faces the document's text names: family, bold, italic.
// shortcut: the Latin slots only (ascii, hAnsi); East Asian and complex-script families, symbol fonts and list labels
// aren't requested. Add them when a document in those scripts shows unresolved text.
fn faces(doc: &Document) -> HashSet<(&str, bool, bool)> {
    let mut out = HashSet::new();
    let stories = doc.sections.iter().map(|s| &s.blocks).chain(doc.stories.values()).chain(doc.footnotes.values()).chain(doc.endnotes.values());
    stories.for_each(|blocks| walk(blocks, &mut out));
    out
}

fn walk<'a>(blocks: &'a [Block], out: &mut HashSet<(&'a str, bool, bool)>) {
    for b in blocks {
        match b {
            Block::Para(p) => para_faces(p, out),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
        }
    }
}

/// A paragraph's faces, its text boxes' included, and its mark's: an empty line's height and what typing in it
/// draws with.
fn para_faces<'a>(p: &'a Paragraph, out: &mut HashSet<(&'a str, bool, bool)>) {
    run_faces(&p.mark, out);
    for i in &p.inlines {
        match i {
            Inline::Text(_, run) => run_faces(run, out),
            Inline::Float(f, _) => {
                if let Some(tb) = &f.text_box {
                    walk(&tb.blocks, out);
                }
            }
            _ => {}
        }
    }
}

fn run_faces<'a>(run: &'a crate::docx::Run, out: &mut HashSet<(&'a str, bool, bool)>) {
    for family in [&run.ascii, &run.h_ansi] {
        if !family.is_empty() {
            out.insert((family, run.bold, run.italic));
        }
    }
}
