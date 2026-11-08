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
}

impl Editor {
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
        let start = self.selection.ordered(&self.index).map_or(focus, |(a, _)| a);
        let end = self.selection.ordered(&self.index).map_or(focus, |(_, b)| b);
        let (a, _) = self.span(start, unit);
        let (_, b) = self.span(end, unit);
        self.selection = Selection::range(a, b, unit);
        Outcome::Moved
    }

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

    pub(super) fn block_mode(&mut self) -> Outcome {
        self.modes = Modes { extend: 0, block: true };
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
