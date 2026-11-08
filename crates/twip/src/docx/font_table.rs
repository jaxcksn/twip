//! Font table

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

fn panose(hex: &str) -> Option<[u8; 10]> {
    if hex.len() != 20 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 10];
    for (k, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).ok()?;
    }
    Some(out)
}

/// The descriptions in a font table part, by lowercase family name. Malformed XML gives none.
pub fn parse(xml: &str) -> HashMap<String, FontDescription> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return HashMap::new() };
    let mut out = HashMap::new();
    for font in doc.root_element().children().filter(|n| n.has_tag_name((W, "font"))) {
        let Some(name) = font.attribute((W, "name")).filter(|n| !n.is_empty()) else { continue };
        let val = |tag: &str| font.children().find(|n| n.has_tag_name((W, tag))).and_then(|n| n.attribute((W, "val")));
        let description = FontDescription {
            class: val("family").and_then(|v| match v {
                "roman" => Some(FontClass::Roman),
                "swiss" => Some(FontClass::Swiss),
                "modern" => Some(FontClass::Modern),
                "script" => Some(FontClass::Script),
                "decorative" => Some(FontClass::Decorative),
                "auto" => Some(FontClass::Auto),
                _ => None,
            }),
            pitch: val("pitch").and_then(|v| match v {
                "fixed" => Some(FontPitch::Fixed),
                "variable" => Some(FontPitch::Variable),
                "default" => Some(FontPitch::Default),
                _ => None,
            }),
            panose: val("panose1").and_then(panose),
            charset: val("charset").and_then(|v| u8::from_str_radix(v, 16).ok()),
        };
        out.entry(name.to_lowercase()).or_insert(description);
    }
    out
}

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

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

/// A family the document's tables were rewritten from, and to: the save rewrites its styles, numbering, theme and
/// font table parts the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontRename {
    pub from: String,
    pub to: String,
}

/// Renames `from` to `to` in a level of run properties; returns how many slots changed.
pub fn rename_props(p: &mut super::props::RunProps, rename: &FontRename) -> u32 {
    let mut n = 0;
    for slot in [&mut p.ascii, &mut p.h_ansi, &mut p.east_asia, &mut p.east_asia_below, &mut p.cs] {
        if let Some(super::props::FontRef::Name(name)) = slot
            && name.eq_ignore_ascii_case(&rename.from)
        {
            *name = rename.to.clone();
            n += 1;
        }
    }
    n
}

/// Renames `from` to `to` in the style, numbering and theme tables a resolver holds, and in its categories. Returns
/// the places that changed: styles, document defaults, numbering levels and theme slots.
pub fn rename_tables(res: &mut super::resolve::Resolver, rename: &FontRename) -> u32 {
    let mut n = 0;
    let mut place = |changed: u32| n += u32::from(changed > 0);
    place(rename_props(&mut res.styles.doc_rpr, rename));
    for style in res.styles.by_id.values_mut() {
        let mut k = rename_props(&mut style.base.rpr, rename);
        for region in style.regions.values_mut() {
            k += rename_props(&mut region.rpr, rename);
        }
        place(k);
    }
    for level in res.numbering.abstracts.values_mut().flat_map(|a| a.levels.iter_mut().flatten()) {
        place(rename_props(&mut level.rpr, rename));
    }
    let t = &mut res.theme;
    for slot in [&mut t.major, &mut t.minor] {
        if slot.eq_ignore_ascii_case(&rename.from) {
            *slot = rename.to.clone();
            place(1);
        }
    }
    for slot in [&mut t.major_ea, &mut t.minor_ea, &mut t.major_cs, &mut t.minor_cs].into_iter().flatten() {
        if slot.eq_ignore_ascii_case(&rename.from) {
            *slot = rename.to.clone();
            place(1);
        }
    }
    let from = rename.from.to_lowercase();
    if let Some(category) = res.categories.get(&from).copied() {
        let mut categories = (*res.categories).clone();
        categories.remove(&from);
        categories.entry(rename.to.to_lowercase()).or_insert(category);
        res.categories = std::rc::Rc::new(categories);
    }
    n
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

    #[test]
    fn a_font_table_reads_leniently() {
        let xml = r#"<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:font w:name="Garamond"><w:panose1 w:val="02020404030301010803"/><w:charset w:val="00"/><w:family w:val="roman"/><w:pitch w:val="variable"/></w:font>
            <w:font w:name="Odd"><w:panose1 w:val="XYZ"/><w:family w:val="fancy"/><w:pitch w:val="sometimes"/></w:font>
            <w:font w:name="garamond"><w:family w:val="swiss"/></w:font>
        </w:fonts>"#;
        let table = parse(xml);
        let garamond = &table["garamond"];
        assert_eq!((garamond.class, garamond.pitch, garamond.charset), (Some(FontClass::Roman), Some(FontPitch::Variable), Some(0)));
        assert_eq!(garamond.panose, Some([2, 2, 4, 4, 3, 3, 1, 1, 8, 3]));
        assert_eq!(table["odd"], FontDescription::default());
        assert!(parse("<not xml").is_empty());
    }
}
