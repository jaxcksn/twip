//! Note numbering

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::docx::model::*;
use crate::layout::ids::ParaId;
use crate::layout::node::{collect_refs, mark_key};
use crate::layout::record::PageRecord;

#[derive(Clone, Default)]
pub struct JobNotes {
    pub numbers: HashMap<(NoteKind, i64), String>,
    pub(super) endnotes: Vec<i64>,
    /// The section each endnote follows (by `endnotes`' order).
    pub(super) ends: Vec<usize>,
    pub(super) separator: Vec<Block>,
    /// The automatic notes numbered by page, in reference order, with their section's start and format.
    paged: Vec<(NoteKind, i64, i64, String)>,
    /// Compat 15: the notes numbered by their page's note keys ([`JobNotes::renumbered`]).
    keyed: Vec<Keyed>,
    /// The footnotes and endnotes as laid out, when a note's last mark is hidden ([`joined_notes`]).
    joined: [Option<HashMap<i64, Vec<Block>>>; 2],
}

/// A kind's notes are one story in reference order: a note whose last paragraph mark is hidden joins the next note's
/// first paragraph. Footnotes draw it in the earlier note and the next note again in full; endnotes once, the next
/// note going on from it. `None` when no note's last mark is hidden.
fn joined_notes(
    notes: &HashMap<i64, Vec<Block>>,
    order: &[i64],
    review: &crate::docx::review::ReviewView,
    once: bool,
) -> Option<HashMap<i64, Vec<Block>>> {
    let hidden = |b: Option<&Vec<Block>>| matches!(b.and_then(|b| b.last()), Some(Block::Para(p)) if review.hides(&p.mark.rev));
    if !order.iter().any(|id| hidden(notes.get(id))) {
        return None;
    }
    let mut out = notes.clone();
    // The note the last one's last paragraph is in now: an endnote whose one paragraph went on in the note before it
    // goes on there, and so on down a run of them.
    let mut holder: Option<i64> = None;
    for w in order.windows(2) {
        let at = match holder {
            Some(h) if out.get(&w[0]).is_none_or(Vec::is_empty) => h,
            _ => w[0],
        };
        holder = None;
        if !hidden(out.get(&at)) {
            continue;
        }
        let Some(first @ Block::Para(_)) = out.get(&w[1]).and_then(|b| b.first()).cloned() else { continue };
        out.entry(at).or_default().push(first);
        if once {
            out.entry(w[1]).or_default().remove(0);
            holder = Some(at);
        }
    }
    Some(out)
}

/// Compat 15, a note restarting each page: its number is the rank of its key among the keys used on its page, in the
/// footnote number format. The key is a footnote story's start: a footnote's own, or for an endnote's mark the
/// footnote story holding the endnote story's start read as a footnote position.
#[derive(Clone)]
struct Keyed {
    kind: NoteKind,
    id: i64,
    key: i64,
    fmt: String,
    /// An endnote mark: its note's first paragraph (the page it is drawn on).
    mark: Option<ParaId>,
}

/// Each note's story start in its kind's stories laid end to end in reference order, and the end of the last one:
/// a story is its paragraphs' units, one more per paragraph mark.
fn story_starts(notes: &HashMap<i64, Vec<Block>>, ids: impl Iterator<Item = i64>) -> (Vec<(i64, i64)>, i64) {
    fn len(blocks: &[Block]) -> i64 {
        blocks
            .iter()
            .map(|b| match b {
                Block::Para(p) => p.inlines.iter().map(Inline::units).sum::<usize>() as i64 + 1,
                // ponytail: a table counts its cells' paragraphs, not its cell and row marks.
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).map(|c| len(&c.blocks)).sum(),
            })
            .sum()
    }
    let mut at = 0;
    let starts = ids
        .map(|id| {
            let start = at;
            at += notes.get(&id).map_or(0, |b| len(b));
            (id, start)
        })
        .collect();
    (starts, at)
}

impl JobNotes {
    pub fn new(doc: &Document) -> JobNotes {
        let mut refs = Vec::new();
        let mut seen = HashSet::new();
        let mut numbers = HashMap::new();
        // Automatic notes counted per kind (foot, end) since the restart origin.
        let mut count = [0, 0];
        let mut ends = Vec::new();
        let mut paged = Vec::new();
        let mut keyed = Vec::new();
        let nsec = doc.sections.len();
        // Each section's endnotes follow it, or the next one without `w:noEndnote`; the last one when none is.
        let mut end_of = vec![nsec.saturating_sub(1); nsec];
        for e in (0..nsec).rev() {
            if !doc.sections[e].no_endnote {
                end_of[e] = e;
            } else if e + 1 < nsec {
                end_of[e] = end_of[e + 1];
            }
        }
        let generic = doc.settings.compat() == crate::docx::Compat::Generic;
        let (mut all, mut all_seen) = (Vec::new(), HashSet::new());
        for s in &doc.sections {
            collect_refs(&s.blocks, &mut all, &mut all_seen);
        }
        let of = |k: NoteKind| all.iter().filter(move |r| r.0 == k).map(|r| r.1);
        let (foot_starts, foot_end) = story_starts(&doc.footnotes, of(NoteKind::Foot));
        let (end_starts, _) = story_starts(&doc.endnotes, of(NoteKind::End));
        for (si, s) in doc.sections.iter().enumerate() {
            let from = refs.len();
            collect_refs(&s.blocks, &mut refs, &mut seen);
            for (k, p) in s.notes.iter().enumerate() {
                if p.restart == NoteRestart::EachSect {
                    count[k] = 0;
                }
            }
            for &(kind, id, custom) in &refs[from..] {
                let k = usize::from(kind == NoteKind::End);
                let p = &s.notes[k];
                let fmt = p.fmt.as_deref().unwrap_or(if k == 0 { "decimal" } else { "lowerRoman" });
                let mut number = crate::docx::numbering::format_number(p.start + count[k], fmt);
                if p.restart == NoteRestart::EachPage && !custom {
                    if generic {
                        // Keys are looked up among the footnotes: an endnote's reference finds none and shows 8, as
                        // does a mark that resolves to none.
                        let foot_fmt = s.notes[0].fmt.as_deref().unwrap_or("decimal");
                        let start_of = |v: &[(i64, i64)], id: i64| v.iter().find(|e| e.0 == id).map(|e| e.1);
                        let key = match kind {
                            NoteKind::Foot => start_of(&foot_starts, id),
                            NoteKind::End => start_of(&end_starts, id)
                                .filter(|&cp| cp < foot_end)
                                .and_then(|cp| foot_starts.iter().rev().find(|e| e.1 <= cp).map(|e| e.1)),
                        };
                        if kind == NoteKind::End {
                            number = crate::docx::numbering::format_number(8, foot_fmt);
                        }
                        let first_para = |b: &Vec<Block>| b.iter().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
                        let mark = if kind == NoteKind::End { doc.endnotes.get(&id).and_then(first_para) } else { None };
                        match key {
                            Some(key) if kind == NoteKind::Foot || mark.is_some() => {
                                keyed.push(Keyed { kind, id, key, fmt: foot_fmt.to_string(), mark });
                            }
                            _ if kind == NoteKind::End => {
                                numbers.insert(mark_key(kind, id), crate::docx::numbering::format_number(8, foot_fmt));
                            }
                            _ => {}
                        }
                    } else {
                        paged.push((kind, id, p.start, fmt.to_string()));
                    }
                }
                numbers.insert((kind, id), number);
                count[k] += i64::from(!custom);
                if kind == NoteKind::End {
                    // After its section, or the next one without `w:noEndnote`; all at the end with docEnd.
                    ends.push(if doc.settings.endnotes_sect_end { end_of[si] } else { nsec - 1 });
                }
            }
        }
        let endnotes: Vec<i64> = refs.iter().filter(|r| r.0 == NoteKind::End).map(|r| r.1).collect();
        let mut separator = Vec::new();
        if !endnotes.is_empty() {
            separator = doc.separators.get("endnote:separator").cloned().unwrap_or_default();
            for b in &mut separator {
                if let Block::Para(p) = b {
                    let mut st = (*p.style).clone();
                    st.keep_next = true;
                    p.style = Rc::new(st);
                }
            }
        }
        let foot: Vec<i64> = of(NoteKind::Foot).collect();
        let joined = [joined_notes(&doc.footnotes, &foot, &doc.review, false), joined_notes(&doc.endnotes, &endnotes, &doc.review, true)];
        JobNotes { numbers, endnotes, ends, separator, paged, keyed, joined }
    }

    /// The footnotes (`k` 0) or endnotes as laid out.
    pub(super) fn stories<'a>(&'a self, doc: &'a Document, k: usize) -> &'a HashMap<i64, Vec<Block>> {
        self.joined[k].as_ref().unwrap_or(if k == 0 { &doc.footnotes } else { &doc.endnotes })
    }
}

impl JobNotes {
    /// The numbers of the notes restarting each page, counted per kind on the pages their references landed on, where
    /// they differ from the current ones.
    pub(super) fn renumbered(&self, pages: &[PageRecord]) -> Option<HashMap<(NoteKind, i64), String>> {
        if self.paged.is_empty() && self.keyed.is_empty() {
            return None;
        }
        // A footnote's first part is drawn on its reference's page; an endnote's reference is shown there.
        let page_of = |kind, id| pages.iter().position(|pg| pg.effects.notes_shown.iter().any(|n| (n.0, n.1) == (kind, id)));
        let mut count: HashMap<(NoteKind, usize), i64> = HashMap::new();
        let mut changed = HashMap::new();
        for &(kind, id, start, ref fmt) in &self.paged {
            let Some(pi) = page_of(kind, id) else { continue };
            let c = count.entry((kind, pi)).or_insert(0);
            let n = crate::docx::numbering::format_number(start + *c, fmt);
            *c += 1;
            if self.numbers.get(&(kind, id)) != Some(&n) {
                changed.insert((kind, id), n);
            }
        }
        // Compat 15: the rank of each note's key among the keys its page uses.
        let on: Vec<Option<usize>> = self
            .keyed
            .iter()
            .map(|k| match k.mark {
                Some(p) => pages.iter().position(|pg| pg.effects.paras.contains(&p)),
                None => page_of(k.kind, k.id),
            })
            .collect();
        for (k, pi) in self.keyed.iter().zip(&on) {
            let Some(pi) = pi else { continue };
            let mut keys: Vec<i64> = self.keyed.iter().zip(&on).filter(|(_, p)| **p == Some(*pi)).map(|(k, _)| k.key).collect();
            keys.sort_unstable();
            keys.dedup();
            let n = crate::docx::numbering::format_number(keys.partition_point(|&x| x < k.key) as i64 + 1, &k.fmt);
            let at = if k.mark.is_some() { mark_key(k.kind, k.id) } else { (k.kind, k.id) };
            if self.numbers.get(&at) != Some(&n) {
                changed.insert(at, n);
            }
        }
        (!changed.is_empty()).then_some(changed)
    }

    /// The endnotes that follow section `si`: `ends` is in section order.
    pub(super) fn after(&self, si: usize) -> impl Iterator<Item = i64> + '_ {
        let (from, to) = (self.ends.partition_point(|&e| e < si), self.ends.partition_point(|&e| e <= si));
        self.endnotes[from..to].iter().copied()
    }

    /// Whether endnotes follow section `si`.
    pub(super) fn follow(&self, si: usize) -> bool {
        self.ends.binary_search(&si).is_ok()
    }
}
