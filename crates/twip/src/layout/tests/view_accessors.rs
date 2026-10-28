//! Layout reads run, paragraph, section and table-grid properties through the markup view's accessors (REV-2):
//! `run_in`, `para_style_in`, `mark_in`, `section_in` and `table_grid_in`. A scan of the layout input points for
//! direct reads, with the reads that are right as they are listed.

use std::path::Path;

/// The files whose reads are scanned.
const FILES: [&str; 9] = ["text.rs", "line.rs", "flow.rs", "table.rs", "story.rs", "page.rs", "document.rs", "node.rs", "incremental.rs"];

/// Direct reads of a paragraph's properties or mark, or a table's grid, by the receivers layout names them with.
const FIELDS: [&str; 8] = ["p.style", "para.style", "src.style", "n.style", "p.mark", "para.mark", "src.mark", "t.grid"];

/// A section's page properties: read through `section_in`, never off a `Section`.
const PAGE: [&str; 11] = ["page_w", "page_h", "top", "bottom", "left", "right", "header", "footer", "gutter", "columns", "title_pg"];

/// Direct reads that are right: (file, the line's code, why).
const ALLOWED: [(&str, &str, &str); 10] = [
    ("text.rs", "self.style_index(p.style)", "a piece's style index"),
    ("text.rs", "self.east_asia_face(p.style)", "a piece's style index"),
    ("table.rs", "grid == t.grid.as_slice()", "`in_view` compares the view's grid with the model's"),
    ("document.rs", "let mut st = (*p.style).clone();", "the endnote separator's own copy, made before layout"),
    ("document.rs", "p.style = Rc::new(st);", "the endnote separator's own copy, made before layout"),
    ("document.rs", "p.style.style_id.as_deref()", "STYLEREF looks paragraphs up by their style as it is"),
    ("table.rs", "Some((ParaId(p.id), p.mark.size))", "the caret height at a cell's closing paragraph, for editing"),
    ("table.rs", "*grid == t.grid", "the table cache compares the model"),
    ("table.rs", "t.grid.len().max(", "the shown table's grid (`in_view` applied `table_grid_in`)"),
    ("table.rs", "t.grid.get(i)", "the shown table's grid (`in_view` applied `table_grid_in`)"),
];

fn ident(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// The direct reads on one line of code.
fn reads(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for f in FIELDS {
        for (at, _) in code.match_indices(f) {
            let rest = &code[at + f.len()..];
            // A piece's style index, and the revision marks themselves (no view changes them).
            let fine = rest.starts_with(" as ") || rest.starts_with(".rev") || rest.starts_with(".change");
            if !ident(code[..at].chars().next_back()) && !ident(rest.chars().next()) && !fine {
                out.push(f.to_string());
            }
        }
    }
    for recv in [".section.", "sections[si]."] {
        for (at, _) in code.match_indices(recv) {
            let rest = &code[at + recv.len()..];
            if let Some(f) = PAGE.iter().find(|f| rest.starts_with(**f) && !ident(rest[f.len()..].chars().next())) {
                out.push(format!("{recv}{f}"));
            }
        }
    }
    out
}

#[test]
fn layout_reads_properties_through_the_view() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/layout");
    let mut found = Vec::new();
    for file in FILES {
        let text = std::fs::read_to_string(dir.join(file)).expect("a layout source");
        // The file's tests aren't layout input points.
        let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or(&text);
        for (n, line) in code.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if ALLOWED.iter().any(|(f, l, _)| *f == file && code.contains(l)) {
                continue;
            }
            for r in reads(code) {
                found.push(format!("{file}:{}: {r} ({})", n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "direct property reads where the view's accessor is needed:\n{}", found.join("\n"));
}

#[test]
fn the_scan_finds_direct_reads() {
    assert_eq!(reads("let s = &p.style;"), ["p.style"]);
    assert_eq!(reads("f(&para.mark, x)"), ["para.mark"]);
    assert_eq!(reads("let n = self.sec.section.columns.num;"), [".section.columns"]);
    assert!(reads("sp.styles[p.style as usize]").is_empty());
    assert!(reads("p.para_style_in(view)").is_empty());
    assert!(reads("src.style.change.is_some()").is_empty());
}
