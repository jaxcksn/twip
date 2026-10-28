//! Every rule id cited in a comment of `src/**/*.rs` is a rule of the rulebook (`docs/docs/rulebook`).

use std::collections::BTreeSet;
use std::path::Path;

const CATEGORIES: [&str; 21] = [
    "UNIT", "FONT", "STYLE", "FIELD", "LINE", "PARA", "BREAK", "JUST", "TAB", "LIST", "PAGE", "COL", "FLOAT", "FRAME", "TABLE", "HDR", "NOTE",
    "COMPAT", "LNUM", "SETUP", "REV",
];

/// The comment part of a line (`//` to the end), ignoring `//` inside string literals.
fn comment(line: &str) -> &str {
    let mut in_str = false;
    let b = line.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str && b[i + 1] == b'/' => return &line[i..],
            _ => {}
        }
        i += 1;
    }
    ""
}

/// The rule ids (`TABLE-9`) in `text`: a category, a hyphen and digits, as a whole word that isn't part of a longer
/// hyphenated name (`E2E-TBL-COL-01` isn't one).
fn ids(text: &str, out: &mut BTreeSet<String>) {
    let part = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
    for cat in CATEGORIES {
        let tag = format!("{cat}-");
        for (at, _) in text.match_indices(&tag) {
            let digits: String = text[at + tag.len()..].chars().take_while(char::is_ascii_digit).collect();
            let after = text[at + tag.len() + digits.len()..].chars().next();
            if !digits.is_empty() && !part(text[..at].chars().next_back()) && !part(after) {
                out.insert(format!("{cat}-{digits}"));
            }
        }
    }
}

fn cited(dir: &Path, out: &mut BTreeSet<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            cited(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            for line in std::fs::read_to_string(&p).unwrap().lines() {
                ids(comment(line), out);
            }
        }
    }
}

#[test]
fn every_cited_rule_is_in_the_rulebook() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let book = root.join("../../docs/docs/rulebook");
    let mut rules = BTreeSet::new();
    for e in std::fs::read_dir(&book).expect("the rulebook").flatten() {
        if e.path().extension().is_some_and(|x| x == "md") {
            for line in std::fs::read_to_string(e.path()).unwrap().lines().filter(|l| l.starts_with("### ")) {
                ids(line, &mut rules);
            }
        }
    }
    assert!(rules.len() > 200, "rulebook headings found: {}", rules.len());
    let mut code = BTreeSet::new();
    cited(&root.join("src"), &mut code);
    let unknown: Vec<&String> = code.difference(&rules).collect();
    assert!(unknown.is_empty(), "cited in src but not in the rulebook: {unknown:?}");
}
