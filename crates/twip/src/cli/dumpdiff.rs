//! `twip dumpdiff <a dir> <b dir> [--features f1,f2,...]`: compares two layout dumps document by document.
//! Per page, `line N` ids are renumbered by each line's position (first baseline y, then first x) and
//! the item lines are sorted, because item order is an artefact of how an engine emits items, while every item's
//! geometry must match. Each document is `exact` (identical as written), `order-only` (identical after
//! canonicalizing) or `different`. With `--features`, only documents whose features are within the list count.
//!
//! Line ids are renumbered per page: they only group runs into lines, and a cached table row's lines can share ids
//! across the pages a header row repeats on (TABLE-46), which says nothing about layout.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// The documents of a dump directory: source path → dump file name, from its `index.txt`.
fn index(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let text = std::fs::read_to_string(dir.join("index.txt")).map_err(|e| format!("{}: {e}", dir.join("index.txt").display()))?;
    // Lines are `<name>.txt <source path>`; both may contain spaces, the name never contains ".txt ".
    Ok(text.lines().filter_map(|l| l.find(".txt ").map(|i| (l[i + 5..].to_string(), l[..i + 4].to_string()))).collect())
}

/// The value after `key ` in a dump line (`y 712.000`).
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(&format!(" {key} "))? + key.len() + 2;
    line[at..].split(' ').next()
}

/// A dump in canonical form: pages, each its header line and sorted item lines, with line ids renumbered by position.
pub fn canonical(dump: &str) -> Vec<Vec<String>> {
    let mut pages: Vec<Vec<&str>> = Vec::new();
    for line in dump.lines() {
        if line.starts_with("page ") || pages.is_empty() {
            pages.push(Vec::new());
        }
        if let Some(p) = pages.last_mut() {
            p.push(line);
        }
    }
    // Each (page, line id)'s position: its first run's baseline and x.
    let mut first: HashMap<(usize, &str), (usize, f64, f64)> = HashMap::new();
    for (pi, page) in pages.iter().enumerate() {
        for l in page.iter().filter(|l| l.starts_with("G ")) {
            let (Some(id), Some(y), Some(x)) = (field(l, "line"), field(l, "y"), field(l, "x")) else { continue };
            let pos = (pi, y.parse().unwrap_or(0.0), x.parse().unwrap_or(0.0));
            first.entry((pi, id)).or_insert(pos);
        }
    }
    type Placed<'a> = ((usize, &'a str), (usize, f64, f64));
    let mut order: Vec<Placed> = first.into_iter().collect();
    order.sort_by(|a, b| a.1.0.cmp(&b.1.0).then(a.1.1.total_cmp(&b.1.1)).then(a.1.2.total_cmp(&b.1.2)).then(a.0.cmp(&b.0)));
    let renumber: HashMap<(usize, &str), usize> = order.iter().enumerate().map(|(n, (key, _))| (*key, n)).collect();
    pages
        .iter()
        .enumerate()
        .map(|(pi, page)| {
            let mut items: Vec<String> = page
                .iter()
                .skip(1)
                .map(|l| match (l.starts_with("G "), field(l, "line").and_then(|id| Some((id, renumber.get(&(pi, id))?)))) {
                    (true, Some((id, n))) => l.replacen(&format!(" line {id} "), &format!(" line {n} "), 1),
                    _ => l.to_string(),
                })
                .collect();
            items.sort();
            let mut out = vec![page.first().map_or(String::new(), |h| h.to_string())];
            out.extend(items);
            out
        })
        .collect()
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Exact,
    OrderOnly,
    /// The first canonical lines that differ: (page, a's line, b's line).
    Different(usize, String, String),
}

pub fn compare(a: &str, b: &str) -> Verdict {
    if a == b {
        return Verdict::Exact;
    }
    let (ca, cb) = (canonical(a), canonical(b));
    if ca == cb {
        return Verdict::OrderOnly;
    }
    for p in 0..ca.len().max(cb.len()) {
        let (pa, pb) = (ca.get(p), cb.get(p));
        let n = pa.map_or(0, Vec::len).max(pb.map_or(0, Vec::len));
        for i in 0..n {
            let la = pa.and_then(|v| v.get(i)).cloned().unwrap_or_else(|| "(none)".into());
            let lb = pb.and_then(|v| v.get(i)).cloned().unwrap_or_else(|| "(none)".into());
            if la != lb {
                return Verdict::Different(p, la, lb);
            }
        }
    }
    Verdict::Different(0, "(page count)".into(), "(page count)".into())
}

pub fn run(a: &Path, b: &Path, features: Option<&BTreeSet<&'static str>>) -> Result<(), String> {
    let ia = index(a)?;
    let ib: BTreeMap<String, String> = index(b)?.into_iter().collect();
    let (mut exact, mut order, mut different, mut missing, mut skipped) = (0, 0, 0, 0, 0);
    for (source, name_a) in &ia {
        let doc_features = std::fs::read(source).ok().and_then(|bytes| twip::docx::read(&bytes).ok()).map(|d| twip::features::of(&d));
        if let Some(allowed) = features
            && !doc_features.as_ref().is_some_and(|f| f.is_subset(allowed))
        {
            skipped += 1;
            continue;
        }
        let Some(name_b) = ib.get(source) else {
            missing += 1;
            println!("missing  {source}");
            continue;
        };
        let read = |dir: &Path, name: &str| std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: {e}"));
        match compare(&read(a, name_a)?, &read(b, name_b)?) {
            Verdict::Exact => exact += 1,
            Verdict::OrderOnly => {
                order += 1;
                println!("order-only {source}");
            }
            Verdict::Different(page, la, lb) => {
                different += 1;
                let list: Vec<&str> = doc_features.iter().flatten().copied().collect();
                println!("different  {source} [{}]\n    page {page}\n    a: {la}\n    b: {lb}", list.join(","));
            }
        }
    }
    println!("{exact} exact, {order} order-only, {different} different, {missing} missing in b, {skipped} outside the feature set");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Verdict, compare};

    const A: &str = "page 0 612.000 792.000 None\n\
        R 1.000 2.000 3.000 4.000 000000\n\
        G Calibri/400/false/false/false size 11.000 y 100.000 x 72.000 color None para 1 line 0 glyphs [] chars \"a\" cx [] src []\n\
        G Calibri/400/false/false/false size 11.000 y 120.000 x 72.000 color None para 1 line 1 glyphs [] chars \"b\" cx [] src []\n";

    #[test]
    fn reordered_items_and_renumbered_lines_are_order_only() {
        let b = "page 0 612.000 792.000 None\n\
            G Calibri/400/false/false/false size 11.000 y 120.000 x 72.000 color None para 1 line 0 glyphs [] chars \"b\" cx [] src []\n\
            R 1.000 2.000 3.000 4.000 000000\n\
            G Calibri/400/false/false/false size 11.000 y 100.000 x 72.000 color None para 1 line 1 glyphs [] chars \"a\" cx [] src []\n";
        assert_eq!(compare(A, A), Verdict::Exact);
        assert_eq!(compare(A, b), Verdict::OrderOnly);
    }

    #[test]
    fn moved_geometry_is_different() {
        let b = A.replace("y 120.000", "y 121.000");
        assert!(matches!(compare(A, &b), Verdict::Different(0, ..)));
    }
}
