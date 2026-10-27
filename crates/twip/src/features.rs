//! Which layout features a document uses, so tools and tests can limit a comparison to documents within a feature
//! set (`dumpdiff --features`, `golden --features`, the layout tests' gate sets).

use std::collections::BTreeSet;

use crate::docx::props::{Jc, VertAlign};
use crate::docx::{Block, Document, FieldKind, Inline, NoteKind, Paragraph, Run};

/// Every feature name [`of`] can report, in the order the CLI lists them.
pub const ALL: &[&str] = &[
    "tables",
    "nested_tables",
    "footnotes",
    "endnotes",
    "floats",
    "frames",
    "columns",
    "sections",
    "header_footer",
    "page_fields",
    "line_numbers",
    "justified",
    "tabs",
    "lists",
    "vertical",
    "para_borders",
    "compat_non15",
    "images",
    "valign",
    "drop_caps",
    "hidden",
];

/// The features `doc` uses: its body, notes, headers and footers, and text boxes.
pub fn of(doc: &Document) -> BTreeSet<&'static str> {
    let mut f = BTreeSet::new();
    if doc.sections.len() > 1 {
        f.insert("sections");
    }
    for s in &doc.sections {
        if s.columns.num > 1 || s.columns.cols.len() > 1 {
            f.insert("columns");
        }
        if s.line_numbers.is_some() {
            f.insert("line_numbers");
        }
        if !s.v_align.is_empty() && s.v_align != "top" {
            f.insert("valign");
        }
        let referenced = s.headers.iter().chain(&s.footers).flatten();
        if referenced.into_iter().any(|name| doc.stories.get(name).is_some_and(|b| !b.is_empty())) {
            f.insert("header_footer");
        }
        blocks(&s.blocks, 0, &mut f);
    }
    if doc.settings.compat_mode != 15 || !doc.settings.compat.is_empty() {
        f.insert("compat_non15");
    }
    for b in doc.stories.values().chain(doc.footnotes.values()).chain(doc.endnotes.values()) {
        blocks(b, 0, &mut f);
    }
    f
}

fn blocks(bs: &[Block], depth: usize, f: &mut BTreeSet<&'static str>) {
    for b in bs {
        match b {
            Block::Para(p) => para(p, depth, f),
            Block::Table(t) => {
                f.insert("tables");
                if depth > 0 {
                    f.insert("nested_tables");
                }
                for c in t.rows.iter().flat_map(|r| &r.cells) {
                    blocks(&c.blocks, depth + 1, f);
                }
            }
        }
    }
}

fn para(p: &Paragraph, depth: usize, f: &mut BTreeSet<&'static str>) {
    let st = &p.style;
    if matches!(st.jc, Jc::Both | Jc::Distribute) {
        f.insert("justified");
    }
    if [&st.border_top, &st.border_bottom, &st.border_left, &st.border_right, &st.border_between].iter().any(|b| b.is_some()) {
        f.insert("para_borders");
    }
    if let Some(frame) = &st.frame {
        f.insert("frames");
        if frame.drop_cap.as_deref().is_some_and(|d| d != "none") {
            f.insert("drop_caps");
        }
    }
    if let Some(list) = &p.list {
        f.insert("lists");
        if list.suff == "tab" || list.suff.is_empty() {
            f.insert("tabs");
        }
    }
    run(&p.mark, f);
    for i in &p.inlines {
        match i {
            Inline::Text(_, r) | Inline::Break(_, r) | Inline::Sym { run: r, .. } | Inline::NoteMark(_, _, r) | Inline::Separator { run: r, .. } => {
                run(r, f)
            }
            Inline::Tab(r) | Inline::PTab(_, r) => {
                f.insert("tabs");
                run(r, f);
            }
            Inline::Picture(_, r) => {
                f.insert("images");
                run(r, f);
            }
            Inline::Float(fl, r) => {
                f.insert("floats");
                if fl.image.is_some() {
                    f.insert("images");
                }
                if let Some(tb) = &fl.text_box {
                    blocks(&tb.blocks, depth, f);
                }
                run(r, f);
            }
            Inline::NoteRef { kind, run: r, .. } => {
                f.insert(match kind {
                    NoteKind::Foot => "footnotes",
                    NoteKind::End => "endnotes",
                });
                run(r, f);
            }
            Inline::FieldBegin { kind, run: r, .. } => {
                if let Some(
                    FieldKind::Page
                    | FieldKind::NumPages
                    | FieldKind::SectionPages
                    | FieldKind::Section
                    | FieldKind::PageRef(_)
                    | FieldKind::StyleRef(_),
                ) = kind
                {
                    f.insert("page_fields");
                }
                run(r, f);
            }
            Inline::FieldSep { run: r, .. } | Inline::FieldEnd { run: r, .. } => run(r, f),
            Inline::Bookmark(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) => {}
        }
    }
}

fn run(r: &Run, f: &mut BTreeSet<&'static str>) {
    if r.vert_align != VertAlign::Baseline || r.position != 0.0 {
        f.insert("vertical");
    }
    if r.hidden {
        f.insert("hidden");
    }
}

/// Parses a `--features` list (`a,b,c`; empty = no features). Unknown names are errors, so a typo can't widen a gate.
pub fn parse(list: &str) -> Result<BTreeSet<&'static str>, String> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|name| ALL.iter().copied().find(|a| *a == name).ok_or_else(|| format!("unknown feature {name:?}; known: {}", ALL.join(","))))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_rejects_unknown_names() {
        assert!(super::parse("").unwrap().is_empty());
        assert_eq!(super::parse("tables, lists").unwrap().len(), 2);
        assert!(super::parse("table").is_err());
    }
}
