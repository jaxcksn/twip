//! The document's styles for a style gallery or list, with their preview formatting resolved through the style chain.

pub mod builtin;
pub mod describe;

use crate::format::{StyleKind, StyleSheet};

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
pub fn check_name(styles: &StyleSheet, name: &str, except: Option<&str>) -> Result<(), NameRefusal> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameRefusal::Empty);
    }
    let same = |n: &str| n.trim().eq_ignore_ascii_case(name);
    let named = |s: &crate::format::Style| same(s.name.as_deref().unwrap_or_default()) || s.aliases.iter().any(|a| same(a));
    if styles.styles.values().any(|e| Some(&*e.style.id) != except && named(&e.style)) {
        return Err(NameRefusal::Duplicate);
    }
    match builtin::built_in(name) {
        Some(b) if except.is_none_or(|e| e != b.id) => Err(NameRefusal::Reserved),
        _ => Ok(()),
    }
}

/// The first `StyleN` name no style has.
pub fn next_new_name(styles: &StyleSheet) -> String {
    (1..).map(|n| format!("Style{n}")).find(|n| check_name(styles, n, None).is_ok()).unwrap_or_default()
}

use crate::api::services::{StyleInfo, StylePreview, StyleType, StyleVisibility};
use crate::api::value::{AutoOr, Color, Points, StyleId};
use crate::docx::Document;
use crate::docx::style_xml::StyleDef;
use crate::format::CharProps;

/// The style type of a style of `kind`.
fn style_type(kind: StyleKind) -> StyleType {
    match kind {
        StyleKind::Paragraph => StyleType::Paragraph,
        StyleKind::Character => StyleType::Character,
        StyleKind::Table => StyleType::Table,
        StyleKind::Numbering => StyleType::List,
    }
}

/// The typed kind of a built-in style of `kind`.
pub fn typed_kind(kind: crate::docx::styles::StyleKind) -> StyleKind {
    use crate::docx::styles::StyleKind as K;
    match kind {
        K::Paragraph => StyleKind::Paragraph,
        K::Character => StyleKind::Character,
        K::Table => StyleKind::Table,
        K::Numbering => StyleKind::Numbering,
    }
}

/// A gallery preview of a run whose levels fold to `p`.
fn preview(c: &crate::format::cascade::Cascade, p: &CharProps) -> StylePreview {
    let run = c.run_style(&c.run_props(p, &CharProps::default()));
    StylePreview {
        font: run.ascii.clone(),
        size: Points(run.size),
        bold: run.bold,
        italic: run.italic,
        color: run.color.map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c))),
    }
}

/// The paragraph, character, table and list styles (`kind` only, when given), by priority then name.
pub fn list(doc: &Document, kind: Option<StyleType>, glossary: bool) -> Vec<StyleInfo> {
    let Some(r) = doc.resolver.as_deref() else { return Vec::new() };
    let typed = r.typed();
    let c = &*typed.cascade;
    let styles = &*c.styles;
    let mut out: Vec<StyleInfo> = styles
        .styles
        .values()
        .filter_map(|e| {
            let s = &*e.style;
            let t = style_type(s.kind);
            if kind.is_some_and(|k| k != t) {
                return None;
            }
            // A style only a sub-template's glossary defines is offered while that sub-template is edited.
            if e.origin == crate::format::Origin::Glossary && !glossary {
                return None;
            }
            // A run with no direct formatting under the document defaults and this style's chain, as text resolves.
            let mut base = c.doc_char().clone();
            base.merge_level(&c.chain_char(Some(&s.id)));
            let meta = |v: Option<bool>| v.unwrap_or(false);
            let id = |v: &Option<std::rc::Rc<str>>| v.as_deref().filter(|b| styles.styles.contains_key(*b)).map(|b| StyleId(b.to_string()));
            let name = describe::display_name(s);
            Some(StyleInfo {
                id: StyleId(s.id.to_string()),
                name: if name.is_empty() { s.id.to_string() } else { name },
                kind: t,
                built_in: !meta(s.custom),
                quick: meta(s.quick),
                // The priority of styles that give none.
                priority: s.priority.unwrap_or(99),
                hidden: meta(s.meta.semi_hidden) || meta(s.meta.hidden),
                based_on: id(&c.based_on(e).map(std::rc::Rc::from)),
                preview: preview(c, &base),
                aliases: s.aliases.iter().map(|a| a.to_string()).collect(),
                link: id(&s.link),
                visibility: match (meta(s.meta.semi_hidden), meta(s.meta.unhide_when_used)) {
                    (true, true) => StyleVisibility::HideUntilUsed,
                    (true, false) => StyleVisibility::Hide,
                    (false, _) => StyleVisibility::Show,
                },
                locked: meta(s.meta.locked),
                uses: 0,
            })
        })
        .collect();
    // The gallery's built-in styles the document doesn't define yet: applying one adds it.
    let cx = typed.styles.cx();
    for b in builtin::BUILT_IN.iter().filter(|b| b.quick && !styles.styles.contains_key(b.id)) {
        let t = style_type(typed_kind(b.kind));
        if kind.is_some_and(|k| k != t) {
            continue;
        }
        let def = StyleDef::from_xml(&builtin::definition(b, &cx.w), &cx);
        let s = def.get();
        let mut base = c.doc_char().clone();
        base.merge_level(&c.chain_char(s.based_on.as_deref()));
        base.merge_level(&s.format.char);
        out.push(StyleInfo {
            id: StyleId(b.id.to_string()),
            name: if b.name.chars().any(char::is_uppercase) { b.name.to_string() } else { title_case(b.name) },
            kind: t,
            built_in: true,
            quick: true,
            priority: b.priority,
            hidden: false,
            based_on: s.based_on.as_deref().filter(|x| styles.styles.contains_key(*x)).map(|x| StyleId(x.to_string())),
            preview: preview(c, &base),
            aliases: Vec::new(),
            link: None,
            visibility: match (b.semi_hidden, b.unhide_when_used) {
                (true, true) => StyleVisibility::HideUntilUsed,
                (true, false) => StyleVisibility::Hide,
                (false, _) => StyleVisibility::Show,
            },
            locked: false,
            uses: 0,
        });
    }
    out.sort_by(|a, b| (a.priority, &a.name).cmp(&(b.priority, &b.name)));
    out
}

fn title_case(s: &str) -> String {
    s.split(' ').map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect::<String>()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{NameRefusal, check_name, next_new_name};

    fn styles() -> crate::format::StyleSheet {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
        let doc = crate::docx::read_editable(&std::fs::read(path).expect("FX-STYLES")).expect("reads");
        (*doc.resolver.expect("a resolver").cascade().styles).clone()
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
