//! The fonts a session lays out with, and the document's families and characters the library can't draw.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::Session;
use crate::api::event::Event;
use crate::api::fonts::{FontLibrary, GenericFamily};
use crate::api::state::{FontsState, MissingCharacters, MissingFont};
use crate::api::warning::{Warning, WarningCode};
use crate::docx::{Block, Document, Inline, Paragraph};
use crate::editing::index::DocIndex;
use crate::font::Fonts;
use crate::layout::ParaId;

/// The most distinct missing characters reported per family.
// shortcut: 64 per family; report counts by script instead if documents with large unsupported scripts need more.
const CHARACTERS_REPORTED: usize = 64;

/// Missing characters by the family their runs name.
pub(super) type Missing = BTreeMap<String, BTreeSet<char>>;

/// What the session last saw of its library, and what it reported.
#[derive(Default)]
pub(super) struct Watch {
    generation: u64,
    revision: u64,
    pub(super) state: FontsState,
    /// The families already reported missing for this document, lowercase.
    warned: BTreeSet<String>,
    /// The faces the last check saw: an edit that adds none needs no check.
    faces: HashSet<(String, bool, bool)>,
    /// Each paragraph's missing characters, only for paragraphs that have some.
    pub(super) coverage: HashMap<ParaId, Missing>,
}

impl Session {
    /// The library this session lays out with (its engine's).
    pub fn fonts(&self) -> &FontLibrary {
        &self.fonts
    }

    /// After the document was replaced: checks its families and characters. Returns `FontsChanged` when the lists
    /// moved, and the warning naming the missing families, which the open reports.
    pub(super) fn fonts_opened(&mut self) -> (Vec<Event>, Option<Warning>) {
        let (generation, revision) = (self.fonts.generation(), self.fonts.fonts().revision());
        self.watch = Watch { generation, revision, state: std::mem::take(&mut self.watch.state), ..Watch::default() };
        self.cover_all();
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
            self.cover_all();
        }
        let (event, warning) = self.check_fonts();
        if relaid || event.is_some() || warning.is_some() {
            self.changed(warning.map(Event::Warning).into_iter().chain(event).collect());
        }
    }

    /// After an edit that touched `paras` (and tables, when `tables`): checks the edited paragraphs' characters, and
    /// the document's families again when the paragraphs name a face the last check didn't see (a font applied, a
    /// paste). Typing in a face the document has costs one paragraph.
    // shortcut: a face the edit removed from the document stays in the lists until the next full check (an open, a
    // library change, an edit that adds a face).
    pub(super) fn edited_fonts(&mut self, paras: impl IntoIterator<Item = ParaId>, tables: bool) -> (Option<Event>, Option<Warning>) {
        let (doc, index) = (self.editor.document(), self.editor.index());
        let fonts = self.fonts.fonts();
        let mut touched = HashSet::new();
        for id in paras {
            match index.para(doc, id) {
                Some(p) => {
                    para_faces(p, &mut touched);
                    set_coverage(&mut self.watch.coverage, id, para_missing(p, &fonts));
                }
                None => {
                    self.watch.coverage.remove(&id);
                }
            }
        }
        self.watch.coverage.retain(|&id, _| index.para(doc, id).is_some());
        let fresh = tables || touched.iter().any(|&(f, _, b, i)| !self.watch.faces.contains(&(f.to_string(), b, i)));
        drop(fonts);
        if fresh {
            return self.check_fonts();
        }
        let characters = characters(&self.watch.coverage);
        if characters == self.watch.state.characters {
            return (None, None);
        }
        self.watch.state.characters = characters;
        (Some(Event::FontsChanged(self.watch.state.clone())), None)
    }

    /// Rebuilds every paragraph's missing characters.
    pub(super) fn cover_all(&mut self) {
        let (doc, index) = (self.editor.document(), self.editor.index());
        let fonts = self.fonts.fonts();
        let mut coverage = HashMap::new();
        for id in index.para_ids() {
            if let Some(p) = index.para(doc, id) {
                set_coverage(&mut coverage, id, para_missing(p, &fonts));
            }
        }
        drop(fonts);
        self.watch.coverage = coverage;
    }

    /// The document's families and characters against the library: the new lists, `FontsChanged` when they moved,
    /// and a warning for the families missing for the first time.
    pub(super) fn check_fonts(&mut self) -> (Option<Event>, Option<Warning>) {
        let faces = faces(self.editor.document());
        self.watch.faces = faces.iter().map(|&(f, _, b, i)| (f.to_string(), b, i)).collect();
        let mut state = self.fonts.check(faces, &self.editor.document().font_descriptions);
        state.characters = characters(&self.watch.coverage);
        let fresh: Vec<&MissingFont> = state.missing.iter().filter(|m| !self.watch.warned.contains(&m.family.to_lowercase())).collect();
        let warning = (!fresh.is_empty()).then(|| Warning {
            code: WarningCode::FontUnresolved,
            message: format!("no font for {}", fresh.iter().map(|m| describe(m)).collect::<Vec<_>>().join("; ")),
            count: fresh.len() as u32,
            location: None,
        });
        let warned: Vec<String> = fresh.iter().map(|m| m.family.to_lowercase()).collect();
        self.watch.warned.extend(warned);
        let event = (state != self.watch.state).then(|| Event::FontsChanged(state.clone()));
        self.watch.state = state;
        (event, warning)
    }
}

/// "Garamond (bold); Stand-in Sans draws it".
pub(crate) fn describe(m: &MissingFont) -> String {
    let style = match (m.bold, m.italic) {
        (false, false) => "",
        (true, false) => " (bold)",
        (false, true) => " (italic)",
        (true, true) => " (bold italic)",
    };
    match &m.drawn_with {
        Some(drawn) => format!("{}{style}; {drawn} draws it", m.family),
        None => format!("{}{style}; nothing draws it", m.family),
    }
}

fn set_coverage(coverage: &mut HashMap<ParaId, Missing>, id: ParaId, missing: Missing) {
    if missing.is_empty() {
        coverage.remove(&id);
    } else {
        coverage.insert(id, missing);
    }
}

/// The paragraphs' missing characters, merged per family.
pub(super) fn characters(coverage: &HashMap<ParaId, Missing>) -> Vec<MissingCharacters> {
    let mut merged: BTreeMap<String, (String, BTreeSet<char>)> = BTreeMap::new();
    for missing in coverage.values() {
        for (family, chars) in missing {
            merged.entry(family.to_lowercase()).or_insert_with(|| (family.clone(), BTreeSet::new())).1.extend(chars);
        }
    }
    merged
        .into_values()
        .map(|(family, chars)| MissingCharacters {
            family,
            characters: chars.iter().take(CHARACTERS_REPORTED).collect(),
            truncated: chars.len() > CHARACTERS_REPORTED,
        })
        .collect()
}

/// A paragraph's characters that neither their run's face, the character-fallback families nor the symbol fallback
/// has, by the family the run names.
fn para_missing(p: &Paragraph, fonts: &Fonts) -> Missing {
    let mut out = Missing::new();
    // With no faces at all, the missing families already say why nothing draws.
    if fonts.face_count() == 0 {
        return out;
    }
    for i in &p.inlines {
        let Inline::Text(text, run) = i else { continue };
        for c in text.chars() {
            if c.is_whitespace() || c.is_control() || c == '\u{AD}' || c == '\u{2011}' {
                continue;
            }
            let family = crate::layout::text::family_for(c, run);
            let Some(face) = fonts.face(family, run.category(family), run.bold, run.italic) else { continue };
            let drawn = match crate::layout::text::symbol_unicode(family, c) {
                mapped if mapped != c && !face.family.eq_ignore_ascii_case(family) => mapped,
                _ => c,
            };
            if !face.has_glyph(drawn) && fonts.fallback(drawn, run.bold, run.italic).is_none() {
                out.entry(family.to_string()).or_default().insert(drawn);
            }
        }
    }
    out
}

/// As [`document_fonts`], for a document no session holds (a conversion).
pub(crate) fn unopened_document_fonts(doc: &Document, fonts: &Fonts) -> FontsState {
    document_fonts(doc, &DocIndex::build(doc), fonts)
}

/// The document's missing families and characters in `fonts`, with nothing pending: what an export to PDF reports.
pub(crate) fn document_fonts(doc: &Document, index: &DocIndex, fonts: &Fonts) -> FontsState {
    let mut coverage = HashMap::new();
    for id in index.para_ids() {
        if let Some(p) = index.para(doc, id) {
            set_coverage(&mut coverage, id, para_missing(p, fonts));
        }
    }
    let mut state = crate::api::fonts::report(fonts, faces(doc));
    state.characters = characters(&coverage);
    state
}

/// The faces the document's text names: family, its generic category, bold, italic. The East Asian slot counts for
/// runs with characters that use it.
// shortcut: complex-script families, symbol characters and list labels aren't requested. Add them when a document in
// those scripts shows missing text.
fn faces(doc: &Document) -> HashSet<Face<'_>> {
    let mut out = HashSet::new();
    let stories = doc.sections.iter().map(|s| &s.blocks).chain(doc.stories.values()).chain(doc.footnotes.values()).chain(doc.endnotes.values());
    stories.for_each(|blocks| walk(blocks, &mut out));
    out
}

/// A family the text names, its generic category, bold and italic.
type Face<'a> = (&'a str, Option<GenericFamily>, bool, bool);

fn walk<'a>(blocks: &'a [Block], out: &mut HashSet<Face<'a>>) {
    for b in blocks {
        match b {
            Block::Para(p) => para_faces(p, out),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
        }
    }
}

/// A paragraph's faces, its text boxes' included, and its mark's: an empty line's height and what typing in it
/// draws with.
fn para_faces<'a>(p: &'a Paragraph, out: &mut HashSet<Face<'a>>) {
    run_faces(&p.mark, "", out);
    for i in &p.inlines {
        match i {
            Inline::Text(text, run) => run_faces(run, text, out),
            i if i.story().is_some() => {
                if let Some((_, tb)) = i.story() {
                    walk(&tb.blocks, out);
                }
            }
            _ => {}
        }
    }
}

fn run_faces<'a>(run: &'a crate::docx::Run, text: &str, out: &mut HashSet<Face<'a>>) {
    for family in [&run.ascii, &run.h_ansi] {
        if !family.is_empty() {
            out.insert((family, run.category(family), run.bold, run.italic));
        }
    }
    if !run.east_asia.is_empty() && text.chars().any(|c| std::ptr::eq(crate::layout::text::family_for(c, run), run.east_asia.as_str())) {
        out.insert((&run.east_asia, run.category(&run.east_asia), run.bold, run.italic));
    }
}
