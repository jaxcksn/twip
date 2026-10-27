//! Subsystem layering: docx → font → layout → editing →
//! {render, pdf} → host. A source scan, because the rules are about which modules name which.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every `crate::<module>...` path named in the files under `src/<module>` (comments included: a doc link is a
/// dependency too), as `(file, path)`.
fn crate_paths(module: &str) -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root.join(module), &mut files);
    let mut out = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).unwrap_or_default();
        // A module's tests may compose layers (open a document to export it); only its code is layered.
        let text = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (i, _) in text.match_indices("crate::") {
            // A string literal naming a path (an import test's list) isn't a dependency.
            if text[..i].ends_with('"') {
                continue;
            }
            let path: String = text[i + 7..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':').collect();
            out.push((f.strip_prefix(&root).unwrap_or(&f).display().to_string(), path));
        }
    }
    out
}

fn violations(module: &str, forbidden: &[&str], allowed: &[&str]) -> Vec<String> {
    crate_paths(module)
        .into_iter()
        .filter(|(_, p)| forbidden.iter().any(|f| p == f || p.starts_with(&format!("{f}::"))))
        .filter(|(_, p)| !allowed.iter().any(|a| p == a || p.starts_with(&format!("{a}::"))))
        .map(|(f, p)| format!("{f}: crate::{p}"))
        .collect()
}

#[test]
fn editing_stays_below_rendering_and_host() {
    let v = violations("editing", &["render", "pdf", "host", "editor", "legacy"], &[]);
    assert!(v.is_empty(), "editing imports a higher or legacy layer:\n{}", v.join("\n"));
}

#[test]
fn layout_doesnt_know_the_editor() {
    let v = violations("layout", &["editing", "editor", "legacy", "render", "pdf", "host"], &[]);
    assert!(v.is_empty(), "layout imports a consumer:\n{}", v.join("\n"));
}

#[test]
fn renderers_read_only_the_view() {
    for m in ["render", "pdf"] {
        let v = violations(m, &["editing", "editor", "legacy", "host"], &["editing::view"]);
        assert!(v.is_empty(), "{m} reads editing internals:\n{}", v.join("\n"));
    }
}
