//! Frames

use std::rc::Rc;

use super::model::{Block, Compat, Document, Float, Inline, Paragraph, TextBox};

pub fn convert(doc: &mut Document) {
    let legacy = doc.settings.compat() == Compat::Legacy;
    for s in &mut doc.sections {
        blocks(&mut s.blocks, legacy, true);
    }
    for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
        blocks(b, legacy, false);
    }
}

fn key(p: &Paragraph) -> Option<&str> {
    p.style.frame.as_ref().map(|f| f.key.as_str())
}

/// `pages`: the body's own blocks, where a page break before a frame paragraph breaks the page.
fn blocks(list: &mut Vec<Block>, legacy: bool, pages: bool) {
    let mut out = Vec::with_capacity(list.len());
    let mut groups: Vec<Vec<Paragraph>> = Vec::new();
    for b in std::mem::take(list) {
        match b {
            // A page break before a frame paragraph starts a frame of its own.
            Block::Para(p) if key(&p).is_some() => match groups.last_mut() {
                Some(g) if key(&g[0]) == key(&p) && !(pages && p.style.page_break_before) => g.push(p),
                _ => groups.push(vec![p]),
            },
            Block::Para(mut p) => {
                // It breaks the page before the paragraph the frames anchor at; the frames before the break stay.
                if pages && groups.iter().any(|g| g[0].style.page_break_before) && !p.style.page_break_before {
                    let mut s = (*p.style).clone();
                    s.page_break_before = true;
                    p.style = Rc::new(s);
                }
                for (k, g) in groups.drain(..).enumerate() {
                    p.inlines.insert(k, Inline::Float(Box::new(float(g, legacy)), p.mark.clone()));
                }
                out.push(Block::Para(p));
            }
            Block::Table(mut t) => {
                for r in &mut t.rows {
                    for c in &mut r.cells {
                        blocks(&mut c.blocks, legacy, false);
                    }
                }
                anchor_before(&mut out, std::mem::take(&mut groups), legacy);
                out.push(Block::Table(t));
            }
        }
    }
    anchor_before(&mut out, groups, legacy);
    *list = out;
}

/// Frames before a table or ending the list are placed by the page or margin: they anchor at the end of the paragraph
/// before them.
/// shortcut: a text-anchored frame or drop cap there, or one after a table, stays in the flow.
fn anchor_before(out: &mut Vec<Block>, groups: Vec<Vec<Paragraph>>, legacy: bool) {
    let placed = |g: &[Paragraph]| {
        g[0].style.frame.as_ref().is_some_and(|f| {
            matches!(f.v_anchor.as_deref(), None | Some("page" | "margin"))
                && !(legacy && in_line(f))
                && f.drop_cap.as_deref().is_none_or(|d| d == "none")
        })
    };
    for g in groups {
        match out.last_mut() {
            Some(Block::Para(p)) if placed(&g) => p.inlines.push(Inline::Float(Box::new(float(g, legacy)), p.mark.clone())),
            _ => out.extend(g.into_iter().map(Block::Para)),
        }
    }
}

fn float(group: Vec<Paragraph>, legacy: bool) -> Float {
    let fr = group[0].style.frame.clone().unwrap();
    let start = group.first().and_then(|p| p.src.as_ref()).and_then(|s| s.ranges.first()).map(|r| r.start);
    let end = group.last().and_then(|p| p.src.as_ref()).and_then(|s| s.ranges.last()).map(|r| r.end);
    let frame_src = start.zip(end).map(|(a, b)| a..b);
    let apart = group.iter().any(|p| p.style.suppress_overlap);
    let flow = group[0].style.text_flow;
    let blocks = group
        .into_iter()
        .map(|mut p| {
            let mut s = (*p.style).clone();
            s.frame = None;
            s.in_frame = true;
            p.style = Rc::new(s);
            Block::Para(p)
        })
        .collect::<Vec<Block>>();
    let sides = bleed(&blocks);
    let mut f = Float {
        id: super::next_para_id(),
        w: 0.0,
        auto_w: true,
        h: 0.0,
        image: None,
        alt: "".into(),
        lock_aspect: false,
        text_box: Some(TextBox { blocks, insets: [0.0; 4], outline: 0.0, auto_fit: true, v_anchor: 0, flow }),
        h_relative: String::new(),
        h_align: None,
        h_offset: 0.0,
        v_relative: String::new(),
        v_align: None,
        v_offset: 0.0,
        wrap: String::new(),
        wrap_text: "bothSides".into(),
        dist: [0.0; 4],
        effect: [0.0; 4],
        poly: None,
        behind: false,
        allow_overlap: !apart,
        layout_in_cell: true,
        order: 0,
        z: 0,
        src: None,
        locked: false,
        frame_src,
        in_note: false,
        parts: Vec::new(),
        shape: None,
        rot_swap: false,
        vml: false,
        relative: Default::default(),
    };
    place(&mut f, &fr, sides, legacy);
    f
}

/// How far a frame's paragraph borders reach past it on the left and on the right, points: on a side its first
/// paragraph has a border, the most of any bordered paragraph's 1.5pt gap, border width (three lines for a double)
/// and spacing, less its indent on that side (a hanging first line counted on the left), and not below 0.
pub fn bleed(blocks: &[Block]) -> [f64; 2] {
    let paras: Vec<&Paragraph> = blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect();
    let reach = |b: &Option<super::props::Border>, indent: f64| {
        b.as_ref().filter(|b| b.visible()).map(|b| (1.5 + b.width * if b.style == "double" { 3.0 } else { 1.0 } + b.space - indent).max(0.0))
    };
    let side = |left: bool| {
        let first = paras.first().map(|p| if left { &p.style.border_left } else { &p.style.border_right });
        if first.is_none_or(|b| b.as_ref().is_none_or(|b| !b.visible())) {
            return 0.0;
        }
        paras
            .iter()
            .filter_map(|p| match left {
                true => reach(&p.style.border_left, p.style.ind_left + p.style.ind_first.min(0.0)),
                false => reach(&p.style.border_right, p.style.ind_right),
            })
            .fold(0.0, f64::max)
    };
    [side(true), side(false)]
}

/// No vertical position: `w:y` 0, or no `w:y` and no `w:yAlign` other than `inline`.
fn in_line(fr: &super::props::FramePr) -> bool {
    match fr.y {
        Some(y) => (y * 20.0).round() == 0.0,
        None => fr.y_align.as_deref().is_none_or(|a| a == "inline"),
    }
}

/// Places a frame's float by its `framePr`: its size, position, wrapping and distance from text. `sides` is how far
/// its paragraphs' borders reach past it on the left and the right ([`bleed`]), kept from the text with `hSpace`.
/// `legacy`: the document is in compat ≤ 14.
pub fn place(f: &mut Float, fr: &super::props::FramePr, sides: [f64; 2], legacy: bool) {
    let margin_cap = fr.drop_cap.as_deref() == Some("margin");
    let anchor = |a: Option<&str>, text: &str, missing: &str| match a {
        Some(a @ ("margin" | "page")) => a.to_string(),
        Some(_) => text.to_string(),
        None => missing.to_string(),
    };
    // A frame without vAnchor is margin-relative; a drop cap sits in its paragraph whatever it says.
    let drop_cap = fr.drop_cap.as_deref().is_some_and(|d| d != "none");
    f.w = fr.w.unwrap_or(0.0);
    f.auto_w = fr.w.is_none_or(|w| w <= 0.0);
    // `hRule="auto"`: the content's height, whatever `w:h` says.
    f.h = if fr.h_rule.as_deref() == Some("auto") { 0.0 } else { fr.h.unwrap_or(0.0) };
    if let Some(tb) = &mut f.text_box {
        tb.auto_fit = fr.h_rule.as_deref() != Some("exact");
    }
    f.h_relative = if margin_cap { "leftMargin".into() } else { anchor(fr.h_anchor.as_deref(), "column", "column") };
    // An explicit `w:x` / `w:y` beats the alignment. It holds the position plus one twip; 0 and a few negative values
    // are alignment codes.
    let (x_code, x_at) = fr.x.map_or((None, None), |x| match (x * 20.0).round() as i64 {
        0 => (None, None),
        -4 => (Some("center"), None),
        -8 => (Some("right"), None),
        -12 => (Some("inside"), None),
        -16 => (Some("outside"), None),
        _ => (None, Some(x - 0.05)),
    });
    // From the text, `w:y` is the position itself.
    let text_y = fr.v_anchor.as_deref().is_some_and(|a| a != "margin" && a != "page");
    let (y_code, y_at) = fr.y.map_or((None, None), |y| match (y * 20.0).round() as i64 {
        0 => (None, Some(0.0)),
        -4 => (Some("top"), None),
        -8 => (Some("center"), None),
        -12 => (Some("bottom"), None),
        -16 => (Some("inside"), None),
        -20 => (Some("outside"), None),
        _ if text_y => (None, Some(y)),
        _ => (None, Some(y - 0.05)),
    });
    f.h_align = if margin_cap {
        Some("right".into())
    } else if fr.x.is_some() {
        x_code.map(str::to_string)
    } else {
        fr.x_align.clone()
    };
    f.h_offset = x_at.unwrap_or(0.0);
    f.v_relative = anchor(fr.v_anchor.as_deref(), "paragraph", if drop_cap { "paragraph" } else { "margin" });
    // Compat ≤ 14: a frame without a vertical position stays in the flow, from the page or the margin too.
    if legacy && in_line(fr) {
        f.v_relative = "paragraph".into();
    }
    f.v_align = if fr.y.is_some() { y_code.map(str::to_string) } else { fr.y_align.clone().filter(|a| a != "inline") };
    f.v_offset = y_at.unwrap_or(0.0);
    f.wrap = match fr.wrap.as_deref() {
        Some("notBeside") => "topAndBottom",
        Some("none") => "none",
        _ => "square",
    }
    .into();
    f.dist = [fr.v_space, fr.v_space, fr.h_space + sides[0], fr.h_space + sides[1]];
}
