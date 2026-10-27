//! Frames (FRAME-1–FRAME-4): consecutive paragraphs with identical `w:framePr` become one text-box float anchored at
//! the start of the next paragraph, which then starts at the frame's top (the frame takes no space in the flow).

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
    // FRAME-3: paragraph borders enlarge the frame to the right (text at x, exclusion at x + w + both sides).
    let side = |b: &Option<super::props::Border>| b.as_ref().map_or(0.0, |b| b.space + b.width);
    let (lx, rx) = (side(&group[0].style.border_left), side(&group[0].style.border_right));
    // FRAME-4: a margin drop cap ends at the left margin.
    let margin_cap = fr.drop_cap.as_deref() == Some("margin");
    let start = group.first().and_then(|p| p.src.as_ref()).and_then(|s| s.ranges.first()).map(|r| r.start);
    let end = group.last().and_then(|p| p.src.as_ref()).and_then(|s| s.ranges.last()).map(|r| r.end);
    let frame_src = start.zip(end).map(|(a, b)| a..b);
    let blocks = group
        .into_iter()
        .map(|mut p| {
            let mut s = (*p.style).clone();
            s.frame = None;
            p.style = Rc::new(s);
            Block::Para(p)
        })
        .collect();
    let anchor = |a: Option<&str>, text: &str| match a {
        Some(a @ ("margin" | "page")) => a.to_string(),
        _ => text.to_string(),
    };
    Float {
        id: super::next_para_id(),
        w: fr.w.unwrap_or(0.0) + lx + rx,
        auto_w: fr.w.is_none_or(|w| w <= 0.0),
        h: fr.h.unwrap_or(0.0),
        image: None,
        alt: "".into(),
        lock_aspect: false,
        // FRAME-1: height = content (with h as a minimum) unless hRule="exact".
        text_box: Some(TextBox { blocks, insets: [0.0, 0.0, lx + rx, 0.0], outline: 0.0, auto_fit: fr.h_rule.as_deref() != Some("exact") }),
        h_relative: if margin_cap { "leftMargin".into() } else { anchor(fr.h_anchor.as_deref(), "column") },
        h_align: if margin_cap { Some("right".into()) } else { fr.x_align.clone() },
        h_offset: fr.x.unwrap_or(0.0),
        v_relative: anchor(fr.v_anchor.as_deref(), "paragraph"),
        v_align: fr.y_align.clone().filter(|a| a != "inline"),
        v_offset: fr.y.unwrap_or(0.0),
        // FRAME-3
        wrap: match fr.wrap.as_deref() {
            Some("notBeside") => "topAndBottom",
            Some("none") => "none",
            _ => "square",
        }
        .into(),
        wrap_text: "bothSides".into(),
        dist: [fr.v_space, fr.v_space, fr.h_space, fr.h_space],
        behind: false,
        allow_overlap: true,
        layout_in_cell: true,
        order: 0,
        src: None,
        locked: false,
        frame_src,
    }
}
