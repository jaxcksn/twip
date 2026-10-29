//! The document's styles for a style gallery or list, with their preview formatting resolved through the style chain.

use crate::api::services::{StyleInfo, StylePreview, StyleType};
use crate::api::value::{AutoOr, Color, Points, StyleId};
use crate::docx::Document;
use crate::docx::props::RunProps;
use crate::docx::styles::StyleKind;

/// The paragraph, character and table styles (`kind` only, when given), by priority then name. Numbering styles
/// aren't listed.
pub fn list(doc: &Document, kind: Option<StyleType>) -> Vec<StyleInfo> {
    let Some(r) = doc.resolver.as_deref() else { return Vec::new() };
    let styles = &r.styles;
    let mut out: Vec<StyleInfo> = styles
        .by_id
        .values()
        .filter_map(|s| {
            let t = match s.kind {
                StyleKind::Paragraph => StyleType::Paragraph,
                StyleKind::Character => StyleType::Character,
                StyleKind::Table => StyleType::Table,
                StyleKind::Numbering => return None,
            };
            if kind.is_some_and(|k| k != t) {
                return None;
            }
            // A run with no direct formatting under the document defaults and this style's chain, as text resolves.
            let mut base = styles.doc_rpr.clone();
            base.merge_level(&styles.chain_rpr(Some(&s.id)));
            let run = r.run_style(&r.run_props(&base, &RunProps::default()));
            // Built-in styles are named in lower case in the file ("heading 1"); lists show them capitalised.
            let name = if !s.custom && !s.name.chars().any(char::is_uppercase) { title_case(&s.name) } else { s.name.clone() };
            Some(StyleInfo {
                id: StyleId(s.id.clone()),
                name: if name.is_empty() { s.id.clone() } else { name },
                kind: t,
                built_in: !s.custom,
                quick: s.quick,
                // The priority of styles that give none.
                priority: s.priority.unwrap_or(99),
                hidden: s.hidden,
                based_on: s.based_on.clone().filter(|b| styles.by_id.contains_key(b)).map(StyleId),
                preview: StylePreview {
                    font: run.ascii.clone(),
                    size: Points(run.size),
                    bold: run.bold,
                    italic: run.italic,
                    color: run.color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))),
                },
            })
        })
        .collect();
    out.sort_by(|a, b| (a.priority, &a.name).cmp(&(b.priority, &b.name)));
    out
}

fn title_case(s: &str) -> String {
    s.split(' ').map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect::<String>()).collect::<Vec<_>>().join(" ")
}
