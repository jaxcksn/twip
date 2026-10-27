//! Comparing a layout with reference geometry (`*.golden.json` files of drawn characters and images).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A document's characters and image boxes per page.
pub fn lay_out(doc: &crate::docx::Document, fonts: &crate::font::Fonts) -> (Vec<Vec<Ch>>, Vec<Vec<[f64; 4]>>) {
    let snapshot = crate::layout::layout(doc, fonts);
    let mut chars = Vec::new();
    let mut images = Vec::new();
    for page in snapshot.pages() {
        let (mut cs, mut im) = (Vec::new(), Vec::new());
        crate::layout::traverse::paint(page, &mut |p| match p.absolute() {
            crate::layout::fragment::Item::Glyphs(r) => {
                let mut prev_x = f64::NAN;
                for &(c, x, _) in r.chars.iter() {
                    cs.push(Ch { c, x, y: r.baseline, lig: x == prev_x, seg: p.line as usize });
                    prev_x = x;
                }
            }
            crate::layout::fragment::Item::Image { rect, .. } => im.push(rect),
            _ => {}
        });
        chars.push(cs);
        images.push(im);
    }
    (chars, images)
}

/// PDF coordinates sit on a 0.12pt grid (UNIT-7).
pub const TOL: f64 = 0.13;
/// Inside a line the reference PDF places glyphs with hinted 600-dpi advances (Times 12 'a': exact 5.326, drawn 5.28) and
/// re-syncs along the line, so interior glyph x is checked loosely; line starts and baselines use [`TOL`]. This is a
/// drawing difference, not a layout one.
pub const GLYPH_TOL: f64 = 0.5;

/// One drawn character. `lig`: a later letter of a ligature (its x is the ligature's origin, which a PDF extractor
/// reports differently), so its x isn't compared.
#[derive(Clone, Copy, Debug)]
pub struct Ch {
    pub c: char,
    pub x: f64,
    pub y: f64,
    pub lig: bool,
    /// The drawn segment (our glyph run, or the extractor's line) holding the character, in logical order.
    pub seg: usize,
}

fn keep(c: char) -> bool {
    !c.is_whitespace() && c != '\u{AD}' && c != '\u{200B}'
}

/// Ligature code points as their letters (a PDF may report either).
fn expand(c: char) -> &'static str {
    match c {
        '\u{FB00}' => "ff",
        '\u{FB01}' => "fi",
        '\u{FB02}' => "fl",
        '\u{FB03}' => "ffi",
        '\u{FB04}' => "ffl",
        _ => "",
    }
}

/// The reference's image boxes per page (`images` in the golden file).
pub fn golden_images(v: &Value) -> Vec<Vec<[f64; 4]>> {
    v["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            p["images"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| {
                    let a: Vec<f64> = r.as_array()?.iter().filter_map(|x| x.as_f64()).collect();
                    (a.len() == 4).then(|| [a[0], a[1], a[2], a[3]])
                })
                .collect()
        })
        .collect()
}

/// Image placement: every reference image has one of ours within [`TOL`] on each edge (order-independent).
pub fn compare_images(ours: &[Vec<[f64; 4]>], gold: &[Vec<[f64; 4]>], v: &mut Verdict) {
    for (pi, g) in gold.iter().enumerate() {
        let o = ours.get(pi).cloned().unwrap_or_default();
        for r in g {
            let best = o.iter().map(|q| (0..4).map(|k| (q[k] - r[k]).abs()).fold(0.0, f64::max)).fold(f64::INFINITY, f64::min);
            if best > TOL {
                v.images_ok = false;
                v.geom_ok = false;
                if v.note.is_empty() {
                    v.note = if best.is_finite() {
                        format!("p{} image [{:.2},{:.2},{:.2},{:.2}] off by {:.2}", pi + 1, r[0], r[1], r[2], r[3], best)
                    } else {
                        format!("p{} image [{:.2},{:.2},{:.2},{:.2}] missing", pi + 1, r[0], r[1], r[2], r[3])
                    };
                }
            }
        }
    }
}

pub fn golden(v: &Value) -> Vec<Vec<Ch>> {
    let mut pages = Vec::new();
    for p in v["pages"].as_array().into_iter().flatten() {
        let mut out = Vec::new();
        for (seg, l) in p["lines"].as_array().into_iter().flatten().enumerate() {
            if l["dir"].as_array().map(|d| d[0].as_i64() != Some(1)).unwrap_or(false) {
                continue; // shortcut: rotated text (TABLE-33) isn't compared yet
            }
            let y = l["y"].as_f64().unwrap_or(0.0);
            for s in l["spans"].as_array().into_iter().flatten() {
                let text: Vec<char> = s["text"].as_str().unwrap_or("").chars().collect();
                let xs = s["x"].as_array().cloned().unwrap_or_default();
                for (i, c) in text.iter().enumerate() {
                    let x = xs.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    out.push(Ch { c: *c, x, y, lig: false, seg });
                }
            }
        }
        pages.push(out);
    }
    pages
}

/// Characters grouped into rows by baseline (within 1pt); whitespace dropped. Within a row, segments are ordered by
/// their first x and keep their own (logical) character order: a PDF extractor reports odd x for the later letters of
/// a ligature, so sorting characters by x would scramble the reference's text.
fn rows(chars: &[Ch]) -> Vec<(f64, Vec<Ch>)> {
    let mut segs: BTreeMap<usize, Vec<Ch>> = BTreeMap::new();
    for ch in chars {
        let e = expand(ch.c);
        let list = segs.entry(ch.seg).or_default();
        if !e.is_empty() {
            for (k, d) in e.chars().enumerate() {
                list.push(Ch { c: d, lig: k > 0 || ch.lig, ..*ch });
            }
        } else if keep(ch.c) {
            list.push(*ch);
        }
    }
    // A segment's position is its leftmost character's (a line's y is its first glyph's, as the extractor reports).
    let first = |s: &Vec<Ch>| *s.iter().min_by(|a, b| a.x.total_cmp(&b.x)).unwrap();
    let mut segs: Vec<Vec<Ch>> = segs.into_values().filter(|s| !s.is_empty()).collect();
    segs.sort_by(|a, b| first(a).y.total_cmp(&first(b).y).then(first(a).x.total_cmp(&first(b).x)));
    let mut out: Vec<(f64, Vec<Vec<Ch>>)> = Vec::new();
    for s in segs {
        let y = first(&s).y;
        match out.last_mut() {
            Some((ry, row)) if (y - *ry).abs() < 1.0 => row.push(s),
            _ => out.push((y, vec![s])),
        }
    }
    out.into_iter()
        .map(|(y, mut r)| {
            r.sort_by(|a, b| first(a).x.total_cmp(&first(b).x));
            // Every character of a row compares with the row's y (the extractor gives one y per line).
            (y, r.into_iter().flatten().map(|c| Ch { y, ..c }).collect())
        })
        .collect()
}

fn text(row: &[Ch]) -> String {
    row.iter().map(|c| c.c).collect()
}

/// Whether two matched characters disagree beyond tolerance (the k-th character of its row).
fn off(k: usize, a: &Ch, b: &Ch) -> bool {
    let xtol = if k == 0 { TOL } else { GLYPH_TOL };
    ((a.x - b.x).abs() > xtol && !a.lig && !b.lig) || (a.y - b.y).abs() > TOL
}

#[derive(Default, Debug)]
pub struct Verdict {
    pub pages_ok: bool,
    pub text_ok: bool,
    pub geom_ok: bool,
    pub rows_total: usize,
    pub rows_matched: usize,
    pub max_dx: f64,
    pub max_dy: f64,
    pub images_ok: bool,
    pub note: String,
}

pub fn compare(ours: &[Vec<Ch>], gold: &[Vec<Ch>]) -> Verdict {
    let mut v = Verdict { pages_ok: ours.len() == gold.len(), text_ok: true, geom_ok: true, images_ok: true, ..Default::default() };
    if !v.pages_ok {
        v.note = format!("pages: ours {} vs Word {}", ours.len(), gold.len());
    }
    for pi in 0..gold.len().max(ours.len()) {
        let g = rows(gold.get(pi).map(|p| &p[..]).unwrap_or(&[]));
        let o = rows(ours.get(pi).map(|p| &p[..]).unwrap_or(&[]));
        v.rows_total += g.len();
        for (ri, gr) in g.iter().enumerate() {
            let or = o.get(ri);
            if or.is_some_and(|or| text(&or.1) == text(&gr.1)) {
                v.rows_matched += 1;
                let or = or.unwrap();
                for (k, (a, b)) in or.1.iter().zip(&gr.1).enumerate() {
                    if !a.lig && !b.lig {
                        v.max_dx = v.max_dx.max((a.x - b.x).abs());
                    }
                    v.max_dy = v.max_dy.max((a.y - b.y).abs());
                    if off(k, a, b) && v.geom_ok {
                        v.geom_ok = false;
                        if v.note.is_empty() {
                            v.note = format!(
                                "p{} row {} '{}': char '{}' ours ({:.2},{:.2}) Word ({:.2},{:.2})",
                                pi + 1,
                                ri + 1,
                                trunc(&text(&gr.1)),
                                b.c,
                                a.x,
                                a.y,
                                b.x,
                                b.y
                            );
                        }
                    }
                }
            } else if v.text_ok {
                v.text_ok = false;
                v.geom_ok = false;
                if v.note.is_empty() || v.note.starts_with("pages") {
                    let ours_t = or.map(|r| trunc(&text(&r.1))).unwrap_or_else(|| "<none>".into());
                    let extra = if v.note.is_empty() { String::new() } else { format!("{}; ", v.note) };
                    v.note = format!("{extra}p{} row {} y={:.2}: Word '{}' ours '{}'", pi + 1, ri + 1, gr.0, trunc(&text(&gr.1)), ours_t);
                }
            }
        }
        if o.len() > g.len() && v.text_ok {
            v.text_ok = false;
            v.geom_ok = false;
            if v.note.is_empty() {
                v.note = format!("p{}: {} extra rows, first '{}'", pi + 1, o.len() - g.len(), trunc(&text(&o[g.len()].1)));
            }
        }
    }
    if !v.pages_ok {
        v.text_ok = false;
        v.geom_ok = false;
    }
    v
}

fn load(docx: &Path, fonts: &crate::font::Fonts) -> (Vec<Vec<Ch>>, Vec<Vec<Ch>>) {
    let doc = crate::docx::read(&std::fs::read(docx).expect("read docx")).expect("parse docx");
    let g: Value = serde_json::from_str(&std::fs::read_to_string(docx.with_extension("golden.json")).expect("golden")).unwrap();
    (lay_out(&doc, fonts).0, golden(&g))
}

/// Prints the first row of `docx` that differs from the reference, glyph by glyph (or row `want` when given).
pub fn diff(docx: &Path, fonts: &crate::font::Fonts, want: Option<(usize, usize)>) {
    let (o, g) = load(docx, fonts);
    for pi in 0..g.len() {
        let gr = rows(&g[pi]);
        let or = rows(o.get(pi).map(|p| &p[..]).unwrap_or(&[]));
        for ri in 0..gr.len() {
            let hit = match want {
                Some((wp, wr)) => wp == pi + 1 && wr == ri + 1,
                None => {
                    or.get(ri).is_none_or(|r| text(&r.1) != text(&gr[ri].1) || r.1.iter().zip(&gr[ri].1).enumerate().any(|(k, (a, b))| off(k, a, b)))
                }
            };
            if !hit {
                continue;
            }
            println!("page {} row {}: Word y={:.3} ours y={:.3}", pi + 1, ri + 1, gr[ri].0, or.get(ri).map_or(f64::NAN, |r| r.0));
            println!("  Word: {}", text(&gr[ri].1));
            println!("  ours: {}", or.get(ri).map(|r| text(&r.1)).unwrap_or_default());
            let empty = Vec::new();
            let ochars = or.get(ri).map(|r| &r.1).unwrap_or(&empty);
            for k in 0..gr[ri].1.len().max(ochars.len()) {
                let a = gr[ri].1.get(k);
                let b = ochars.get(k);
                println!(
                    "  {:>3} {} {:>9} | {} {:>9} {}",
                    k,
                    a.map_or(' ', |c| c.c),
                    a.map_or(String::new(), |c| format!("{:.2}", c.x)),
                    b.map_or(' ', |c| c.c),
                    b.map_or(String::new(), |c| format!("{:.2}{}", c.x, if c.lig { "~" } else { "" })),
                    match (a, b) {
                        (Some(a), Some(b)) => format!("{:+.3}", b.x - a.x),
                        _ => String::new(),
                    }
                );
            }
            return;
        }
    }
    println!("no differences");
}

/// Lists page `page`'s rows side by side (baseline, text) — `from` = first row to show.
pub fn rows_of(docx: &Path, fonts: &crate::font::Fonts, page: usize, from: usize) {
    let (o, g) = load(docx, fonts);
    let gr = rows(g.get(page - 1).map(|p| &p[..]).unwrap_or(&[]));
    let or = rows(o.get(page - 1).map(|p| &p[..]).unwrap_or(&[]));
    for k in from.saturating_sub(1)..gr.len().max(or.len()) {
        let a = gr.get(k).map(|r| format!("{:8.2} {}", r.0, trunc(&text(&r.1)))).unwrap_or_default();
        let b = or.get(k).map(|r| format!("{:8.2} {}", r.0, trunc(&text(&r.1)))).unwrap_or_default();
        let same = gr.get(k).zip(or.get(k)).is_some_and(|(x, y)| text(&x.1) == text(&y.1));
        println!("{:>3} {} {:<62} | {}", k + 1, if same { ' ' } else { '*' }, a, b);
    }
}

/// Writes every matched row's characters as `x_ours x_word` pairs (whitespace dropped) for drift analysis.
pub fn pairs(docx: &Path, fonts: &crate::font::Fonts, out: &Path) {
    let (o, g) = load(docx, fonts);
    let mut s = String::new();
    for pi in 0..g.len() {
        let gr = rows(&g[pi]);
        let or = rows(o.get(pi).map(|p| &p[..]).unwrap_or(&[]));
        for (ri, r) in gr.iter().enumerate() {
            let Some(q) = or.get(ri).filter(|q| text(&q.1) == text(&r.1)) else { continue };
            let n = r.1.len();
            for (k, (a, b)) in q.1.iter().zip(&r.1).enumerate() {
                if !a.lig && !b.lig {
                    s.push_str(&format!("{} {} {} {} {:.4} {:.4} {} {:.4} {:.4}\n", pi + 1, ri + 1, k, n, a.x, b.x, b.c, a.y, b.y));
                }
            }
        }
    }
    std::fs::write(out, s).expect("write pairs");
}

fn trunc(s: &str) -> String {
    let t: String = s.chars().take(50).collect();
    if s.chars().count() > 50 { format!("{t}…") } else { t }
}

/// Runs every fixture under `paths` (folders or .docx files); prints pass rates per folder and writes
/// `report`. With `features`, only documents whose features are within the set count (`golden --features`).
pub fn run(paths: &[String], fonts: &crate::font::Fonts, report: &Path, features: Option<&std::collections::BTreeSet<&'static str>>) {
    let mut files: Vec<PathBuf> = Vec::new();
    for p in paths {
        collect(Path::new(p), &mut files);
    }
    files.sort();
    let mut by_folder: BTreeMap<String, Vec<(String, Verdict)>> = BTreeMap::new();
    for f in &files {
        let gold_path = f.with_extension("golden.json");
        let Ok(gold) = std::fs::read_to_string(&gold_path) else { continue };
        let folder = f.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let name = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let verdict = match std::fs::read(f).map_err(|e| e.to_string()).and_then(|b| crate::docx::read(&b)) {
            Ok(doc) if features.is_some_and(|f| !crate::features::of(&doc).is_subset(f)) => continue,
            Ok(doc) => {
                let layout = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lay_out(&doc, fonts)));
                match layout {
                    Ok((chars, images)) => {
                        let g: Value = serde_json::from_str(&gold).unwrap_or(Value::Null);
                        let mut v = compare(&chars, &golden(&g));
                        compare_images(&images, &golden_images(&g), &mut v);
                        v
                    }
                    Err(_) => Verdict { note: "panic during layout".into(), ..Default::default() },
                }
            }
            Err(e) => Verdict { note: format!("read error: {e}"), ..Default::default() },
        };
        by_folder.entry(folder).or_default().push((name, verdict));
    }
    let mut rep = String::new();
    let (mut tn, mut tp, mut tt, mut tg, mut trm, mut trt) = (0, 0, 0, 0, 0, 0);
    println!("{:<8} {:>5} {:>7} {:>7} {:>7} {:>13}", "folder", "docs", "pages", "text", "geom", "rows");
    for (folder, list) in &by_folder {
        let n = list.len();
        let p = list.iter().filter(|(_, v)| v.pages_ok).count();
        let t = list.iter().filter(|(_, v)| v.text_ok).count();
        let g = list.iter().filter(|(_, v)| v.geom_ok).count();
        let rm: usize = list.iter().map(|(_, v)| v.rows_matched).sum();
        let rt: usize = list.iter().map(|(_, v)| v.rows_total).sum();
        println!("{folder:<8} {n:>5} {p:>7} {t:>7} {g:>7} {:>6}/{:<6}", rm, rt);
        (tn, tp, tt, tg, trm, trt) = (tn + n, tp + p, tt + t, tg + g, trm + rm, trt + rt);
        rep.push_str(&format!("== {folder}: {n} docs, pages {p}, text {t}, geom {g}, rows {rm}/{rt}\n"));
        for (name, v) in list {
            let mark = if v.geom_ok {
                "ok  "
            } else if v.text_ok {
                "geom"
            } else if v.pages_ok {
                "text"
            } else {
                "FAIL"
            };
            rep.push_str(&format!("  {mark} {name}: rows {}/{} dx {:.3} dy {:.3} {}\n", v.rows_matched, v.rows_total, v.max_dx, v.max_dy, v.note));
        }
    }
    println!("{:<8} {tn:>5} {tp:>7} {tt:>7} {tg:>7} {:>6}/{:<6}", "total", trm, trt);
    let _ = std::fs::write(report, rep);
    println!("report: {}", report.display());
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                collect(&e.path(), out);
            }
        }
    } else if p.extension().is_some_and(|e| e == "docx") {
        out.push(p.to_path_buf());
    }
}
