//! The wire: how API values serialize to JSON for the SDKs, and the fixtures that pin it.
//!
//! The rules, applied by the `serde` attributes on every `api` type:
//! - fields are camelCase, and inputs refuse unknown fields;
//! - enums are internally tagged with `type` (commands, events, menu actions) or `kind` (menu items and targets,
//!   blocks, dialog values), and their unit variants are camelCase strings;
//! - a variant with one value that isn't an object carries it in `value`; an options object spreads its fields;
//! - optional fields are left out when unset; fields that state reports as absent are `null`;
//! - a mixed value is `"mixed"` ([`super::state::Tri`]), lengths are points, colours `#RRGGBB`, bytes base64.
//!
//! [`fixtures`] lists one example of every variant of every type, committed as `crates/twip/tests/wire/<type>.json`
//! (`TWIP_WRITE_WIRE=1 cargo nextest run -p twip --lib fixtures_are_current` rewrites them); each SDK round-trips them
//! through its own types.

mod examples;

pub use examples::{Fixture, fixtures};

/// `#[serde(with = "value")]`: a single argument as `{ "value": v }` inside its tagged object.
pub(crate) mod value {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, twip_wire::Encode)]
    struct Out<'a, T> {
        value: &'a T,
    }

    #[derive(Deserialize, twip_wire::Decode)]
    #[serde(deny_unknown_fields)]
    struct In<T> {
        value: T,
    }

    pub fn serialize<S: Serializer, T: Serialize>(value: &T, s: S) -> Result<S::Ok, S::Error> {
        Out { value }.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
        In::deserialize(d).map(|w| w.value)
    }
}

/// `#[serde(with = "nullable")]` on an `Option<Option<T>>` field: absent is `None` (unchanged), `null` is
/// `Some(None)` (cleared), a value is `Some(Some(v))`.
pub(crate) mod nullable {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(value: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => v.serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every committed fixture decodes into its type and encodes back to the same JSON, and the committed files are
    /// what the examples make now.
    #[test]
    fn fixtures_round_trip() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire");
        for f in fixtures() {
            let path = dir.join(format!("{}.json", f.name));
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let committed: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let serde_json::Value::Array(items) = &committed else { panic!("{}: not an array", f.name) };
            for (i, item) in items.iter().enumerate() {
                let again = (f.round_trip)(item).unwrap_or_else(|e| panic!("{}[{i}]: {e}", f.name));
                assert_eq!(&again, item, "{}[{i}] re-encodes differently", f.name);
            }
            assert_eq!(committed, serde_json::Value::Array((f.examples)()), "{} differs from its examples", f.name);
        }
    }

    /// twip-wire reads and writes every fixture as serde does, and accepts and refuses the same variations of them.
    #[test]
    fn twip_wire_matches_serde() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire");
        let mut failures = Vec::new();
        let mut checked = 0;
        for f in fixtures() {
            let text = std::fs::read_to_string(dir.join(format!("{}.json", f.name))).expect("fixture");
            let serde_json::Value::Array(items) = serde_json::from_str(&text).expect("JSON") else { panic!("{}: not an array", f.name) };
            for (i, item) in items.iter().enumerate() {
                for (label, input) in std::iter::once(("as committed".to_string(), item.clone())).chain(variations(item)) {
                    let c = (f.compare)(&serde_json::to_vec(&input).expect("serializes"));
                    checked += 1;
                    let problem = match (c.serde, c.wire) {
                        (Ok(()), Err(e)) => Some(format!("wire refuses what serde reads: {e}")),
                        (Err(e), Ok(())) => Some(format!("wire reads what serde refuses: {e}")),
                        _ => c.mismatch,
                    };
                    if let Some(p) = problem {
                        failures.push(format!("{}[{i}] {label}: {p}\n    {input}", f.name));
                    }
                }
            }
        }
        // The first difference of each kind per type.
        let mut seen = std::collections::BTreeSet::new();
        let shown: Vec<&String> = failures
            .iter()
            .filter(|f| {
                let kind =
                    f.split(" at `").next().unwrap_or(f).split(['[', ']']).filter(|s| !s.chars().all(|c| c.is_ascii_digit())).collect::<String>();
                seen.insert(kind)
            })
            .take(60)
            .collect();
        assert!(
            failures.is_empty(),
            "{} of {checked} inputs differ:\n{}",
            failures.len(),
            shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
        );
    }

    /// Variations of `v`: at each object an unknown key added and each key removed, each value replaced by one of
    /// another kind.
    fn variations(v: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
        use serde_json::{Value, json};
        fn paths(v: &Value, at: String, out: &mut Vec<String>) {
            match v {
                Value::Object(m) => m.iter().for_each(|(k, x)| paths(x, format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")), out)),
                Value::Array(a) => a.iter().enumerate().for_each(|(i, x)| paths(x, format!("{at}/{i}"), out)),
                _ => {}
            }
            out.push(at);
        }
        let mut all = Vec::new();
        paths(v, String::new(), &mut all);
        let mut out = Vec::new();
        for p in all {
            let node = v.pointer(&p).expect("a path into v");
            let mut with = |label: String, f: &dyn Fn(&mut Value)| {
                let mut c = v.clone();
                f(c.pointer_mut(&p).expect("a path into the copy"));
                out.push((format!("{label} at `{p}`"), c));
            };
            if let Value::Object(m) = node {
                with("unknown key".into(), &|n| {
                    n.as_object_mut().expect("object").insert("zzUnknown".into(), json!(1));
                });
                for k in m.keys() {
                    with(format!("without `{k}`"), &|n| {
                        n.as_object_mut().expect("object").remove(k);
                    });
                }
            }
            let other = match node {
                Value::String(_) | Value::Null => json!(7),
                Value::Array(_) => json!({}),
                _ => json!("x"),
            };
            with("wrong kind".into(), &|n| *n = other.clone());
        }
        out
    }

    /// The committed files are byte for byte what the examples write, and no file is left that no type makes.
    /// `TWIP_WRITE_WIRE=1 cargo nextest run -p twip --lib fixtures_are_current` rewrites them.
    #[test]
    fn fixtures_are_current() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wire");
        let write = std::env::var_os("TWIP_WRITE_WIRE").is_some();
        let fixtures = fixtures();
        let mut stale = Vec::new();
        for f in &fixtures {
            let path = dir.join(format!("{}.json", f.name));
            let text = serde_json::to_string_pretty(&(f.examples)()).expect("examples serialize") + "\n";
            if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
                continue;
            }
            if write {
                std::fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            } else {
                stale.push(path.display().to_string());
            }
        }
        // `balloons.json` is hand-written: the cases `stack_balloons` must give.
        for e in std::fs::read_dir(&dir).expect("tests/wire").flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "balloons.json" || name.strip_suffix(".json").is_some_and(|n| fixtures.iter().any(|f| f.name == n)) {
                continue;
            }
            if write {
                std::fs::remove_file(e.path()).unwrap_or_else(|err| panic!("{name}: {err}"));
            } else {
                stale.push(e.path().display().to_string());
            }
        }
        assert!(stale.is_empty(), "wire fixtures differ from the api types (rerun with TWIP_WRITE_WIRE=1):\n  {}", stale.join("\n  "));
    }
}
