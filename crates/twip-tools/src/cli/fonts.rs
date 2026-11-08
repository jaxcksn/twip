//! `twip chrome-fonts <dir> <out dir>`: the subsets of the chrome typeface the renderer bundles
//! (`crates/twip/src/render/chrome_fonts/`), JetBrains Mono 500 and 600, from `JetBrainsMono-{Medium,SemiBold}.ttf` in
//! `<dir>` (the release zip at <https://github.com/JetBrains/JetBrainsMono/releases>, `fonts/ttf/`).
//!
//! Each subset keeps Basic Latin and the symbols the chrome draws; the family is under the SIL Open Font License 1.1
//! (`licenses/jetbrains-mono.txt`).

use std::path::Path;

/// The characters the chrome draws: Basic Latin, Latin-1 Supplement, Latin Extended-A and the symbols in its labels,
/// hints and readouts.
fn chars() -> String {
    let mut s: String = (' '..='~').collect();
    s.push_str("″×Δ°·→←−⌫⌥⇧⌘▾…—–“”‘’");
    s.extend('\u{a0}'..='\u{17f}');
    s.push('↗');
    s
}

const FILES: [(&str, &str); 2] = [("JetBrainsMono-Medium.ttf", "jetbrains-mono-500.ttf"), ("JetBrainsMono-SemiBold.ttf", "jetbrains-mono-600.ttf")];

pub fn run(dir: &Path, out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let chars = chars();
    for (from, to) in FILES {
        let path = dir.join(from);
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let sub = twip::internal::pdf::subset_chars(&data, &chars).ok_or_else(|| format!("{}: not a TrueType face", path.display()))?;
        std::fs::write(out.join(to), &sub).map_err(|e| format!("{to}: {e}"))?;
        println!("{to}: {} bytes (from {})", sub.len(), data.len());
    }
    Ok(())
}
