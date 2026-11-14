//! The wire: how API values serialize to JSON for the SDKs, and the fixtures that pin it.
//!
//! The rules, applied by the `#[wire(...)]` attributes on every `api` type (`twip_wire`'s derives):
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

#[cfg(test)]
mod examples;

#[cfg(test)]
pub use examples::{Fixture, fixtures};

/// An API value as a `serde_json::Value` (serde_json's `to_value`), for tests and the inspection surface.
#[cfg(any(test, feature = "inspect"))]
pub(crate) fn to_value<T: twip_wire::Encode + ?Sized>(v: &T) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(&twip_wire::to_vec(v))
}

/// An API value from a `serde_json::Value` (serde_json's `from_value`), for tests.
#[cfg(test)]
pub(crate) fn from_value<T: twip_wire::Decode>(v: serde_json::Value) -> Result<T, twip_wire::Error> {
    twip_wire::from_slice(&serde_json::to_vec(&v).expect("JSON"))
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
            let text = serde_json::to_string_pretty(&(f.examples)()).expect("examples encode") + "\n";
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
