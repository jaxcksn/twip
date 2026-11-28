//! What a node read from DOCX keeps of its source ([`Origin`]), and the model's views of it

use std::ops::Range;

use super::model::*;

/// A paragraph's place in its part's XML (byte offsets), for lossless saving.
#[derive(Clone, Debug)]
pub struct ParaSrc {
    pub ranges: Vec<Range<usize>>,
    /// The elements that produced its units, in order, with how many units each did: `w:t`, `w:tab`, a drawing, a
    /// whole complex field (from its `begin` run to its `end` run), ...
    pub segs: Vec<(Range<usize>, usize)>,
    /// The elements inside each simple field and hyperlink seg (by its start) that produced its result's units.
    pub simple: Vec<(usize, Vec<(Range<usize>, usize)>)>,
    /// Content that can't be mapped back (a field whose runs don't share a parent): edits rewrite it from the model.
    pub lossy: bool,
    /// Made by the reader to close a story that ends without a paragraph: no element, `ranges[0]` is where its XML
    /// goes, an empty range before the story's end tag or the story's whole element when it is written empty.
    pub closing: bool,
    /// The paragraphs a vanished mark joined onto this one's start, in order (the last of [`Self::ranges`] is its own).
    pub joined: Vec<JoinedPart>,
}

impl Paragraph {
    /// Where it was read from.
    pub(in crate::docx) fn src(&self) -> Option<&ParaSrc> {
        self.origin.get()
    }

    /// The paragraphs a vanished mark joined onto its start as read, in order (its own mark among them when that is
    /// vanished too).
    pub fn joined_parts(&self) -> &[JoinedPart] {
        self.src().map_or(&[][..], |s| &s.joined[..])
    }

    /// The paragraphs a vanished mark joined onto its start as read, when its own mark isn't one of them.
    pub fn joined_before(&self) -> &[JoinedPart] {
        self.src().filter(|s| s.joined.len() + 1 == s.ranges.len()).map_or(&[][..], |s| &s.joined[..])
    }

    /// How many paragraphs it was read as: more than one when vanished marks joined them, none for one made since.
    pub fn read_parts(&self) -> usize {
        self.src().map_or(0, |s| s.ranges.len())
    }

    /// The ends of [`Self::joined_parts`] moved by `to` (part, end).
    pub fn move_joined(&mut self, to: impl Fn(usize, u32) -> u32) {
        let Some(s) = self.src() else { return };
        let mut s = s.clone();
        for (k, j) in s.joined.iter_mut().enumerate() {
            j.end = to(k, j.end);
        }
        self.origin = Origin::new(s);
    }
}

impl Table {
    /// Its element's byte range in its part.
    pub(in crate::docx) fn src(&self) -> Option<&Range<usize>> {
        self.origin.get()
    }
}

impl Row {
    /// Its element's byte range in its part.
    pub(in crate::docx) fn src(&self) -> Option<&Range<usize>> {
        self.origin.get()
    }
}

impl Float {
    /// Whether, as read, the frame came after the start of `p`, its anchor (`None`: either was made since).
    pub fn read_after(&self, p: &Paragraph) -> Option<bool> {
        Some(self.frame_src.as_ref()?.start > p.src()?.ranges.first()?.start)
    }
}

/// The byte range a read block spans in its part.
#[cfg(test)]
pub(crate) fn read_range(b: &Block) -> Option<Range<usize>> {
    match b {
        Block::Para(p) => p.src().map(|s| s.ranges[0].start..s.ranges[s.ranges.len() - 1].end),
        Block::Table(t) => t.src().cloned(),
    }
}
