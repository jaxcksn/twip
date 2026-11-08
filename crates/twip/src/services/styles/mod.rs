//! The document's styles for a style gallery or list, with their preview formatting resolved through the style chain.

pub mod builtin;
pub mod describe;

use crate::docx::styles::Styles;

/// Why a name can't be a new style's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRefusal {
    Empty,
    /// Another style's name or alias.
    Duplicate,
    /// A built-in style's name.
    Reserved,
}

/// Whether a style other than `except` may take `name` (compared without case, as aliases too).
pub fn check_name(styles: &Styles, name: &str, except: Option<&str>) -> Result<(), NameRefusal> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameRefusal::Empty);
    }
    let same = |n: &str| n.trim().eq_ignore_ascii_case(name);
    if styles.by_id.values().any(|s| Some(s.id.as_str()) != except && (same(&s.name) || s.aliases.iter().any(|a| same(a)))) {
        return Err(NameRefusal::Duplicate);
    }
    match builtin::built_in(name) {
        Some(b) if except.is_none_or(|e| e != b.id) => Err(NameRefusal::Reserved),
        _ => Ok(()),
    }
}

/// The first `StyleN` name no style has.
pub fn next_new_name(styles: &Styles) -> String {
    (1..).map(|n| format!("Style{n}")).find(|n| check_name(styles, n, None).is_ok()).unwrap_or_default()
}

use crate::api::services::{StyleInfo, StylePreview, StyleType, StyleVisibility};
use crate::api::value::{AutoOr, Color, Points, StyleId};
use crate::docx::Document;
use crate::docx::props::RunProps;
use crate::docx::styles::StyleKind;

/// The paragraph, character, table and list styles (`kind` only, when given), by priority then name.
pub fn list(doc: &Document, kind: Option<StyleType>, glossary: bool) -> Vec<StyleInfo> {
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
                StyleKind::Numbering => StyleType::List,
            };
            if kind.is_some_and(|k| k != t) {
                return None;
            }
            // A style only a sub-template's glossary defines is offered while that sub-template is edited.
            if s.origin == crate::docx::styles::StyleOrigin::Glossary && !glossary {
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
                hidden: s.hidden(),
                based_on: s.based_on.clone().filter(|b| styles.by_id.contains_key(b)).map(StyleId),
                preview: StylePreview {
                    font: run.ascii.clone(),
                    size: Points(run.size),
                    bold: run.bold,
                    italic: run.italic,
                    color: run.color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))),
                },
                aliases: s.aliases.clone(),
                link: s.link.clone().filter(|l| styles.by_id.contains_key(l)).map(StyleId),
                visibility: match (s.meta.semi_hidden, s.meta.unhide_when_used) {
                    (true, true) => StyleVisibility::HideUntilUsed,
                    (true, false) => StyleVisibility::Hide,
                    (false, _) => StyleVisibility::Show,
                },
                locked: s.meta.locked,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.priority, &a.name).cmp(&(b.priority, &b.name)));
    out
}

fn title_case(s: &str) -> String {
    s.split(' ').map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect::<String>()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{NameRefusal, check_name, next_new_name};

    fn styles() -> crate::docx::styles::Styles {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
        let doc = crate::docx::read_editable(&std::fs::read(path).expect("FX-STYLES")).expect("reads");
        doc.resolver.expect("a resolver").styles.clone()
    }

    #[test]
    fn new_names_skip_used_names_aliases_and_built_ins() {
        let s = styles();
        assert_eq!(next_new_name(&s), "Style1");
        assert_eq!(check_name(&s, "  ", None), Err(NameRefusal::Empty));
        assert_eq!(check_name(&s, "callout", None), Err(NameRefusal::Duplicate));
        assert_eq!(check_name(&s, "MB", None), Err(NameRefusal::Duplicate), "an alias");
        assert_eq!(check_name(&s, "Heading 4", None), Err(NameRefusal::Reserved), "a built-in the document doesn't define");
        assert_eq!(check_name(&s, "Callout", Some("Callout")), Ok(()), "a style keeps its own name");
        assert_eq!(check_name(&s, "Sidebar", None), Ok(()));
        assert_eq!(super::builtin::built_in("annotation text").map(|b| b.id), Some("CommentText"));
    }
}
