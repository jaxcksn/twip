//! `twip-tools wire-fixtures [--check]`: writes one JSON example of every variant of every `api` type to
//! `crates/twip/tests/wire/<type>.json`. With `--check` it writes nothing and fails on any difference, including
//! files that no type makes any more.

use std::path::Path;

pub fn run(check: bool) -> Result<(), String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire");
    let fixtures = twip::api::wire::fixtures();
    let mut stale = Vec::new();
    for f in &fixtures {
        let path = dir.join(format!("{}.json", f.name));
        let mut text = serde_json::to_string_pretty(&(f.examples)()).map_err(|e| e.to_string())?;
        text.push('\n');
        let old = std::fs::read_to_string(&path).ok();
        if old.as_deref() == Some(text.as_str()) {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
        } else {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    // Files no fixture writes any more. `balloons.json` is hand-written: the cases `stack_balloons` must give.
    for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let known = name == "balloons.json" || name.strip_suffix(".json").is_some_and(|n| fixtures.iter().any(|f| f.name == n));
        if !known {
            if check {
                stale.push(e.path().display().to_string());
            } else {
                std::fs::remove_file(e.path()).map_err(|err| format!("{name}: {err}"))?;
            }
        }
    }
    if !stale.is_empty() {
        return Err(format!("wire fixtures differ from the api types (run twip-tools wire-fixtures):\n  {}", stale.join("\n  ")));
    }
    println!("{} wire fixtures {}", fixtures.len(), if check { "match" } else { "written" });
    Ok(())
}
