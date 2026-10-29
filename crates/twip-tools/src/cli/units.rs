//! `twip units <docx or folders...>`: lays out each document with every atomic unit timed and prints
//! the units over the limit (8 ms) by document, class and duration.

use std::path::{Path, PathBuf};

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        v.sort();
        for c in v {
            collect(&c, out);
        }
    } else if p.extension().is_some_and(|e| e == "docx") {
        out.push(p.to_path_buf());
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    // `--over <ms>` reports units over another limit (diagnostics).
    let (limit, paths) = match args.iter().position(|a| a == "--over") {
        Some(i) => {
            let ms: f64 = args.get(i + 1).and_then(|s| s.parse().ok()).ok_or("--over needs milliseconds")?;
            (ms, args.iter().enumerate().filter(|(k, _)| *k != i && *k != i + 1).map(|(_, a)| a.clone()).collect::<Vec<_>>())
        }
        None => (twip::internal::layout::job::UNIT_LIMIT_MS, args.to_vec()),
    };
    twip::internal::layout::job::time_units(limit);
    let fonts = twip::internal::font::Fonts::system();
    let mut docs = Vec::new();
    for p in &paths {
        collect(Path::new(p), &mut docs);
    }
    let (mut over, mut laid) = (0, 0);
    let mut worst: Vec<(f64, &'static str, String)> = Vec::new();
    for path in &docs {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(doc) = twip::internal::docx::read(&bytes) else { continue };
        twip::internal::layout::job::take_over_limit();
        twip::internal::layout::layout(&doc, &fonts);
        laid += 1;
        for (class, ms) in twip::internal::layout::job::take_over_limit() {
            over += 1;
            println!("{:8.2} ms  {class:<14} {}", ms, path.display());
            worst.push((ms, class, path.display().to_string()));
        }
    }
    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{laid} documents laid out; {over} units over {limit} ms");
    if let Some((ms, class, path)) = worst.first() {
        println!("longest: {ms:.2} ms ({class}) in {path}");
    }
    Ok(())
}
