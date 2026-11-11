//! Layout output snapshots

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::font::Fonts;
use crate::layout::fragment::{BoxKind, Child, Item, PageChild};
use crate::layout::layout;
use crate::layout::traverse::paint;

fn bundled_fonts() -> Fonts {
    let mut fonts = Fonts::default();
    let mut families = Vec::new();
    for file in ["stand-in-sans.ttf", "stand-in-serif.ttf", "stand-in-mono.ttf", "stand-in-cjk.ttf"] {
        let bytes = std::fs::read(root().join("fixtures/editor/fonts").join(file)).expect("stand-in font");
        families.extend(fonts.add_bytes(bytes).expect("stand-in font").into_iter().map(|f| f.family));
    }
    let family = |prefix: &str| families.iter().find(|f| f.starts_with(prefix)).cloned();
    let (sans, serif, mono, cjk) = (family("Stand-in Sans"), family("Stand-in Serif"), family("Stand-in Mono"), family("Stand-in CJK"));
    fonts.set_settings(|s| {
        s.generic.sans_serif = sans.clone();
        s.generic.serif = serif;
        s.generic.monospace = mono;
        s.fallback = sans;
        s.character_fallback = cjk.into_iter().collect();
    });
    fonts
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            fixtures(&p, out);
        } else if p.extension().is_some_and(|e| e == "docx") {
            out.push(p);
        }
    }
}

/// Each float's rectangle and each table cell's, in page space, in tree order.
fn boxes(children: &[Child], dx: f64, dy: f64, out: &mut Vec<(&'static str, [f64; 4])>) {
    let at = |r: [f64; 4]| [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];
    for c in children {
        match c {
            Child::Box(b) => {
                if let BoxKind::Float { rect, .. } = b.kind {
                    out.push(("float", at(rect)));
                }
                boxes(&b.children, dx + b.offset.0, dy + b.offset.1, out);
            }
            Child::Group { offset, children } => boxes(children, dx + offset.0, dy + offset.1, out),
            Child::Table(t) => {
                for cell in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    out.push(("cell", at(cell.place.rect)));
                    boxes(&cell.content, dx, dy, out);
                }
            }
            Child::Para(_) | Child::Line { .. } | Child::Items(_) => {}
        }
    }
}

/// Pages, then per drawn line its start, baseline and text, then picture boxes, floats and table cells, in twips.
// ponytail: coordinates rounded to whole twips and compared as text; a value landing on a rounding edge on one
// platform only would need a tolerance compare.
fn snapshot(doc: &crate::docx::Document, fonts: &Fonts) -> String {
    let f = |v: f64| format!("{}", (v * 20.0).round() as i64);
    let mut s = String::new();
    for (pi, page) in layout(doc, fonts).pages().iter().enumerate() {
        writeln!(s, "page {pi} {} {}", f(page.w), f(page.h)).unwrap();
        let mut lines: Vec<(u32, f64, f64, String)> = Vec::new();
        let mut images = Vec::new();
        paint(page, &mut |p| match p.absolute() {
            Item::Glyphs(r) => {
                let i = match lines.iter().position(|l| l.0 == p.line && p.line != 0) {
                    Some(i) => i,
                    None => {
                        lines.push((p.line, f64::INFINITY, r.baseline, String::new()));
                        lines.len() - 1
                    }
                };
                let line = &mut lines[i];
                for &(c, x, _) in r.chars.iter() {
                    line.1 = line.1.min(x);
                    line.3.push(c);
                }
            }
            Item::Image { rect, .. } => images.push(rect),
            _ => {}
        });
        for (_, x, y, text) in lines {
            let text = text.trim_end();
            if !text.is_empty() {
                writeln!(s, "  {} {} {text}", f(x), f(y)).unwrap();
            }
        }
        for r in images {
            writeln!(s, "  image {} {} {} {}", f(r[0]), f(r[1]), f(r[2]), f(r[3])).unwrap();
        }
        let mut found = Vec::new();
        for c in page.children.iter() {
            let b = match c {
                PageChild::Body(b) => &**b,
                PageChild::Header(b) | PageChild::Footer(b) => b,
            };
            boxes(&b.children, b.offset.0, b.offset.1, &mut found);
        }
        for (kind, r) in found {
            writeln!(s, "  {kind} {} {} {} {}", f(r[0]), f(r[1]), f(r[2]), f(r[3])).unwrap();
        }
    }
    s
}

fn first_difference(want: &str, got: &str) -> String {
    let pages = |t: &str| t.lines().filter(|l| l.starts_with("page ")).count();
    let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let at = w.iter().zip(&g).position(|(a, b)| a != b).unwrap_or(w.len().min(g.len()));
    let page = g[..at.min(g.len())].iter().rev().find(|l| l.starts_with("page ")).copied().unwrap_or("page 0");
    format!(
        "pages {} -> {}, first change at line {} ({page}):\n    - {}\n    + {}",
        pages(want),
        pages(got),
        at + 1,
        w.get(at).copied().unwrap_or("<end>"),
        g.get(at).copied().unwrap_or("<end>"),
    )
}

fn pages(t: &str) -> Vec<(&str, Vec<&str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for l in t.lines() {
        match l.strip_prefix("  ") {
            Some(item) => out.last_mut().into_iter().for_each(|p| p.1.push(item)),
            None => out.push((l, Vec::new())),
        }
    }
    out
}

/// One page drawn twice: grey where the snapshot and the layout agree, red for the snapshot only, green for the layout only.
fn page_svg(header: &str, want: &[&str], got: &[&str]) -> String {
    let n = |s: &str| s.parse::<f64>().unwrap_or(0.0) / 20.0;
    let dims: Vec<f64> = header.split(' ').skip(2).map(n).collect();
    let (w, h) = (dims.first().copied().unwrap_or(612.0), dims.get(1).copied().unwrap_or(792.0));
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{}\" height=\"{}\">\n", w * 1.5, h * 1.5);
    writeln!(s, "<rect width=\"{w}\" height=\"{h}\" fill=\"white\" stroke=\"black\"/>").unwrap();
    let mut draw = |item: &str, colour: &str| {
        let v: Vec<&str> = item.splitn(3, ' ').collect();
        if let Some(r) = ["image ", "float ", "cell "].iter().find_map(|k| item.strip_prefix(k)) {
            let r: Vec<f64> = r.split(' ').map(n).collect();
            if let [x0, y0, x1, y1] = r[..] {
                writeln!(s, "<rect x=\"{x0}\" y=\"{y0}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{colour}\"/>", x1 - x0, y1 - y0).unwrap();
            }
        } else if let [x, y, text] = v[..] {
            let (x, y, text) = (n(x), n(y), escape(text));
            writeln!(
                s,
                "<text x=\"{x}\" y=\"{y}\" font-size=\"9\" font-family=\"sans-serif\" fill=\"{colour}\" xml:space=\"preserve\">{text}</text>"
            )
            .unwrap();
        }
    };
    for item in got {
        draw(item, if want.contains(item) { "#999" } else { "#080" });
    }
    for item in want.iter().filter(|i| !got.contains(i)) {
        draw(item, "#d00");
    }
    s + "</svg>\n"
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `target/layout-diff`, which CI uploads as an artifact when the check fails.
fn diff_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().join("target").join("layout-diff")
}

/// The pages that differ as SVG files in `target/layout-diff/<name>/`, returning their paths under the root.
fn write_diff(name: &str, want: &str, got: &str) -> Vec<String> {
    let folder = name.trim_end_matches(".txt");
    std::fs::create_dir_all(diff_root().join(folder)).unwrap();
    let (w, g) = (pages(want), pages(got));
    let mut written = Vec::new();
    for i in 0..w.len().max(g.len()) {
        let (wp, gp) = (w.get(i), g.get(i));
        if wp.map(|p| &p.1) == gp.map(|p| &p.1) {
            continue;
        }
        let header = gp.or(wp).map_or("page", |p| p.0);
        let svg = page_svg(header, wp.map_or(&[][..], |p| &p.1), gp.map_or(&[][..], |p| &p.1));
        let file = format!("{folder}/page-{i}.svg");
        std::fs::write(diff_root().join(&file), svg).unwrap();
        written.push(file);
    }
    written
}

/// Every fixture's layout equals its committed snapshot in `tests/layout`. `TWIP_WRITE_LAYOUT=1` rewrites them.
#[test]
fn layout_output_matches_snapshots() {
    let (dir, write) = (root().join("layout"), std::env::var_os("TWIP_WRITE_LAYOUT").is_some());
    let fonts = bundled_fonts();
    let mut paths = Vec::new();
    fixtures(&root().join("fixtures"), &mut paths);
    let mut got = BTreeMap::new();
    for path in &paths {
        // DATE/TIME fields at a fixed instant (2026-11-09 12:00 UTC), so the snapshots don't change with the day.
        let Some(d) = std::fs::read(path).ok().and_then(|b| crate::docx::read_at(&b, 1_794_225_600).ok()) else { continue };
        let rel = path.strip_prefix(root().join("fixtures")).unwrap().to_string_lossy().replace(['\\', '/'], "__");
        got.insert(rel.replace(".docx", ".txt"), snapshot(&d, &fonts));
    }
    if write {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in &got {
            std::fs::write(dir.join(name), text).unwrap();
        }
        return;
    }
    let _ = std::fs::remove_dir_all(diff_root());
    let (mut failures, mut index) = (Vec::new(), String::new());
    for (name, text) in &got {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(want) if want.replace("\r\n", "\n") == *text => {}
            Ok(want) => {
                let want = want.replace("\r\n", "\n");
                let summary = first_difference(&want, text);
                write!(index, "<h2>{name}</h2>\n<pre>{}</pre>\n", escape(&summary)).unwrap();
                for page in write_diff(name, &want, text) {
                    writeln!(index, "<p>{page}</p><img src=\"{page}\">").unwrap();
                }
                failures.push(format!("{name}: {summary}"));
            }
            Err(_) => failures.push(format!("{name}: no snapshot")),
        }
    }
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !got.contains_key(&name) {
            failures.push(format!("{name}: snapshot without a fixture"));
        }
    }
    if !index.is_empty() {
        let page = format!(
            "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Layout changes</title>\n<style>body{{font-family:sans-serif}}img{{border:1px solid #ccc;max-width:100%}}</style>\n<h1>Layout changes</h1>\n<p>Grey: unchanged. <span style=\"color:#d00\">Red: the snapshot.</span> <span style=\"color:#080\">Green: this layout.</span></p>\n{index}"
        );
        std::fs::write(diff_root().join("index.html"), page).unwrap();
    }
    assert!(
        failures.is_empty(),
        "layout output changed for {} fixture(s); the changed pages are drawn in {} (in CI: the layout-diff artifact). If intended, run `TWIP_WRITE_LAYOUT=1 cargo test -p twip --lib layout_output` and commit tests/layout:\n{}",
        failures.len(),
        diff_root().join("index.html").display(),
        failures.join("\n")
    );
}
