//! A document's font descriptions and their generic categories

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A generic font category, like the CSS generic families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GenericFamily {
    /// Fonts with serifs (`roman` in a document's font table).
    Serif,
    /// Fonts without serifs (`swiss`).
    SansSerif,
    /// Fixed-pitch fonts (`modern`, or any font the document says is fixed pitch).
    Monospace,
    /// Script and handwriting fonts (`script`).
    Script,
    /// Decorative and display fonts (`decorative`).
    Decorative,
}

/// A document's description of a font family, from its font table (`w:font`). Each field is absent when the
/// document doesn't state it, or states a value this reader doesn't know.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontDescription {
    /// The general class (`w:family`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<FontClass>,
    /// The pitch (`w:pitch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<FontPitch>,
    /// The PANOSE classification (`w:panose1`): 10 bytes, 20 hex digits on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "panose_out", deserialize_with = "panose_in")]
    pub panose: Option<[u8; 10]>,
    /// The character set (`w:charset`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<u8>,
}

/// A font table's general class of a family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontClass {
    /// Serif (`roman`).
    Roman,
    /// Sans serif (`swiss`).
    Swiss,
    /// Fixed pitch (`modern`).
    Modern,
    /// Script (`script`).
    Script,
    /// Decorative (`decorative`).
    Decorative,
    /// Not stated (`auto`).
    Auto,
}

/// A font table's pitch of a family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontPitch {
    /// Every character has the same width.
    Fixed,
    /// Widths vary.
    Variable,
    /// Not stated.
    Default,
}

fn panose_out<S: Serializer>(p: &Option<[u8; 10]>, s: S) -> Result<S::Ok, S::Error> {
    match p {
        Some(bytes) => s.serialize_str(&bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()),
        None => s.serialize_none(),
    }
}

fn panose_in<'de, D: Deserializer<'de>>(d: D) -> Result<Option<[u8; 10]>, D::Error> {
    let hex = Option::<String>::deserialize(d)?;
    Ok(hex.as_deref().and_then(panose))
}

/// A PANOSE classification from its 20 hex digits; `None` for anything else.
pub fn panose(hex: &str) -> Option<[u8; 10]> {
    if hex.len() != 20 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 10];
    for (k, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).ok()?;
    }
    Some(out)
}

/// A family's generic category from what its description states: fixed pitch first, then the general class, then
/// the PANOSE family kind and serif style.
pub fn classify(d: &FontDescription) -> Option<GenericFamily> {
    let panose = d.panose.as_ref();
    if d.pitch == Some(FontPitch::Fixed) || panose.is_some_and(|p| p[0] == 2 && p[3] == 9) {
        return Some(GenericFamily::Monospace);
    }
    match d.class {
        Some(FontClass::Roman) => return Some(GenericFamily::Serif),
        Some(FontClass::Swiss) => return Some(GenericFamily::SansSerif),
        Some(FontClass::Modern) => return Some(GenericFamily::Monospace),
        Some(FontClass::Script) => return Some(GenericFamily::Script),
        Some(FontClass::Decorative) => return Some(GenericFamily::Decorative),
        Some(FontClass::Auto) | None => {}
    }
    match panose? {
        [2, 11..=13, ..] => Some(GenericFamily::SansSerif),
        [2, 2..=10, ..] => Some(GenericFamily::Serif),
        [3, ..] => Some(GenericFamily::Script),
        [4 | 5, ..] => Some(GenericFamily::Decorative),
        _ => None,
    }
}

/// Every described family's category, by lowercase name.
pub fn categories(descriptions: &HashMap<String, FontDescription>) -> HashMap<String, GenericFamily> {
    descriptions.iter().filter_map(|(name, d)| Some((name.clone(), classify(d)?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn described(class: Option<FontClass>, pitch: Option<FontPitch>, panose: Option<&str>) -> FontDescription {
        FontDescription { class, pitch, panose: panose.and_then(super::panose), charset: None }
    }

    #[test]
    fn a_family_is_classified_by_what_its_description_states() {
        use FontClass::{Auto, Decorative as D, Modern, Roman, Script as S, Swiss};
        use GenericFamily as G;
        let cases: &[(Option<FontClass>, Option<FontPitch>, Option<&str>, Option<GenericFamily>)] = &[
            (Some(Roman), Some(FontPitch::Variable), None, Some(G::Serif)),
            (Some(Swiss), None, None, Some(G::SansSerif)),
            (Some(Modern), None, None, Some(G::Monospace)),
            (Some(S), None, None, Some(G::Script)),
            (Some(D), None, None, Some(G::Decorative)),
            // Fixed pitch wins over the general class.
            (Some(Swiss), Some(FontPitch::Fixed), None, Some(G::Monospace)),
            // A monospaced PANOSE proportion counts as fixed pitch.
            (Some(Swiss), None, Some("020B0609020204030204"), Some(G::Monospace)),
            // With no class, the PANOSE family kind and serif style decide.
            (Some(Auto), None, Some("020B0604020202020204"), Some(G::SansSerif)),
            (None, None, Some("02020603050405020304"), Some(G::Serif)),
            (Some(Auto), None, Some("03010101010101010101"), Some(G::Script)),
            (None, None, Some("04040505050A02020702"), Some(G::Decorative)),
            (None, None, Some("05000000000000000000"), Some(G::Decorative)),
            (Some(Auto), None, Some("00000000000000000000"), None),
            (Some(Auto), Some(FontPitch::Variable), None, None),
            (None, None, None, None),
        ];
        for &(class, pitch, panose, want) in cases {
            assert_eq!(classify(&described(class, pitch, panose)), want, "{class:?} {pitch:?} {panose:?}");
        }
    }
}
