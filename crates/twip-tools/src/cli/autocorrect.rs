//! `twip autocorrect-list <dir> <out dir>`: the AutoCorrect lists the editor ships (`crates/twip/src/editing/autocorrect/`),
//! from LibreOffice's en-US lists in `<dir>`
//! (<https://github.com/LibreOffice/core/tree/master/extras/source/autocorr/lang/en-US>, MPL-2.0):
//!
//! - `DocumentList.xml` → `en-US.tsv`: the replacement list, a `from<TAB>to` line per entry, sorted by `from`. Entries
//!   written with LibreOffice's wildcard (`.*`) are left out: the editor's symbol rules cover them.
//! - `SentenceExceptList.xml` → `en-US-sentence.txt`: abbreviations after which a sentence doesn't start.
//! - `WordExceptList.xml` → `en-US-caps.txt`: words whose two initial capitals stay.
//!
//! Each file keeps an MPL-2.0 header and stays a file of its own (`licenses/libreoffice-autocorrect.txt`).

use std::path::Path;

/// The value of attribute `name` in `line`, unescaped.
fn attr(line: &str, name: &str) -> Option<String> {
    let start = line.find(&format!("{name}=\""))? + name.len() + 2;
    let end = start + line[start..].find('"')?;
    Some(line[start..end].replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"))
}

const HEADER: &str = "# This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not\n\
                      # distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.\n";

fn blocks(dir: &Path, file: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .filter(|l| l.contains("<block-list:block "))
        .filter_map(|l| Some((attr(l, "block-list:abbreviated-name")?, attr(l, "block-list:name"))))
        .filter(|(from, to)| !from.is_empty() && ![Some(from), to.as_ref()].into_iter().flatten().any(|s| s.contains(['\t', '\n', '\r'])))
        .collect())
}

fn write(out: &Path, name: &str, source: &str, lines: &[String]) -> Result<(), String> {
    let mut text = format!("{HEADER}# From LibreOffice extras/source/autocorr/lang/en-US/{source} (twip-tools autocorrect-list).\n");
    lines.iter().for_each(|l| {
        text.push_str(l);
        text.push('\n');
    });
    std::fs::write(out.join(name), text).map_err(|e| format!("{name}: {e}"))?;
    println!("{name}: {} lines", lines.len());
    Ok(())
}

pub fn run(dir: &Path, out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut entries: Vec<(String, String)> =
        blocks(dir, "DocumentList.xml")?.into_iter().filter_map(|(f, t)| Some((f, t?))).filter(|(f, _)| !f.contains(".*")).collect();
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);
    write(out, "en-US.tsv", "DocumentList.xml", &entries.iter().map(|(f, t)| format!("{f}\t{t}")).collect::<Vec<_>>())?;
    for (file, name) in [("SentenceExceptList.xml", "en-US-sentence.txt"), ("WordExceptList.xml", "en-US-caps.txt")] {
        let mut words: Vec<String> = blocks(dir, file)?.into_iter().map(|(f, _)| f).collect();
        words.sort();
        words.dedup();
        write(out, name, file, &words)?;
    }
    Ok(())
}
