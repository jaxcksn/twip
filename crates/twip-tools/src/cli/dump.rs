//! `twip dump <out dir> <docx or folders...>`: writes one dump file per document and an index, for comparing
//! builds of the layout engine item by item. The item text itself is [`twip::internal::layout::dump::page_text`].

use std::fmt::Write;
use std::path::{Path, PathBuf};

use twip::internal::docx::Document;

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path).into_iter().flatten().flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            collect(&e, out);
        }
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("docx")) {
        out.push(path.to_path_buf());
    }
}

/// Dumps each document into `out/<n>-<name>.txt`, with `text` giving a read document's dump, plus `index.txt`.
/// Returns an error message if the output can't be written.
pub fn run(out: &Path, paths: &[String], text: impl Fn(&Document) -> String) -> Result<usize, String> {
    let mut files = Vec::new();
    for p in paths {
        collect(Path::new(p), &mut files);
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut index = String::new();
    for (n, file) in files.iter().enumerate() {
        let name = format!("{n:04}-{}.txt", file.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default());
        let Ok(bytes) = std::fs::read(file) else { continue };
        let dump = match twip::internal::docx::read(&bytes) {
            Ok(doc) => text(&doc),
            Err(e) => format!("read error {e:?}\n"),
        };
        std::fs::write(out.join(&name), dump).map_err(|e| format!("{name}: {e}"))?;
        writeln!(index, "{name} {}", file.display()).unwrap();
    }
    std::fs::write(out.join("index.txt"), index).map_err(|e| format!("index.txt: {e}"))?;
    Ok(files.len())
}
