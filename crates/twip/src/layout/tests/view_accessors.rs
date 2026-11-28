//! Markup view accessors

use std::path::Path;

use super::layout_sources;

const MODULES: [&str; 9] = ["text", "line", "flow", "table", "story", "page", "document", "node", "incremental"];

const FIELDS: [&str; 8] = ["p.style", "para.style", "src.style", "n.style", "p.mark", "para.mark", "src.mark", "t.grid"];

const PAGE: [&str; 11] = ["page_w", "page_h", "top", "bottom", "left", "right", "header", "footer", "gutter", "columns", "title_pg"];

const ALLOWED: [(&str, &str, &str); 11] = [
    ("text", "self.style_index(p.style)", "a piece's style index"),
    ("text", "self.east_asia_face(p.style)", "a piece's style index"),
    ("table", "grid == t.grid.as_slice()", "`in_view` compares the view's grid with the model's"),
    ("document", "let mut st = (*p.style).clone();", "the endnote separator's own copy, made before layout"),
    ("document", "p.style = Rc::new(st);", "the endnote separator's own copy, made before layout"),
    ("document", "p.style.style_id.as_deref()", "STYLEREF looks paragraphs up by their style as it is"),
    ("table", "Some((ParaId(p.id), p.mark.size))", "the caret height at a cell's closing paragraph, for editing"),
    ("table", "*grid == t.grid", "the table cache compares the model"),
    ("table", "t.grid.len().max(", "the shown table's grid (`in_view` applied `table_grid_in`)"),
    ("table", "t.grid.get(i)", "the shown table's grid (`in_view` applied `table_grid_in`)"),
    ("table", "t.grid = bounds.windows(2)", "a layout run's own grid, built only when the table has no grid change"),
];

fn ident(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

fn reads(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for f in FIELDS {
        for (at, _) in code.match_indices(f) {
            let rest = &code[at + f.len()..];
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("layout");
    let mut found = Vec::new();
    let mut seen = Vec::new();
    for (path, text) in layout_sources() {
        let module = path.strip_prefix(&root).ok().and_then(|p| p.components().next()).and_then(|c| c.as_os_str().to_str());
        let Some(module) = module.map(|m| m.strip_suffix(".rs").unwrap_or(m)).filter(|m| MODULES.contains(m)) else { continue };
        seen.push(module.to_string());
        let code = text.split("#[cfg(test)]\nmod tests").next().unwrap_or(&text);
        for (n, line) in code.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if ALLOWED.iter().any(|(m, l, _)| *m == module && code.contains(l)) {
                continue;
            }
            for r in reads(code) {
                found.push(format!("{}:{}: {r} ({})", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(MODULES.iter().all(|m| seen.iter().any(|s| s == m)), "a layout module not found: {seen:?}");
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

#[test]
fn a_field_value_shows_its_revisions_text_mark() {
    use crate::docx::review::TextMark;
    let fld = |t: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{t}"/></w:r>"#);
    let ins = r#"w:id="9" w:author="a" w:date="2026-01-01T00:00:00Z""#;
    let body = format!(
        r#"<w:p><w:r><w:t>Z</w:t></w:r><w:ins {ins}>{}<w:r><w:instrText xml:space="preserve"> DATE \@ "dd MMMM yyyy" </w:instrText></w:r>{}<w:r><w:t>x</w:t></w:r>{}</w:ins><w:r><w:t>Q</w:t></w:r></w:p>"#,
        fld("begin"),
        fld("separate"),
        fld("end")
    );
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).expect("a package");
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..z.len() {
        use std::io::Read;
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).expect("reads");
        if name == "word/document.xml" {
            data = String::from_utf8(data).expect("utf-8").replacen("<w:p/>", &body, 1).into_bytes();
        }
        parts.push((name, data));
    }
    let bytes = crate::docx::write::repackage(&crate::docx::blank::blank_docx(), &parts, zip::CompressionMethod::Deflated).expect("repackages");
    let d = crate::docx::read_at(&bytes, 1_794_096_000).expect("reads");
    let after = |view: &crate::docx::review::ReviewView| {
        super::page_chars_in(&d, view)[0].iter().find(|c| c.0 == 'Q').map(|c| c.1).expect("the text after the field")
    };
    let mut view = super::view(&d);
    let plain = after(&view);
    std::rc::Rc::make_mut(&mut view.markup).style.insertions.mark = TextMark::Bold;
    assert!(after(&view) > plain, "the field's value is bold like the rest of the insertion");
}
