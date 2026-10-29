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
//! [`fixtures`] lists one example of every variant of every type; `twip-tools wire-fixtures` writes them to
//! `crates/twip/tests/wire/<type>.json`, and each SDK round-trips them through its own types.

mod examples;

pub use examples::{Fixture, fixtures};

/// `#[serde(with = "value")]`: a single argument as `{ "value": v }` inside its tagged object.
pub(crate) mod value {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct Out<'a, T> {
        value: &'a T,
    }

    #[derive(Deserialize)]
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
}
