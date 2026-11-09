//! Layout tests

mod attempts;
mod auto_colour;
mod borders;
mod breaking;
mod cache_keys;
mod col_separators;
mod continuation;
mod convergence;
mod determinism;
mod equivalence;
mod excluded;
mod hf_frames;
mod imports;
mod notes;
mod output;
mod pending;
mod random_edits;
mod rebreak;
mod reuse;
mod robustness;
mod rows;
pub(crate) mod shapes;
mod showcase;
mod signatures;
mod snapshot_immutable;
mod source_mapping;
mod spacing;
mod spatial_tree;
mod sweeps;
mod view_accessors;
mod warm;

use std::path::{Path, PathBuf};

use crate::docx::Document;
use crate::font::Fonts;

pub fn fixtures() -> PathBuf {
    std::env::var_os("TWIP_REFERENCE_FIXTURES").map(std::path::PathBuf::from).unwrap_or_default()
}

pub fn baseline_dump() -> PathBuf {
    Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"twip\ng-baseline\run\dump")
}

pub fn fixture(rel: &str) -> Option<Document> {
    doc(&fixtures().join(rel))
}

pub fn doc(path: &Path) -> Option<Document> {
    crate::docx::read(&std::fs::read(path).ok()?).ok()
}

pub fn fonts() -> Fonts {
    Fonts::system_recommended()
}

pub fn gate_set(features: &[&str]) -> Vec<(PathBuf, Document)> {
    let Ok(index) = std::fs::read_to_string(baseline_dump().join("index.txt")) else { return Vec::new() };
    let allowed: std::collections::BTreeSet<&str> = features.iter().copied().collect();
    index
        .lines()
        .filter_map(|l| l.find(".txt ").map(|i| PathBuf::from(&l[i + 5..])))
        .filter_map(|p| doc(&p).map(|d| (p, d)))
        .filter(|(_, d)| crate::features::of(d).iter().all(|f| allowed.contains(f)))
        .collect()
}

pub fn layout_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs")
                && let Ok(text) = std::fs::read_to_string(&p)
            {
                out.push((p, text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("layout"), &mut out);
    out
}

pub fn plain_doc(texts: &[String]) -> Option<Document> {
    use crate::docx::{Block, Inline};
    let mut d = doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/text-basics.docx")))?;
    let template = d.sections[0].blocks.iter().find_map(|b| match b {
        Block::Para(p) if p.list.is_none() && p.inlines.iter().any(|i| matches!(i, Inline::Text(..))) => Some(p.clone()),
        _ => None,
    })?;
    let run = template.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None })?;
    d.sections.truncate(1);
    d.sections[0].blocks = texts
        .iter()
        .map(|t| {
            let mut p = template.clone();
            p.inlines = vec![Inline::Text(t.clone(), run.clone())];
            p.src = None;
            p.id = crate::docx::model::next_para_id();
            Block::Para(p)
        })
        .collect();
    Some(d)
}

pub fn page_chars(doc: &Document) -> Vec<Vec<(char, f64, f64)>> {
    let snap = crate::layout::layout(doc, &fonts());
    snap.pages()
        .iter()
        .map(|page| {
            let mut out = Vec::new();
            for item in crate::layout::traverse::paint_items(page) {
                if let crate::layout::fragment::Item::Glyphs(r) = item {
                    out.extend(r.chars.iter().map(|&(c, x, _)| (c, x, r.baseline)));
                }
            }
            out
        })
        .collect()
}

pub fn package(parts: &[(&str, &str)]) -> Option<Document> {
    crate::docx::read(&package_bytes(parts)?).ok()
}

pub fn package_bytes(parts: &[(&str, &str)]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, xml) in parts {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).ok()?;
        z.write_all(xml.as_bytes()).ok()?;
    }
    Some(z.finish().ok()?.into_inner())
}

pub fn words(n: usize) -> String {
    (0..n).map(|i| ["layout", "fragment", "token", "column", "page"][i % 5]).collect::<Vec<_>>().join(" ")
}
