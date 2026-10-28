//! Extend mode and block selection: F8 turns extend mode on, and each F8 after it selects the word,
//! the sentence, the paragraph, then the document; movement extends the selection while it is on. Ctrl+Shift+F8 turns
//! block mode on: movement then selects a vertical block, the lines between the anchor and the focus cut at their x.
//! Esc ends either mode and keeps the selection; an edit ends it too.

use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::{Granularity, Selection};

/// The modes F8 and Ctrl+Shift+F8 start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    /// Extend mode's level: 0 off, 1 on, 2 word, 3 sentence, 4 paragraph, 5 document.
    pub extend: u8,
    pub block: bool,
}

impl Editor {
    /// F8: extend mode on, then each press selects the next larger unit.
    pub(super) fn extend_mode(&mut self) -> Outcome {
        self.modes.block = false;
        let level = (self.modes.extend + 1).min(5);
        self.modes.extend = level;
        let Some(focus) = self.selection.focus() else { return Outcome::Moved };
        let unit = match level {
            1 => return Outcome::Moved,
            2 => Granularity::Word,
            3 => Granularity::Sentence,
            4 => Granularity::Paragraph,
            _ => {
                let story = focus.story;
                let (Some(a), Some(b)) = (self.nav().story_edge(story, false), self.nav().story_edge(story, true)) else { return Outcome::Moved };
                self.selection = Selection::range(a, b, Granularity::Char);
                return Outcome::Moved;
            }
        };
        // The unit around the selection's start, grown to cover what is already selected.
        let start = self.selection.ordered(&self.index).map_or(focus, |(a, _)| a);
        let end = self.selection.ordered(&self.index).map_or(focus, |(_, b)| b);
        let (a, _) = self.span(start, unit);
        let (_, b) = self.span(end, unit);
        self.selection = Selection::range(a, b, unit);
        Outcome::Moved
    }

    /// A character typed in extend mode: the selection's focus goes past the next occurrence of it after the focus, in
    /// the same story (case-sensitive); nothing changes when there is none.
    pub(super) fn extend_to_char(&mut self, ch: char) -> Outcome {
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        let anchor = self.selection.anchor().unwrap_or(focus);
        let mut p = focus;
        for _ in 0..100_000 {
            let next = self.nav().right(p, false);
            if next.same_place(&p) {
                return Outcome::Unchanged("not found");
            }
            if let (Some((para, u)), Some((pn, un))) = (p.para_offset(), next.para_offset())
                && para == pn
                && let Some(q) = self.index.para(&self.doc, para)
                && super::text::UnitText::of(q).text.chars().skip(u.0 as usize).take((un.0 - u.0) as usize).any(|c| c == ch)
            {
                self.selection = Selection::range(anchor, next, Granularity::Char);
                return Outcome::Moved;
            }
            p = next;
        }
        Outcome::Unchanged("not found")
    }

    /// Ctrl+Shift+F8: block mode on.
    pub(super) fn block_mode(&mut self) -> Outcome {
        self.modes = Modes { extend: 0, block: true };
        Outcome::Moved
    }

    /// Esc in extend or block mode: the mode ends, the selection stays. False when neither is on.
    pub(super) fn end_modes(&mut self) -> bool {
        let on = self.modes != Modes::default();
        // Block mode's Esc leaves the caret at the block's start; extend mode's keeps the selection.
        if self.modes.block
            && let Selection::Block { anchor, .. } = self.selection
        {
            let start = self.block_pieces().first().map(|&(_, _, para, a, _)| Position::text(anchor.story, para, a)).unwrap_or(anchor);
            self.selection = Selection::Caret(start);
        }
        self.modes = Modes::default();
        on
    }

    /// A text selection made while block mode is on, as a block.
    pub(super) fn as_block(&self, s: Selection) -> Selection {
        match s {
            Selection::Text { anchor, focus, .. } if self.modes.block => Selection::Block { anchor, focus },
            other => other,
        }
    }

    /// The block's piece of each line between the anchor's and the focus's lines (same story and table cell), as
    /// paragraph unit ranges with their boxes: (page, rect, para, start, end).
    pub(super) fn block_pieces(&mut self) -> Vec<(usize, [f64; 4], ParaId, Unit, Unit)> {
        let Selection::Block { anchor, focus } = self.selection else { return Vec::new() };
        let (Some(ax), Some(fx)) = (self.nav().caret_x(anchor), self.nav().caret_x(focus)) else { return Vec::new() };
        let (x0, x1) = (ax.min(fx), ax.max(fx));
        let chain = self.index.chain_of(&anchor);
        let on = |p: &Position, para: ParaId, first: Unit, last: Unit| p.para_offset().is_some_and(|(q, u)| q == para && first <= u && u <= last);
        let mut out = Vec::new();
        let mut inside = false;
        let n = self.nav().pages.len();
        'pages: for page in 0..n {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            // Each paragraph's part of each line (a line shows several where the view joins them, REV-6).
            let parts = geo
                .lines
                .iter()
                .filter(|l| !l.repeat && geo.stories[l.story].story == anchor.story)
                .flat_map(|l| l.paras.iter().map(move |&p| (l, p)));
            for (l, para) in parts {
                let (Some(first), Some(last)) = (l.first_of(para), l.last_of(para)) else { continue };
                let hits = [on(&anchor, para, first, last), on(&focus, para, first, last)];
                let starts = hits.iter().any(|h| *h);
                if !inside && !starts {
                    continue;
                }
                let same_level = self.index.para_slot(para).map(|s| self.index.chain(s.container)) == Some(chain.clone());
                if same_level {
                    let stops = || l.stops.iter().filter(|s| s.para == para);
                    let lo = stops().filter(|s| s.x >= x0 - 0.01).min_by(|a, b| a.x.total_cmp(&b.x));
                    let hi = stops().filter(|s| s.x <= x1 + 0.01).max_by(|a, b| a.x.total_cmp(&b.x));
                    if let (Some(lo), Some(hi)) = (lo, hi)
                        && lo.unit < hi.unit
                    {
                        out.push((page, [lo.x, l.top, hi.x, l.bottom], para, lo.unit, hi.unit));
                    }
                }
                let both = hits[0] && hits[1];
                if both || (inside && starts) {
                    break 'pages;
                }
                inside = true;
            }
        }
        out
    }

    /// The block's text, one line per row of the block.
    pub(super) fn block_text(&mut self) -> String {
        let pieces = self.block_pieces();
        pieces
            .iter()
            .filter_map(|(_, _, para, a, b)| {
                let p = self.index.para(&self.doc, *para)?;
                Some(super::text::UnitText::of(p).text.chars().skip(a.0 as usize).take((b.0 - a.0) as usize).collect::<String>())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
