//! Frames: the Frame dialog's properties, and setting and removing the frame the selection is in. A frame is a
//! float whose text box holds the paragraphs that had the same `w:framePr`; each keeps its `w:framePr`.

use std::rc::Rc;

use super::commands::Outcome;
use super::selection::ObjectRef;
use super::step::Step;
use super::{Editor, Selection};
use crate::api::command::{
    FrameHeightRule, FrameHorizontalAlign, FrameHorizontalRelative, FrameProperties, FrameVerticalAlign, FrameVerticalRelative, FrameWidthRule,
    FrameWrap,
};
use crate::api::value::Points;
use crate::docx::Block;
use crate::docx::Inline;
use crate::docx::props::FramePr;
use crate::editing::position::{Position, StoryId};
use crate::font::Fonts;
use crate::format::{ParaEdit, ParaFlag, ParaProp};
use crate::layout::{FloatId, ParaId, Unit};

/// The Frame dialog's properties of a `w:framePr`.
pub fn properties(fr: &FramePr) -> FrameProperties {
    let pt = |v: Option<f64>| Points(v.unwrap_or(0.0));
    FrameProperties {
        allow_overlap: None,
        wrap: if matches!(fr.wrap.as_deref(), Some("notBeside" | "none")) { FrameWrap::None } else { FrameWrap::Around },
        width_rule: if fr.w.is_some_and(|w| w > 0.0) { FrameWidthRule::Exactly } else { FrameWidthRule::Auto },
        width: pt(fr.w),
        height_rule: match (fr.h.is_some_and(|h| h > 0.0), fr.h_rule.as_deref()) {
            (false, _) | (_, Some("auto")) => FrameHeightRule::Auto,
            (true, Some("exact")) => FrameHeightRule::Exactly,
            (true, _) => FrameHeightRule::AtLeast,
        },
        height: pt(fr.h),
        horizontal_align: match fr.x_align.as_deref() {
            Some("left") => Some(FrameHorizontalAlign::Left),
            Some("center") => Some(FrameHorizontalAlign::Center),
            Some("right") => Some(FrameHorizontalAlign::Right),
            Some("inside") => Some(FrameHorizontalAlign::Inside),
            Some("outside") => Some(FrameHorizontalAlign::Outside),
            _ => None,
        },
        horizontal: pt(fr.x),
        horizontal_relative: match fr.h_anchor.as_deref() {
            Some("margin") => FrameHorizontalRelative::Margin,
            Some("page") => FrameHorizontalRelative::Page,
            _ => FrameHorizontalRelative::Column,
        },
        horizontal_distance: Points(fr.h_space),
        vertical_align: match fr.y_align.as_deref() {
            Some("top") => Some(FrameVerticalAlign::Top),
            Some("center") => Some(FrameVerticalAlign::Center),
            Some("bottom") => Some(FrameVerticalAlign::Bottom),
            Some("inside") => Some(FrameVerticalAlign::Inside),
            Some("outside") => Some(FrameVerticalAlign::Outside),
            _ => None,
        },
        vertical: pt(fr.y),
        vertical_relative: match fr.v_anchor.as_deref() {
            Some("page") => FrameVerticalRelative::Page,
            Some("margin") => FrameVerticalRelative::Margin,
            // A frame without vAnchor is margin-relative, unless it is a drop cap.
            None if fr.drop_cap.as_deref().is_none_or(|d| d == "none") => FrameVerticalRelative::Margin,
            _ => FrameVerticalRelative::Paragraph,
        },
        vertical_distance: Points(fr.v_space),
        lock_anchor: fr.key.split(' ').any(|a| a.ends_with("anchorLock=1") || a.ends_with("anchorLock=true")),
    }
}

/// The frame of `p`, keeping a drop cap's kind and lines from `old`, and whether its anchor is locked.
pub fn frame_of(p: &FrameProperties, old: Option<&FramePr>) -> (FramePr, Option<bool>) {
    let twips = |v: Points| (v.0 * 20.0).round() / 20.0;
    let text = |v: &str| Some(v.to_string());
    let ha = |x: FrameHorizontalAlign| match x {
        FrameHorizontalAlign::Left => "left",
        FrameHorizontalAlign::Center => "center",
        FrameHorizontalAlign::Right => "right",
        FrameHorizontalAlign::Inside => "inside",
        FrameHorizontalAlign::Outside => "outside",
    };
    let va = |y: FrameVerticalAlign| match y {
        FrameVerticalAlign::Top => "top",
        FrameVerticalAlign::Center => "center",
        FrameVerticalAlign::Bottom => "bottom",
        FrameVerticalAlign::Inside => "inside",
        FrameVerticalAlign::Outside => "outside",
    };
    let mut f = FramePr {
        w: (p.width_rule == FrameWidthRule::Exactly).then(|| twips(p.width)),
        h: (p.height_rule != FrameHeightRule::Auto).then(|| twips(p.height)),
        h_rule: (p.height_rule == FrameHeightRule::Exactly).then(|| "exact".to_string()),
        x: p.horizontal_align.is_none().then(|| twips(p.horizontal)),
        y: p.vertical_align.is_none().then(|| twips(p.vertical)),
        x_align: p.horizontal_align.map(|x| ha(x).to_string()),
        y_align: p.vertical_align.map(|y| va(y).to_string()),
        h_anchor: text(match p.horizontal_relative {
            FrameHorizontalRelative::Margin => "margin",
            FrameHorizontalRelative::Page => "page",
            FrameHorizontalRelative::Column => "text",
        }),
        v_anchor: text(match p.vertical_relative {
            FrameVerticalRelative::Margin => "margin",
            FrameVerticalRelative::Page => "page",
            FrameVerticalRelative::Paragraph => "text",
        }),
        wrap: text(if p.wrap == FrameWrap::None { "notBeside" } else { "around" }),
        h_space: if p.horizontal_distance.0 > 0.0 { twips(p.horizontal_distance) } else { 0.0 },
        v_space: if p.vertical_distance.0 > 0.0 { twips(p.vertical_distance) } else { 0.0 },
        drop_cap: old.and_then(|o| o.drop_cap.clone()),
        lines: old.and_then(|o| o.lines),
        key: String::new(),
    };
    let locked = p.lock_anchor.then_some(true);
    f.key = crate::docx::para_props::frame_key(&f, locked);
    (f, locked)
}

/// Sets frame `f` on a paragraph's authored properties.
fn set_frame(e: &mut ParaEdit, (f, locked): (FramePr, Option<bool>)) {
    (e.frame, e.frame_locked) = (Some(f), locked);
    e.whole(ParaProp::Frame);
}

/// A `w:x` or `w:y` of `v` points a twip further off when it rounds to one of `codes`, the twips that stand for an
/// alignment: a dragged place stays a place.
fn off_codes(v: f64, codes: &[i64]) -> f64 {
    let t = (v * 20.0).round() as i64;
    if codes.contains(&t) { (t - 1) as f64 / 20.0 } else { v }
}

impl Editor {
    /// The frame the selection is in, or is.
    pub(crate) fn frame_here(&self) -> Option<FloatId> {
        let frame = |f: FloatId| self.float_of(f).is_some_and(crate::docx::Float::is_frame);
        if let Selection::Object(ObjectRef::Floating(f)) = self.selection
            && frame(f)
        {
            return Some(f);
        }
        let mut s = self.selection.focus()?.story;
        while let StoryId::TextBox(f) = s {
            if frame(f) {
                return Some(f);
            }
            s = self.index.story(self.index.para_slot(self.index.float_anchor(f)?)?.container);
        }
        None
    }

    /// The `w:framePr` of the frame's first paragraph.
    pub(super) fn frame_pr(&self, f: FloatId) -> Option<FramePr> {
        let res = self.doc.resolver.as_ref()?;
        let cx = self.cx();
        let tb = self.float_of(f)?.text_box.as_ref()?;
        tb.blocks.iter().find_map(|b| match b {
            Block::Para(p) => crate::docx::para_xml::of(res, &p.style.direct, None, &cx).style.frame,
            _ => None,
        })
    }

    /// The Frame dialog's properties of the frame the selection is in, or of a style's XML.
    pub fn frame_properties(&self, style: Option<&str>) -> Option<FrameProperties> {
        match style {
            Some(raw) => Some(self.style_formatting(raw)?.0.style.frame.as_ref().map(properties).unwrap_or_default()),
            None => Some(properties(&self.frame_pr(self.frame_here()?)?)),
        }
    }

    /// The frame's paragraphs with their `w:framePr` edited, and the first one's new `w:framePr`; `framed`: whether
    /// they stay in the frame.
    fn frame_paras(&self, f: FloatId, framed: bool, edit: &dyn Fn(&mut ParaEdit, Option<&FramePr>)) -> Option<(Vec<Block>, Option<FramePr>)> {
        let res = self.doc.resolver.clone()?;
        let cx = self.cx();
        let mut first = None;
        let blocks = self
            .float_of(f)?
            .text_box
            .as_ref()?
            .blocks
            .iter()
            .cloned()
            .map(|b| match b {
                Block::Para(mut p) => {
                    let old = crate::docx::para_xml::of(&res, &p.style.direct, None, &cx).style.frame;
                    let mut e = ParaEdit::new((**p.style.direct.get()).clone());
                    edit(&mut e, old.as_ref());
                    let mut style = crate::docx::para_xml::of(&res, &p.style.direct.edit(e), None, &cx).style;
                    let frame = style.frame.take();
                    if first.is_none() {
                        first = Some(frame);
                    }
                    let mut again = crate::docx::para_xml::again(&res, &p.style, style, None, &cx);
                    again.in_frame = framed;
                    p.style = Rc::new(again);
                    Block::Para(p)
                }
                b => b,
            })
            .collect();
        Some((blocks, first.flatten()))
    }

    /// Sets the frame's properties and, given `allow_overlap`, whether it may overlap other frames and floating
    /// objects (`w:suppressOverlap` off on each of its paragraphs), in one undo step ("Frame").
    pub fn set_frame_with_overlap(&mut self, props: FrameProperties, allow_overlap: Option<bool>, fonts: &Fonts) -> Outcome {
        self.set_frame_as(props, allow_overlap, "Frame", fonts)
    }

    /// [`Editor::set_frame_with_overlap`], as one undo step named `label`.
    fn set_frame_as(&mut self, props: FrameProperties, allow_overlap: Option<bool>, label: &'static str, fonts: &Fonts) -> Outcome {
        let overlap = |e: &mut ParaEdit| {
            // Off over a style's on.
            if let Some(allow) = allow_overlap {
                e.flag(ParaFlag::SUPPRESS_OVERLAP, !allow);
            }
        };
        if self.style_target.is_some() {
            return self.format_style_ppr(&|e, para| {
                set_frame(e, frame_of(&props, para.style.frame.as_ref()));
                overlap(e);
            });
        }
        let Some(f) = self.frame_here() else { return Outcome::unavailable("the selection isn't in a frame") };
        let edit = |e: &mut ParaEdit, old: Option<&FramePr>| {
            set_frame(e, frame_of(&props, old));
            overlap(e);
        };
        let Some((blocks, Some(fr))) = self.frame_paras(f, true, &edit) else {
            return Outcome::Unchanged("no frame");
        };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no frame") };
        if let Some(allow) = allow_overlap {
            n.allow_overlap = allow;
        }
        let sides = crate::docx::frames::bleed(&blocks);
        if let Some(tb) = &mut n.text_box {
            tb.blocks = blocks;
        }
        crate::docx::frames::place(&mut n, &fr, sides, self.doc.settings.compat() == crate::docx::Compat::Legacy);
        let caret = self.selection;
        let out = self.set_float(n, label, fonts);
        if !matches!(caret, Selection::Object(_)) {
            self.selection = caret;
        }
        out
    }

    /// A frame dragged to `moved` (its place, or its size by a handle), as its paragraphs' `w:framePr` say it: the
    /// moved side's alignment becomes an offset from its reference, a dragged size an exact width and an at-least
    /// height (an exact one stays exact). A drop cap stays in its paragraph; only its size changes.
    pub(super) fn commit_frame(&mut self, f: FloatId, moved: &crate::docx::Float, label: &'static str, fonts: &Fonts) -> Outcome {
        let (Some(was), Some(fr)) = (self.float_of(f).cloned(), self.frame_pr(f)) else { return Outcome::Unchanged("no frame") };
        let mut p = properties(&fr);
        let drop_cap = fr.drop_cap.as_deref().is_some_and(|d| d != "none");
        let ne = |a: f64, b: f64| (a - b).abs() > 1e-9;
        if !drop_cap && (moved.h_align != was.h_align || ne(moved.h_offset, was.h_offset) || moved.h_relative != was.h_relative) {
            p.horizontal_align = None;
            // `w:x` holds the place plus one twip.
            p.horizontal = Points(off_codes(moved.h_offset + 0.05, &[0, -4, -8, -12, -16]));
            p.horizontal_relative = match moved.h_relative {
                crate::docx::HorizontalRelation::Margin => FrameHorizontalRelative::Margin,
                crate::docx::HorizontalRelation::Page => FrameHorizontalRelative::Page,
                _ => FrameHorizontalRelative::Column,
            };
        }
        if !drop_cap && (moved.v_align != was.v_align || ne(moved.v_offset, was.v_offset) || moved.v_relative != was.v_relative) {
            p.vertical_align = None;
            p.vertical_relative = match moved.v_relative {
                crate::docx::VerticalRelation::Margin => FrameVerticalRelative::Margin,
                crate::docx::VerticalRelation::Page => FrameVerticalRelative::Page,
                _ => FrameVerticalRelative::Paragraph,
            };
            // From the text `w:y` is the place itself; from the page or margin it is one twip more.
            let twip = if p.vertical_relative == FrameVerticalRelative::Paragraph { 0.0 } else { 0.05 };
            p.vertical = Points(off_codes(moved.v_offset + twip, &[-4, -8, -12, -16, -20]));
        }
        if ne(moved.w, was.w) {
            (p.width_rule, p.width) = (FrameWidthRule::Exactly, Points(moved.w));
        }
        if ne(moved.h, was.h) {
            if p.height_rule == FrameHeightRule::Auto {
                p.height_rule = FrameHeightRule::AtLeast;
            }
            p.height = Points(moved.h);
        }
        self.selection = Selection::Object(ObjectRef::Floating(f));
        self.set_frame_as(p, None, label, fonts)
    }

    /// Whether the frame the selection is in, or is, may overlap other frames and floating objects (`w:suppressOverlap`
    /// off).
    pub fn frame_allow_overlap(&self) -> Option<bool> {
        let f = self.frame_here()?;
        let res = self.doc.resolver.as_ref()?;
        let cx = self.cx();
        let tb = self.float_of(f)?.text_box.as_ref()?;
        let apart = tb
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Para(p) if crate::docx::para_xml::of(res, &p.style.direct, None, &cx).style.suppress_overlap));
        Some(!apart)
    }

    /// Lets the frame the selection is in, or is, overlap other frames and floating objects, or keeps it apart
    /// (`w:suppressOverlap` on each of its paragraphs). One undo step ("Frame").
    pub fn set_frame_allow_overlap(&mut self, allow: bool, fonts: &Fonts) -> Outcome {
        let Some(props) = self.frame_properties(None) else { return Outcome::unavailable("the selection isn't in a frame") };
        self.set_frame_with_overlap(props, Some(allow), fonts)
    }

    /// Removes the frame: its paragraphs, without their `w:framePr`, go where they were beside the paragraph it was
    /// anchored to.
    pub(super) fn remove_frame(&mut self, fonts: &Fonts) -> Outcome {
        let unframed = |e: &mut ParaEdit| {
            (e.frame, e.frame_locked) = (None, None);
            e.whole(ParaProp::Frame);
        };
        if self.style_target.is_some() {
            return self.format_style_ppr(&|e, _| unframed(e));
        }
        let Some(f) = self.frame_here() else { return Outcome::unavailable("the selection isn't in a frame") };
        let Some((mut blocks, _)) = self.frame_paras(f, false, &|e, _| unframed(e)) else { return Outcome::Unchanged("no frame") };
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(slot) = self.index.para_slot(anchor) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let is_it = |i: &Inline| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f);
        let Some((k, read_after)) = p.inlines.iter().enumerate().find_map(|(k, i)| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some((k, fl.read_after(p))),
            _ => None,
        }) else {
            return Outcome::Unchanged("no anchor");
        };
        // Anchored after the paragraph: read before a table or the end
        let after = read_after.unwrap_or_else(|| p.inlines[..k].iter().any(|i| i.units() > 0));
        let inlines: Vec<Inline> = p.inlines.iter().filter(|i| !is_it(i)).cloned().collect();
        let Some(first) = blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(p.id) } else { None }) else {
            return Outcome::Unchanged("no frame");
        };
        // Frames sharing the anchor, removed before: their paragraphs, numbered as read, stay on their side of this
        // one's. Paragraphs made since reading are numbered after every read one.
        // shortcut: a tracked removal inserts copies, numbered as new, so a frame removed after it goes beside the
        // anchor; the model keeps no other record of where restored paragraphs came from.
        let between = |b: &Block, lo: u64, hi: u64| matches!(b, Block::Para(q) if lo < q.id && q.id < hi);
        let around = self.index.blocks(&self.doc, slot.container).map_or(&[][..], Vec::as_slice);
        let at = if after {
            slot.block + 1 + around.iter().skip(slot.block + 1).take_while(|b| between(b, anchor.0, first)).count()
        } else {
            slot.block - around[..slot.block.min(around.len())].iter().rev().take_while(|b| between(b, first, anchor.0)).count()
        };
        // The paragraphs are written as new ones: their source is the frame's, which goes.
        for b in &mut blocks {
            if let Block::Para(p) = b {
                p.origin = crate::docx::Origin::NONE;
            }
        }
        let mut tx = self.begin("Remove Frame");
        if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err()
            || self.step(&mut tx, Step::Blocks { container: slot.container, range: at..at, with: blocks }).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("stale frame");
        }
        // The caret in the first paragraph inserted (tracked, a copy of the frame's)
        let story = self.index.story(slot.container);
        let restored = self.index.blocks(&self.doc, slot.container).and_then(|b| {
            b.get(at..)?.iter().find_map(|b| match b {
                Block::Para(p) => Some(ParaId(p.id)),
                Block::Table(_) => None,
            })
        });
        if let Some(para) = restored {
            self.selection = Selection::Caret(Position::text(story, para, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }
}
