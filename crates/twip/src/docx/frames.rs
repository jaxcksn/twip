//! Frames

use std::rc::Rc;

use super::model::{Block, Document, Float, Inline, Paragraph, TextBox};

pub fn convert(doc: &mut Document) {
    for s in &mut doc.sections {
        blocks(&mut s.blocks);
    }
    for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
        blocks(b);
    }
}

fn key(p: &Paragraph) -> Option<&str> {
    p.style.frame.as_ref().map(|f| f.key.as_str())
}

fn blocks(list: &mut Vec<Block>) {
    let mut out = Vec::with_capacity(list.len());
    let mut groups: Vec<Vec<Paragraph>> = Vec::new();
    for b in std::mem::take(list) {
        match b {
            Block::Para(p) if key(&p).is_some() => match groups.last_mut() {
                Some(g) if key(&g[0]) == key(&p) => g.push(p),
                _ => groups.push(vec![p]),
            },
            Block::Para(mut p) => {
                for (k, g) in groups.drain(..).enumerate() {
                    p.inlines.insert(k, Inline::Float(Box::new(float(g)), p.mark.clone()));
                }
                out.push(Block::Para(p));
            }
            Block::Table(mut t) => {
                for r in &mut t.rows {
                    for c in &mut r.cells {
                        blocks(&mut c.blocks);
                    }
                }
                // shortcut: a frame before a table or at the end stays in the flow.
                out.extend(groups.drain(..).flatten().map(Block::Para));
                out.push(Block::Table(t));
            }
        }
    }
    out.extend(groups.into_iter().flatten().map(Block::Para));
    *list = out;
}

fn float(group: Vec<Paragraph>) -> Float {
    let fr = group[0].style.frame.clone().unwrap();
    let side = |b: &Option<super::props::Border>| b.as_ref().map_or(0.0, |b| b.space + b.width);
    let (lx, rx) = (side(&group[0].style.border_left), side(&group[0].style.border_right));
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
        .collect();
    let mut f = Float {
        id: super::next_para_id(),
        w: 0.0,
        auto_w: true,
        h: 0.0,
        image: None,
        alt: "".into(),
        lock_aspect: false,
        text_box: Some(TextBox { blocks, insets: [0.0, 0.0, lx + rx, 0.0], outline: 0.0, auto_fit: true, v_anchor: 0, flow }),
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
        parts: Vec::new(),
        shape: None,
        rot_swap: false,
        relative: Default::default(),
    };
    place(&mut f, &fr, lx + rx);
    f
}

/// Places a frame's float by its `framePr`: its size, position, wrapping and distance from text. `sides` is the
/// width its paragraphs' left and right borders add.
pub fn place(f: &mut Float, fr: &super::props::FramePr, sides: f64) {
    let margin_cap = fr.drop_cap.as_deref() == Some("margin");
    let anchor = |a: Option<&str>, text: &str, missing: &str| match a {
        Some(a @ ("margin" | "page")) => a.to_string(),
        Some(_) => text.to_string(),
        None => missing.to_string(),
    };
    // A frame without vAnchor is margin-relative; a drop cap sits in its paragraph whatever it says.
    let drop_cap = fr.drop_cap.as_deref().is_some_and(|d| d != "none");
    f.w = fr.w.unwrap_or(0.0) + sides;
    f.auto_w = fr.w.is_none_or(|w| w <= 0.0);
    f.h = fr.h.unwrap_or(0.0);
    if let Some(tb) = &mut f.text_box {
        tb.auto_fit = fr.h_rule.as_deref() != Some("exact");
    }
    f.h_relative = if margin_cap { "leftMargin".into() } else { anchor(fr.h_anchor.as_deref(), "column", "column") };
    f.h_align = if margin_cap { Some("right".into()) } else { fr.x_align.clone() };
    f.h_offset = fr.x.unwrap_or(0.0);
    f.v_relative = anchor(fr.v_anchor.as_deref(), "paragraph", if drop_cap { "paragraph" } else { "margin" });
    f.v_align = fr.y_align.clone().filter(|a| a != "inline");
    f.v_offset = fr.y.unwrap_or(0.0);
    f.wrap = match fr.wrap.as_deref() {
        Some("notBeside") => "topAndBottom",
        Some("none") => "none",
        _ => "square",
    }
    .into();
    f.dist = [fr.v_space, fr.v_space, fr.h_space, fr.h_space];
}
