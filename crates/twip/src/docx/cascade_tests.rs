//! List counters and links on typed definitions against the XML path's

use std::rc::Rc;

use super::definitions::{ListHolders, StyleHolders};
use super::list_xml::NumberingXml;
use super::numbering::Numbering;
use super::props::ThemeColours;
use super::resolve::Resolver;
use super::style_xml::StylesPart;
use super::styles::{Styles, ThemeFonts};
use crate::document::paragraph::ListId;
use crate::format::cascade::{Cascade, Rules};
use crate::format::counters::Counters;

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn level(ilvl: u8, fmt: &str, text: &str, more: &str) -> String {
    format!(r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/>{more}<w:lvlText w:val="{text}"/></w:lvl>"#)
}

#[test]
fn lists_number_and_link_as_the_xml_path_does() {
    let numbering = format!(
        r#"<w:numbering {NS}><w:abstractNum w:abstractNumId="0">{}{}{}{}</w:abstractNum><w:abstractNum w:abstractNumId="1"><w:numStyleLink w:val="ListStyle"/></w:abstractNum><w:abstractNum w:abstractNumId="2"><w:styleLink w:val="ListStyle"/>{}{}</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="0"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride><w:lvlOverride w:ilvl="1">{}</w:lvlOverride></w:num><w:num w:numId="3"><w:abstractNumId w:val="1"/></w:num><w:num w:numId="4"><w:abstractNumId w:val="2"/></w:num><w:num w:numId="5"><w:lvlOverride w:ilvl="0">{}</w:lvlOverride></w:num></w:numbering>"#,
        level(0, "decimal", "%1.", r#"<w:pStyle w:val="H1"/>"#),
        level(1, "lowerLetter", "%1.%2)", r#"<w:lvlRestart w:val="0"/>"#),
        level(2, "upperRoman", "%1.%2.%3", "<w:isLgl/>"),
        level(3, "bullet", "o", ""),
        level(0, "decimalZero", "%1", ""),
        level(1, "ordinalText", "%1-%2", ""),
        level(1, "upperLetter", "(%2)", ""),
        level(0, "chicago", "%1", ""),
    );
    let styles = format!(
        r#"<w:styles {NS}><w:style w:type="numbering" w:styleId="ListStyle"><w:name w:val="List Style"/><w:pPr><w:numPr><w:numId w:val="4"/></w:numPr></w:pPr></w:style><w:style w:type="paragraph" w:styleId="H1"><w:name w:val="heading 1"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr></w:style><w:style w:type="paragraph" w:styleId="H2"><w:name w:val="heading 2"/><w:basedOn w:val="H1"/></w:style><w:style w:type="paragraph" w:styleId="Own"><w:name w:val="own"/><w:basedOn w:val="H1"/><w:pPr><w:numPr><w:ilvl w:val="1"/></w:numPr></w:pPr></w:style></w:styles>"#
    );
    let colours = ThemeColours::default();
    let res = Resolver::new(Styles::parse(Some(&styles), colours), Numbering::parse(Some(&numbering), &colours), ThemeFonts::default());
    let sheet = StyleHolders::parse(Some(&styles)).sheet;
    let c = Cascade::new(
        Rc::new(sheet),
        Rc::new(ListHolders::parse(Some(&numbering)).lists()),
        ThemeFonts::default(),
        colours,
        Rules::default(),
        super::style_xml::other_regions,
    );
    for id in 0..7 {
        assert_eq!(c.abstract_of(ListId(id)).map(|a| a.0), res.numbering.abstract_of(id, &res.styles), "list {id}");
    }
    let mut links: Vec<_> = c.style_links.iter().map(|((a, l), s)| ((a.0, *l), s.to_string())).collect();
    links.sort();
    let mut want: Vec<_> = res.style_links.iter().map(|(k, s)| (*k, s.clone())).collect();
    want.sort();
    assert_eq!(links, want);
    for list in 1..6 {
        for ilvl in 0..3 {
            for style in [None, Some("H1"), Some("H2"), Some("Own")] {
                for from_style in [false, true] {
                    let frozen = res.label_shows(list, ilvl, from_style, style);
                    assert_eq!(c.label_shows(ListId(list), ilvl, from_style, style), frozen, "{list} {ilvl} {style:?} {from_style}");
                }
            }
        }
    }
    let (mut frozen, mut typed) = (crate::docx::numbering::Counters::default(), Counters::default());
    let steps = [
        (1, 0),
        (1, 1),
        (1, 1),
        (1, 2),
        (2, 0),
        (2, 1),
        (2, 1),
        (1, 0),
        (1, 2),
        (3, 0),
        (4, 1),
        (3, 1),
        (5, 0),
        (5, 0),
        (1, 3),
        (2, 0),
        (6, 0),
        (1, 12),
    ];
    for (list, ilvl) in steps {
        let text = frozen.next(&res.numbering, &res.styles, list, ilvl);
        assert_eq!(typed.next(&c, ListId(list), ilvl), text, "{list} {ilvl}");
        assert_eq!(typed.values(&c, ListId(list)), frozen.values(&res.numbering, &res.styles, list), "{list} {ilvl}");
        assert_eq!(typed.value(&c, ListId(list), ilvl), frozen.value(&res.numbering, &res.styles, list, ilvl), "{list} {ilvl}");
    }
}
