//! Generated export: each fixture read, its source anchors dropped so every paragraph, table, row and section, and
//! every style, document default, latent style, list definition, list and picture bullet is written from the model,
//! saved, read back and compared with the original. A report of what generation loses
//! today, not a gate: `TWIP_STRIPPED_OUT=file` writes it

use std::fmt::Write;

use super::super::Editor;
use super::super::inspect::inspect;
use super::fonts;

/// The document with no source anchors: blocks as a paste writes them, sections without their as-read page,
/// definitions with nothing as read.
fn stripped(d: &crate::docx::Document) -> crate::docx::Document {
    let mut d = d.clone();
    d.resolver = d.resolver.map(|r| {
        let t = r.typed();
        std::rc::Rc::new(r.unanchored(t.styles.root.clone(), t.numbering.root.clone()))
    });
    for s in &mut d.sections {
        s.blocks = s.blocks.iter().map(crate::docx::Block::fresh_identity).collect();
        s.origin = crate::docx::Origin::NONE;
    }
    d
}

/// The editor's document dump, then every laid-out page.
fn dump(e: &mut Editor) -> String {
    let v = inspect(e, "dump", true);
    let pages = crate::layout::dump::page_text(&crate::layout::layout(e.document(), fonts()), e.document());
    format!("{}\n{pages}", serde_json::to_string_pretty(&v["dump"]).expect("a dump"))
}

#[test]
fn documents_written_from_the_model_alone_read_back_as_they_were() {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    let (mut same, mut report) = (0, String::new());
    for path in &paths {
        let name = path.strip_prefix(crate::layout::tests::output::root()).unwrap_or(path).display().to_string();
        let Ok(mut e) = Editor::open(&std::fs::read(path).expect("reads"), fonts()) else { continue };
        let before = dump(&mut e);
        let bytes = match crate::docx::write::save(&stripped(e.document()), e.docx()) {
            Ok(b) => b,
            Err(err) => {
                writeln!(report, "{name}: not saved: {err}").unwrap();
                continue;
            }
        };
        let mut back = match Editor::open(&bytes, fonts()) {
            Ok(b) => b,
            Err(err) => {
                writeln!(report, "{name}: does not read back: {err}").unwrap();
                continue;
            }
        };
        let after = dump(&mut back);
        match before.lines().zip(after.lines()).position(|(a, b)| a != b) {
            None if before.lines().count() == after.lines().count() => same += 1,
            at => {
                let at = at.unwrap_or(before.lines().count().min(after.lines().count()));
                let line = |s: &str| s.lines().nth(at).unwrap_or("(end)").trim().chars().take(160).collect::<String>();
                writeln!(report, "{name}: differs at line {at}: {} -> {}", line(&before), line(&after)).unwrap();
            }
        }
    }
    writeln!(report, "{same} of {} fixtures read back as they were", paths.len()).unwrap();
    if let Some(out) = std::env::var_os("TWIP_STRIPPED_OUT") {
        std::fs::write(out, &report).expect("writes the report");
    }
    assert!(same > 0, "{report}");
}
