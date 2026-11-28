//! Font table

use std::collections::HashMap;

pub use crate::document::fonts::*;

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

/// Renames `from` to `to` in a level of typed character properties; returns how many slots changed.
pub fn rename_char(p: &mut crate::format::CharProps, rename: &FontRename) -> u32 {
    let mut n = 0;
    let f = &mut p.fonts;
    for slot in [&mut f.latin, &mut f.latin_extended, &mut f.east_asian, &mut p.east_asian_below, &mut f.complex] {
        if let Some(crate::format::FontRef::Family(name)) = slot
            && name.eq_ignore_ascii_case(&rename.from)
        {
            *name = rename.to.as_str().into();
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

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
