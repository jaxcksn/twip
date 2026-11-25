//! Native editing tests

mod authored;
mod backgrounds;
mod baseline;
mod basics;
mod bindings;
mod checkboxes;
mod click_type;
mod clipboard;
mod command_gating;
mod comments;
mod content_controls;
mod cursors;
mod custom_lists;
mod depth;
mod edit_cost;
mod empty_stories;
mod field_control_edits;
mod field_edges;
mod field_shading;
mod fields;
mod find_replace;
mod form_fields;
mod formatting;
mod fuzz;
mod go_to;
mod history;
mod hover;
mod into_tables;
mod json;
mod keys;
mod list_definitions;
mod marks;
mod merge;
mod multisel;
mod navigation;
mod objects;
mod outline;
mod page_background;
mod paste;
mod perf;
mod picture_controls;
mod protection;
mod protection_ranges;
mod provisional;
mod readback;
pub(crate) mod recorder;
mod round_trips;
mod saving;
mod saving_objects;
pub(crate) mod scenarios;
mod search;
mod sections;
mod services;
mod shapes;
mod snap;
mod steps;
mod stories;
mod stories_and_steps;
mod style_definitions;
mod style_model;
mod styles;
mod sub_template_settings;
mod sub_templates;
mod table_dialogs;
mod table_handles;
mod table_structure;
mod table_tools;
mod text_units;
mod toc;
mod tracked_structure;
mod transactions;
pub(crate) mod vertical;

use std::path::Path;

use crate::docx::Block;
use crate::font::Fonts;

use super::Editor;
use super::text::UnitText;

pub(crate) use super::commands::{deleting as del, moving as mv};

pub(crate) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len()).max(1);
    let mut done: Vec<(usize, R)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Relaxed);
                        let Some(item) = items.get(i) else { break done };
                        done.push((i, f(item)));
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a worker panicked")).collect()
    });
    done.sort_by_key(|d| d.0);
    done.into_iter().map(|d| d.1).collect()
}

pub fn fonts() -> &'static Fonts {
    // One per thread over the test kit's shared catalog, so threads share no font memos.
    thread_local! {
        static FONTS: &'static Fonts = Box::leak(Box::new(Fonts::system_recommended()));
    }
    FONTS.with(|f| *f)
}

pub fn open(name: &str) -> Editor {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()));
    Editor::open(&bytes, fonts()).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

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

pub fn blank_with(text: &str) -> Vec<u8> {
    blank_in(text, "")
}

pub fn blank_in(text: &str, font: &str) -> Vec<u8> {
    let rpr = if font.is_empty() { String::new() } else { format!("<w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\"/></w:rPr>") };
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;");
    let paras: String = text.split('\n').map(|l| format!("<w:p><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", esc(l))).collect();
    blank_body(&paras)
}

/// The blank document with `paras` (body XML) for its paragraph.
pub fn blank_body(paras: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = String::from_utf8(data).unwrap().replacen("<w:p/>", paras, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}
