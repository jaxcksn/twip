//! Only the document pass reads the whole document: no algorithm below it takes a `Document`.

use super::layout_sources;

#[test]
fn algorithms_take_no_document() {
    const ALGORITHMS: [&str; 9] = ["text.rs", "line.rs", "paragraph.rs", "story.rs", "table.rs", "flow.rs", "page.rs", "float.rs", "traverse.rs"];
    let mut found = Vec::new();
    for (path, text) in layout_sources() {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !ALGORITHMS.contains(&name) || path.parent().is_some_and(|p| p.ends_with("tests")) {
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
