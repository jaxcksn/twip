//! Automatic text's colour over the fill behind it (FONT-15): white over a dark run shading, paragraph shading, cell
//! shading or shape fill, black over a light one and over any highlight.

use super::fonts;
use super::shapes::{anchor, bytes};
use crate::layout::fragment::Item;
use crate::layout::traverse::paint_items;

/// Each glyph run's text and colour on the first page (`None`: black).
fn colours(body: &str) -> Vec<(String, Option<u32>)> {
    let doc = crate::docx::read(&bytes(body).expect("a package")).expect("the document reads");
    let snap = crate::layout::layout(&doc, &fonts());
    paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| if let Item::Glyphs(r) = i { Some((r.chars.iter().map(|c| c.0).collect(), r.colour)) } else { None })
        .filter(|(t, _): &(String, Option<u32>)| !t.trim().is_empty())
        .collect()
}

fn colour_of(body: &str, text: &str) -> Option<u32> {
    let all = colours(body);
    all.iter().find(|(t, _)| t.contains(text)).unwrap_or_else(|| panic!("{text} in {all:?}")).1
}

fn shd(fill: &str) -> String {
    format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{fill}"/>"#)
}

fn para(text: &str, ppr: &str, rpr: &str) -> String {
    format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>{text}</w:t></w:r></w:p>"#)
}

fn cell(fill: &str, content: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/>{}</w:tcPr>{content}</w:tc></w:tr></w:tbl><w:p/>"#,
        shd(fill)
    )
}

const WHITE: Option<u32> = Some(0xFFFFFF);

#[test]
fn automatic_text_turns_white_under_a_fill_of_0_30_r_0_59_g_0_11_b_below_60() {
    for (fill, want) in
        [("3B3B3B", WHITE), ("3C3C3C", None), ("C00000", WHITE), ("C80000", None), ("006400", WHITE), ("006800", None), ("0000F8", WHITE)]
    {
        assert_eq!(colour_of(&para("Shaded", &shd(fill), ""), "Shaded"), want, "paragraph shading {fill}");
        assert_eq!(colour_of(&para("Run", "", &shd(fill)), "Run"), want, "run shading {fill}");
        assert_eq!(colour_of(&cell(fill, &para("Cell", "", "")), "Cell"), want, "cell shading {fill}");
    }
}

#[test]
fn the_nearest_fill_decides_and_a_highlight_never_does() {
    // A light paragraph in a dark cell is black; a dark paragraph in a light cell white.
    assert_eq!(colour_of(&cell("1F3864", &para("Light", &shd("FFF2CC"), "")), "Light"), None);
    assert_eq!(colour_of(&cell("FFF2CC", &para("Dark", &shd("1F3864"), "")), "Dark"), WHITE);
    // A highlight isn't a fill: black over black.
    assert_eq!(colour_of(&para("Marked", "", r#"<w:highlight w:val="black"/>"#), "Marked"), None);
    // An explicit colour stays.
    assert_eq!(colour_of(&para("Red", &shd("000000"), r#"<w:color w:val="FF0000"/>"#), "Red"), Some(0xFF0000));
}

#[test]
fn automatic_text_is_white_in_a_shape_with_a_dark_fill() {
    let text_box = |fill: &str| {
        format!(
            r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{fill}"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>"#
        )
    };
    let body = |fill: &str| format!("<w:p>{}</w:p>", anchor(100, 200, 100, 50, 1, false, 5, &text_box(fill)));
    assert_eq!(colour_of(&body("1F3864"), "Boxed"), WHITE);
    assert_eq!(colour_of(&body("FFF2CC"), "Boxed"), None);
}

#[test]
fn a_filled_shapes_content_keeps_its_colours_on_a_dark_page_and_an_unfilled_ones_does_not() {
    // DRAW-27: paint marks what a filled shape draws, so the dark page leaves it alone.
    let text_box = |fill: &str| {
        format!(
            r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}</wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>"#
        )
    };
    let own = |fill: &str| {
        let body = format!("<w:p>{}</w:p>", anchor(100, 200, 100, 50, 1, false, 5, &text_box(fill)));
        let doc = crate::docx::read(&bytes(&body).expect("a package")).expect("the document reads");
        let snap = crate::layout::layout(&doc, &fonts());
        let mut found = None;
        crate::layout::traverse::paint(&snap.pages()[0], &mut |p| {
            if let Item::Glyphs(r) = p.item
                && r.chars.iter().any(|c| c.0 == 'B')
            {
                found = Some(p.own_colours);
            }
        });
        found.expect("the box's text is painted")
    };
    assert!(own(r#"<a:solidFill><a:srgbClr val="FFF2CC"/></a:solidFill>"#));
    assert!(!own("<a:noFill/>"));
}
