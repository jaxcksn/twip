//! Layout depends on no renderer, editor or adapter code.

use super::layout_sources;

#[test]
fn layout_has_no_forbidden_imports() {
    const FORBIDDEN: [&str; 6] = ["skia_safe", "crate::editor", "crate::legacy", "crate::web", "crate::golden", "crate::cli"];
    let sources = layout_sources();
    assert!(!sources.is_empty(), "layout sources not found");
    let mut found = Vec::new();
    for (path, text) in &sources {
        // This file names the forbidden paths itself.
        if path.ends_with("imports.rs") {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for f in FORBIDDEN {
                if code.contains(f) {
                    found.push(format!("{}:{}: {f}", path.display(), n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "layout imports code it mustn't:\n{}", found.join("\n"));
}
