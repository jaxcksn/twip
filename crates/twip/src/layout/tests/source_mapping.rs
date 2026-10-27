//! Drawn characters map back to the document: every glyph character with a source is the
//! document's character at that caret unit, and the semantic traversal shows each text character of a paragraph
//! once, in order.

use std::collections::HashMap;

use crate::docx::{Block, Document, Inline, Paragraph};
use crate::layout::ids::{ParaId, Unit};
use crate::layout::traverse::semantic_chars;
use crate::layout::{Snapshot, layout};

use super::{fonts, gate_set};

/// Every paragraph of the document by id: sections, tables, stories, notes, separators and text boxes, with whether it
/// is in a text box (whose overflow FLOAT-10 clips).
fn paragraphs(d: &Document) -> HashMap<ParaId, (&Paragraph, bool)> {
    fn walk<'a>(blocks: &'a [Block], in_box: bool, out: &mut HashMap<ParaId, (&'a Paragraph, bool)>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    out.insert(ParaId(p.id), (p, in_box));
                    for i in &p.inlines {
                        if let Inline::Float(f, _) = i
                            && let Some(tb) = &f.text_box
                        {
                            walk(&tb.blocks, true, out);
                        }
                    }
                }
                Block::Table(t) => {
                    for r in &t.rows {
                        for c in &r.cells {
                            walk(&c.blocks, in_box, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = HashMap::new();
    for s in &d.sections {
        walk(&s.blocks, false, &mut out);
    }
    for blocks in d.stories.values().chain(d.footnotes.values()).chain(d.endnotes.values()).chain(d.separators.values()) {
        walk(blocks, false, &mut out);
    }
    out
}

/// A paragraph's text characters by caret unit (one unit per text character, one per other drawn
/// inline, none for bookmarks and anchored objects), with whether each is hidden. Atoms have no entry.
fn text_units(p: &Paragraph) -> HashMap<u32, (char, bool)> {
    let mut out = HashMap::new();
    let mut u = 0u32;
    // A field's code and an evaluated field's result aren't drawn: as hidden text, not expected.
    let shown = crate::docx::fields::layout_shown(&p.inlines);
    for (i, shown) in p.inlines.iter().zip(shown) {
        if let Inline::Text(s, run) = i {
            for c in s.chars() {
                out.insert(u, (c, run.hidden || shown == crate::docx::fields::Shown::Not));
                u += 1;
            }
        } else {
            u += i.units() as u32;
        }
    }
    out
}

/// Whether drawn character `drawn` shows document character `c` (caps and small caps draw it in capitals; symbol
/// fonts map private-use characters to Unicode; soft and non-breaking hyphens draw as '-').
fn shows(drawn: char, c: char) -> bool {
    drawn == c
        || drawn.to_uppercase().eq(c.to_uppercase())
        || (0xF020..=0xF0FF).contains(&(c as u32))
        || (matches!(c, '\u{AD}' | '\u{2011}') && drawn == '-')
}

fn check(name: &str, d: &Document, s: &Snapshot) -> Vec<String> {
    let paras = paragraphs(d);
    let mut problems = Vec::new();
    // Round trip: every character with a source is the document's character at that unit.
    for (pi, page) in s.pages().iter().enumerate() {
        for it in crate::layout::traverse::paint_items(page) {
            let crate::layout::fragment::Item::Glyphs(r) = it else { continue };
            let Some(para) = r.para else { continue };
            let Some(&(p, _)) = paras.get(&para) else {
                problems.push(format!("{name} page {}: glyphs of unknown paragraph {para:?}", pi + 1));
                continue;
            };
            let units = text_units(p);
            for &(c, _, u) in r.chars.iter() {
                let Some(Unit(u)) = u else { continue };
                if let Some(&(want, _)) = units.get(&u)
                    && !shows(c, want)
                {
                    problems.push(format!("{name} page {}: paragraph {para:?} unit {u} draws {c:?}, the document has {want:?}", pi + 1));
                }
            }
        }
    }
    // Semantic: each text unit once, in order, all the paragraph's shown text.
    let mut seen: HashMap<ParaId, Vec<(u32, char)>> = HashMap::new();
    for page in s.pages() {
        let mut chars = Vec::new();
        semantic_chars(page, &mut chars);
        for (para, Unit(u), c) in chars {
            seen.entry(para).or_default().push((u, c));
        }
    }
    let excluded: Vec<(ParaId, std::ops::Range<Unit>)> = s.excluded().iter().map(|(_, p, r)| (*p, r.clone())).collect();
    for (para, mut got) in seen {
        let Some(&(p, in_box)) = paras.get(&para) else { continue };
        let units = text_units(p);
        got.retain(|(u, _)| units.contains_key(u));
        got.sort_by_key(|x| x.0);
        if let Some(w) = got.windows(2).find(|w| w[0].0 == w[1].0) {
            problems.push(format!("{name}: paragraph {para:?} unit {} shown twice", w[0].0));
            continue;
        }
        let excluded_here = |u: u32| excluded.iter().any(|(q, r)| *q == para && r.contains(&Unit(u)));
        let mut want: Vec<u32> = units.iter().filter(|(u, (_, hidden))| !hidden && !excluded_here(**u)).map(|(u, _)| *u).collect();
        want.sort();
        let got_units: Vec<u32> = got.iter().map(|x| x.0).collect();
        // Spaces that end a line may be left undrawn (hanging spaces, BREAK-2), soft hyphens show only where a line
        // breaks, and a text box clips its overflow (FLOAT-10); every other text character is shown.
        if in_box {
            continue;
        }
        let missing: Vec<u32> =
            want.iter().copied().filter(|u| got_units.binary_search(u).is_err() && !units[u].0.is_whitespace() && units[u].0 != '\u{AD}').collect();
        if let Some(u) = missing.first() {
            problems.push(format!("{name}: paragraph {para:?} doesn't show unit {u} ({:?}) ({} missing)", units[u].0, missing.len()));
        }
    }
    problems
}

fn run(every: usize) {
    let fonts = fonts();
    let mut problems = Vec::new();
    let docs = gate_set(crate::features::ALL);
    if every == 1 && !docs.is_empty() {
        assert!(docs.len() > 1000, "the whole baseline set: {} documents", docs.len());
    }
    for (k, (path, d)) in docs.into_iter().enumerate() {
        if k % every != 0 {
            continue;
        }
        let s = layout(&d, &fonts);
        problems.extend(check(&path.display().to_string(), &d, &s));
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
}

/// Every 25th reference dump document (the whole set: `source_mapping_all`).
#[test]
fn source_mapping_round_trip_and_semantic_sample() {
    run(25);
}

/// Every reference dump document (run with `--ignored`).
#[test]
#[ignore]
fn source_mapping_all() {
    run(1);
}
