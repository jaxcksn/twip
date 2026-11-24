//! Extend mode and block selection

use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::{Granularity, Selection};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    pub extend: u8,
    pub block: bool,
    /// Where extending started: what Shrink Selection shrinks back around.
    pub origin: Option<Position>,
}

impl Editor {
    pub(super) fn extend_mode(&mut self) -> Outcome {
        self.modes.block = false;
        let level = (self.modes.extend + 1).min(5);
        self.modes.extend = level;
        let Some(focus) = self.selection.focus() else { return Outcome::Moved };
        if level == 1 {
            self.modes.origin = Some(focus);
        }
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
        let start = self.selection.ordered(&self.index).map_or(focus, |(a, _)| a);
        let end = self.selection.ordered(&self.index).map_or(focus, |(_, b)| b);
        let (a, _) = self.span(start, unit);
        let (_, b) = self.span(end, unit);
        self.selection = Selection::range(a, b, unit);
        Outcome::Moved
    }

    /// Shrink Selection (Shift+F8): extending, the selection one level smaller around where extending started (or,
    /// once the selection has moved off that, its anchor): the story to its paragraph, then the sentence, the word,
    /// and the caret, still extending.
    pub fn shrink_selection(&mut self) -> Outcome {
        if self.modes.extend < 2 {
            return Outcome::Unchanged("not extending");
        }
        // Origin
        let holds =
            |o: &Position| self.selection.ordered(&self.index).is_some_and(|(a, b)| self.index.cmp(&a, o).is_le() && self.index.cmp(o, &b).is_le());
        let Some(origin) = self.modes.origin.filter(holds).or_else(|| self.selection.anchor()).or_else(|| self.selection.focus()) else {
            return Outcome::Unchanged("no caret");
        };
        self.modes.origin = Some(origin);
        let current = self.selection.ordered(&self.index);
        let mut level = (self.modes.extend - 1).min(4);
        // The largest unit below the extension's that stays within the selection: a selection made smaller since
        // (by `select_range`) isn't grown to a word.
        while level > 1 {
            let unit = match level {
                2 => Granularity::Word,
                3 => Granularity::Sentence,
                _ => Granularity::Paragraph,
            };
            let (a, b) = self.span(origin, unit);
            if current.is_some_and(|(x, y)| self.index.cmp(&x, &a).is_le() && self.index.cmp(&b, &y).is_le()) {
                self.modes.extend = level;
                self.selection = Selection::range(a, b, unit);
                return Outcome::Moved;
            }
            level -= 1;
        }
        self.modes.extend = 1;
        self.selection = Selection::Caret(origin);
        Outcome::Moved
    }

    /// Ctrl+Alt+Page Up or Down: the caret to the top or bottom of the window; `extend`, the selection to there.
    pub fn move_to_window_edge(&mut self, bottom: bool, extend: bool, fonts: &crate::font::Fonts) -> Outcome {
        let extend = extend || self.modes != Modes::default();
        let dir = if bottom { super::commands::Dir::Forward } else { super::commands::Dir::Back };
        let out = self.move_caret(super::commands::MoveBy::ScreenEdge, dir, extend, fonts);
        if self.modes.block {
            self.selection = self.as_block(self.selection);
        }
        out
    }

    pub(super) fn extend_to_char(&mut self, ch: char) -> Outcome {
        let Some(focus) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        let anchor = self.selection.anchor().unwrap_or(focus);
        let mut p = focus;
        for _ in 0..100_000 {
            if let Some((para, mut u)) = p.para_offset()
                && let Some(q) = self.index.para(&self.doc, para)
            {
                let t = super::text::UnitText::in_review(q, &self.doc.review);
                while u < t.len() {
                    let v = t.next_grapheme(u);
                    if v <= u {
                        break;
                    }
                    if t.shows(u..v, ch) {
                        self.selection = Selection::range(anchor, Position::text(p.story, para, v), Granularity::Char);
                        return Outcome::Moved;
                    }
                    u = v;
                }
                p = Position::text(p.story, para, t.len());
            }
            let next = self.nav().right(p, false);
            if next.same_place(&p) {
                return Outcome::Unchanged("not found");
            }
            p = next;
        }
        Outcome::Unchanged("not found")
    }

    pub(super) fn block_mode(&mut self) -> Outcome {
        self.modes = Modes { extend: 0, block: true, origin: None };
        Outcome::Moved
    }

    pub(super) fn end_modes(&mut self) -> bool {
        let on = self.modes != Modes::default();
        if self.modes.block
            && let Selection::Block { anchor, .. } = self.selection
        {
            let start = self.block_pieces().first().map(|&(_, _, para, a, _)| Position::text(anchor.story, para, a)).unwrap_or(anchor);
            self.selection = Selection::Caret(start);
        }
        self.modes = Modes::default();
        on
    }

    pub(super) fn as_block(&self, s: Selection) -> Selection {
        match s {
            Selection::Text { anchor, focus, .. } if self.modes.block => Selection::Block { anchor, focus },
            other => other,
        }
    }

    pub(super) fn block_pieces(&mut self) -> Vec<(usize, super::view::Band, ParaId, Unit, Unit)> {
        let Selection::Block { anchor, focus } = self.selection else { return Vec::new() };
        let (Some(ax), Some(fx)) = (self.nav().caret_x(anchor), self.nav().caret_x(focus)) else { return Vec::new() };
        let (x0, x1) = (ax.min(fx), ax.max(fx));
        let chain = self.index.chain_of(&anchor);
        let on = |p: &Position, para: ParaId, first: Unit, last: Unit| p.para_offset().is_some_and(|(q, u)| q == para && first <= u && u <= last);
        let mut out = Vec::new();
        let mut inside = false;
        let n = self.nav().pages.len();
        // From the page before the earlier end's: a position at a line's end also starts the next line.
        let first = [anchor, focus].into_iter().filter_map(|p| self.nav().line_of(p).map(|l| l.page)).min().unwrap_or(0);
        'pages: for page in first.saturating_sub(1)..n {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
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
                        out.push((page, l.strip(lo.x, hi.x), para, lo.unit, hi.unit));
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

    /// The block's text, a line a piece, cut to its first `limit` characters, and whether that is all of it. Each
    /// paragraph is read once.
    pub(super) fn block_text(&mut self, limit: usize) -> (String, bool) {
        let pieces = self.block_pieces();
        let (mut out, mut chars, mut first) = (String::new(), 0usize, true);
        let mut cur: Option<(crate::layout::ParaId, super::text::Seen)> = None;
        for &(_, _, para, a, b) in &pieces {
            if !matches!(&cur, Some((p, _)) if *p == para) {
                let Some(p) = self.index.para(&self.doc, para) else { continue };
                cur = Some((para, crate::prof::time("block:paragraph", || super::seen_text(p, &self.doc.review))));
            }
            let Some((_, seen)) = &cur else { continue };
            let (lo, hi) = (seen.units.partition_point(|&u| u < a.0), seen.units.partition_point(|&u| u < b.0));
            if !first {
                if chars == limit {
                    return (out, false);
                }
                out.push('\n');
                chars += 1;
            }
            first = false;
            let take = (hi - lo).min(limit - chars);
            out.extend(seen.chars[lo..lo + take].iter().copied().map(super::plain_char));
            chars += take;
            if take < hi - lo {
                return (out, false);
            }
        }
        (out, true)
    }
}
