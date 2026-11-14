//! Algorithm signatures

use std::path::Path;

use super::layout_sources;

#[test]
fn algorithms_take_no_document() {
    const ALGORITHMS: [&str; 9] = ["text", "line", "paragraph", "story", "table", "flow", "page", "float", "traverse"];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("layout");
    let mut found = Vec::new();
    for (path, text) in layout_sources() {
        let module = path.strip_prefix(&root).ok().and_then(|p| p.components().next()).and_then(|c| c.as_os_str().to_str());
        let Some(module) = module.map(|m| m.strip_suffix(".rs").unwrap_or(m)) else { continue };
        if !ALGORITHMS.contains(&module) {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("Document") {
                found.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "algorithms that see the document:\n{}", found.join("\n"));
}
