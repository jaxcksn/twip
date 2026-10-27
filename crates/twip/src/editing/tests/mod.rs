//! Native editing tests. Fixtures that are missing fail.

mod basics;
mod depth;
mod fuzz;
mod merge;
mod perf;
pub(crate) mod recorder;
pub(crate) mod scenarios;
mod steps;

use std::path::Path;

use crate::docx::Block;
use crate::font::Fonts;

use super::Editor;
use super::text::UnitText;

/// The system fonts, loaded once per test thread (loading them takes most of a test's time; `Fonts` isn't `Sync`).
pub fn fonts() -> &'static Fonts {
    thread_local! {
        static FONTS: &'static Fonts = Box::leak(Box::new(Fonts::system()));
    }
    FONTS.with(|f| *f)
}

/// Opens an editor fixture (`tests/fixtures/editor/<name>`).
pub fn open(name: &str) -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()));
    Editor::open(&bytes, fonts()).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

/// Every body paragraph's id in document order (cells included).
pub fn texts_ids(e: &Editor) -> Vec<crate::layout::ParaId> {
    fn walk(blocks: &[Block], out: &mut Vec<crate::layout::ParaId>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(crate::layout::ParaId(p.id)),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    out
}

/// The text of every body paragraph in document order (cells included), one string each.
pub fn texts(e: &Editor) -> Vec<String> {
    fn walk(blocks: &[Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(UnitText::of(p).text),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    for s in &e.document().sections {
        walk(&s.blocks, &mut out);
    }
    out
}

/// The id of body paragraph `n` in document order (cells included).
pub fn para_id(e: &Editor, n: usize) -> crate::layout::ParaId {
    fn walk(blocks: &[Block], out: &mut Vec<u64>) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p.id),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    e.document().sections.iter().for_each(|s| walk(&s.blocks, &mut out));
    crate::layout::ParaId(out[n])
}

/// A blank document whose body is `text`, one paragraph per line (Calibri 11 pt, US Letter).
pub fn blank_with(text: &str) -> Vec<u8> {
    blank_in(text, "")
}

/// [`blank_with`], every run in `font` (empty: the default).
pub fn blank_in(text: &str, font: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let rpr = if font.is_empty() { String::new() } else { format!("<w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\"/></w:rPr>") };
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;");
    let paras: String = text.split('\n').map(|l| format!("<w:p><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", esc(l))).collect();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = String::from_utf8(data).unwrap().replacen("<w:p/>", &paras, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}
