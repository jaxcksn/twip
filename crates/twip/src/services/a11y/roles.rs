//! Structure roles: headings, alternative text and header cells, as the reading tree and tagged PDF both derive them.

use crate::docx::{Document, Paragraph, Table};

/// A paragraph's heading level, 1 to 9: its outline level, else its style's level when that is a built-in heading
/// style ("heading 1" to "heading 9", whose level Word fixes by name).
pub(crate) fn heading_level(doc: &Document, p: &Paragraph) -> Option<u32> {
    if let Some(l) = p.style.outline_lvl {
        // Outline level 9 is body text.
        return (0..9).contains(&l).then_some(l as u32 + 1);
    }
    let style = doc.resolver.as_ref()?.styles.by_id.get(p.style.style_id.as_deref()?)?;
    let (word, n) = style.name.split_once(' ')?;
    let level: u32 = n.parse().ok()?;
    (word.eq_ignore_ascii_case("heading") && (1..=9).contains(&level)).then_some(level)
}

/// What a picture or shape says to a reader: its alternative text, nothing because it is decorative, or nothing
/// because it has no alternative text (an authoring gap, not a decision).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Alt<'a> {
    Text(&'a str),
    Decorative,
    Missing,
}

/// A picture's or shape's [`Alt`] as read.
pub(crate) fn alt<'a>(alt: &'a str, shape: Option<&crate::drawing::Shape>) -> Alt<'a> {
    if shape.is_some_and(|s| s.nv.decorative) {
        Alt::Decorative
    } else if alt.trim().is_empty() {
        Alt::Missing
    } else {
        Alt::Text(alt)
    }
}

/// The document's language: its defaults', else `language`.
pub(crate) fn document_language(doc: &Document, language: &str) -> String {
    doc.resolver.as_ref().and_then(|r| r.styles.doc_rpr.lang.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| language.to_string())
}

/// A run's language where it differs from the document's `doc_lang` (a run naming none is in the document's).
pub(crate) fn other_language<'a>(lang: &'a str, doc_lang: &str) -> Option<&'a str> {
    (!lang.is_empty() && !lang.eq_ignore_ascii_case(doc_lang)).then_some(lang)
}

/// What a header cell heads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderScope {
    Row,
    Column,
}

/// What each cell of row `r` of `t` heads: a header row's cells their columns; another row's cell that the row's
/// other cells list among their headers (`tcPr/w:headers`), its row.
pub(crate) fn header_scopes(t: &Table, r: usize) -> Vec<Option<HeaderScope>> {
    let Some(row) = t.rows.get(r) else { return Vec::new() };
    if row.header {
        return vec![Some(HeaderScope::Column); row.cells.len()];
    }
    let listed: std::collections::HashSet<&str> =
        row.cells.iter().filter_map(|c| c.ids.as_deref()).flat_map(|i| i.headers.iter().map(String::as_str)).collect();
    row.cells.iter().map(|c| c.ids.as_deref().and_then(|i| i.id.as_deref()).filter(|id| listed.contains(id)).map(|_| HeaderScope::Row)).collect()
}
