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
use crate::docx::write::props::Props;
use crate::editing::position::{Position, StoryId};
use crate::font::Fonts;
use crate::layout::{FloatId, ParaId, Unit};

/// The Frame dialog's properties of a `w:framePr`.
pub fn properties(fr: &FramePr) -> FrameProperties {
    let pt = |v: Option<f64>| Points(v.unwrap_or(0.0));
    FrameProperties {
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

/// The `w:framePr` of `p`, keeping a drop cap's `dropCap` and `lines` from `old`, in the order the attributes are
/// written in.
pub fn frame_xml(w: &str, p: &FrameProperties, old: Option<&FramePr>) -> String {
    let twips = |v: Points| ((v.0 * 20.0).round() as i64).to_string();
    let mut a: Vec<(&str, String)> = Vec::new();
    if let Some(d) = old.and_then(|o| o.drop_cap.clone()) {
        a.push(("dropCap", d));
    }
    if let Some(l) = old.and_then(|o| o.lines) {
        a.push(("lines", l.to_string()));
    }
    if p.width_rule == FrameWidthRule::Exactly {
        a.push(("w", twips(p.width)));
    }
    if p.height_rule != FrameHeightRule::Auto {
        a.push(("h", twips(p.height)));
    }
    if p.height_rule == FrameHeightRule::Exactly {
        a.push(("hRule", "exact".into()));
    }
    if p.horizontal_distance.0 > 0.0 {
        a.push(("hSpace", twips(p.horizontal_distance)));
    }
    if p.vertical_distance.0 > 0.0 {
        a.push(("vSpace", twips(p.vertical_distance)));
    }
    a.push(("wrap", if p.wrap == FrameWrap::None { "notBeside" } else { "around" }.into()));
    match p.vertical_relative {
        FrameVerticalRelative::Margin => a.push(("vAnchor", "margin".into())),
        FrameVerticalRelative::Page => a.push(("vAnchor", "page".into())),
        FrameVerticalRelative::Paragraph => a.push(("vAnchor", "text".into())),
    }
    let h = match p.horizontal_relative {
        FrameHorizontalRelative::Margin => "margin",
        FrameHorizontalRelative::Page => "page",
        FrameHorizontalRelative::Column => "text",
    };
    a.push(("hAnchor", h.into()));
    let ha = |x: FrameHorizontalAlign| match x {
        FrameHorizontalAlign::Left => "left",
        FrameHorizontalAlign::Center => "center",
        FrameHorizontalAlign::Right => "right",
        FrameHorizontalAlign::Inside => "inside",
        FrameHorizontalAlign::Outside => "outside",
    };
    match p.horizontal_align {
        Some(x) => a.push(("xAlign", ha(x).into())),
        None => a.push(("x", twips(p.horizontal))),
    }
    let va = |y: FrameVerticalAlign| match y {
        FrameVerticalAlign::Top => "top",
        FrameVerticalAlign::Center => "center",
        FrameVerticalAlign::Bottom => "bottom",
        FrameVerticalAlign::Inside => "inside",
        FrameVerticalAlign::Outside => "outside",
    };
    match p.vertical_align {
        Some(y) => a.push(("yAlign", va(y).into())),
        None => a.push(("y", twips(p.vertical))),
    }
    if p.lock_anchor {
        a.push(("anchorLock", "1".into()));
    }
    let attrs: String = a.iter().map(|(k, v)| format!(" {w}{k}=\"{}\"", crate::docx::write::props::esc(v))).collect();
    format!("<{w}framePr{attrs}/>")
}

impl Editor {
    /// The frame the selection is in, or is.
    pub(super) fn frame_here(&self) -> Option<FloatId> {
        let frame = |f: FloatId| self.float_of(f).is_some_and(|fl| fl.frame_src.is_some());
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
    fn frame_pr(&self, f: FloatId) -> Option<FramePr> {
        let res = self.doc.resolver.as_ref()?;
        let cx = self.cx();
        let tb = self.float_of(f)?.text_box.as_ref()?;
        tb.blocks.iter().find_map(|b| match b {
            Block::Para(p) => res.para_from_xml(p.style.direct.0.as_deref(), None, &cx).style.frame,
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

    /// The frame's paragraphs with their `w:framePr` edited, and the first one's new `w:framePr`.
    fn frame_paras(&self, f: FloatId, edit: &dyn Fn(&mut Props, &str, Option<&FramePr>)) -> Option<(Vec<Block>, Option<FramePr>)> {
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
                    let old = res.para_from_xml(p.style.direct.0.as_deref(), None, &cx).style.frame;
                    let mut pr = Props::parse(p.style.direct.0.as_deref(), "pPr", &cx);
                    edit(&mut pr, &cx.w, old.as_ref());
                    let mut style = res.para_from_xml(pr.xml().as_deref(), None, &cx).style;
                    if first.is_none() {
                        first = Some(style.frame.clone());
                    }
                    style.frame = None;
                    style.in_frame = true;
                    p.style = Rc::new(style);
                    Block::Para(p)
                }
                b => b,
            })
            .collect();
        Some((blocks, first.flatten()))
    }

    /// Sets the frame's properties on each of its paragraphs, and places it by them.
    pub(super) fn set_frame(&mut self, props: FrameProperties, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return self.format_style_part("pPr", &|p, cx, para, _| p.set("framePr", frame_xml(&cx.w, &props, para.style.frame.as_ref())));
        }
        let Some(f) = self.frame_here() else { return Outcome::unavailable("the selection isn't in a frame") };
        let Some((blocks, Some(fr))) = self.frame_paras(f, &|p, w, old| p.set("framePr", frame_xml(w, &props, old))) else {
            return Outcome::Unchanged("no frame");
        };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no frame") };
        let sides = crate::docx::frames::bleed(&blocks);
        if let Some(tb) = &mut n.text_box {
            tb.blocks = blocks;
        }
        crate::docx::frames::place(&mut n, &fr, sides);
        let caret = self.selection;
        let out = self.set_float(n, "Frame", fonts);
        if !matches!(caret, Selection::Object(_)) {
            self.selection = caret;
        }
        out
    }

    /// Removes the frame: its paragraphs, without their `w:framePr`, go before the paragraph it was anchored to.
    pub(super) fn remove_frame(&mut self, fonts: &Fonts) -> Outcome {
        if self.style_target.is_some() {
            return self.format_style_part("pPr", &|p, _, _, _| p.remove("framePr"));
        }
        let Some(f) = self.frame_here() else { return Outcome::unavailable("the selection isn't in a frame") };
        let Some((mut blocks, _)) = self.frame_paras(f, &|p, _, _| p.remove("framePr")) else { return Outcome::Unchanged("no frame") };
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(slot) = self.index.para_slot(anchor) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let inlines: Vec<Inline> = p.inlines.iter().filter(|i| !matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)).cloned().collect();
        // The paragraphs are written as new ones: their source is the frame's, which goes.
        for b in &mut blocks {
            if let Block::Para(p) = b {
                p.src = None;
            }
        }
        let first = blocks.iter().find_map(|b| if let Block::Para(p) = b { Some(ParaId(p.id)) } else { None });
        let mut tx = self.begin("Remove Frame");
        if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err()
            || self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: blocks }).is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("stale frame");
        }
        let story = self.index.story(slot.container);
        if let Some(first) = first {
            self.selection = Selection::Caret(Position::text(story, first, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }
}
