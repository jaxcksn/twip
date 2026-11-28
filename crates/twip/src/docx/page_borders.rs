//! Typed section page borders from and to `w:pgBorders`
//!
//! As for paragraph properties ([`super::para_props`]), the parse keeps in [`ForeignProps`] what the typed value does
//! not hold: unmodelled attributes of the element and of its edges (an art border's picture, `r:id`, and its corner
//! pictures), modelled attributes written otherwise than the writer writes their value, unmodelled children and later
//! duplicates of an edge (the first one counts) whole, and an edge with content whole beside its typed value.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::rc::Rc;

use roxmltree::Node;

use super::as_read::AsRead;
use super::model::{PageBorders, Section, SectionPage};
use super::prop_xml::{Compare, Out, Sink, border, border_attrs, foreign_child, push_esc};
use super::props::ThemeColours;
use super::review::SectionChange;
use super::write::props::Cx;
use super::xml::{W, attr, child};
use crate::document::authored::Authored;
use crate::document::section::PageBorderDisplay;
use crate::format::{ForeignAttr, ForeignChild, ForeignProps, PageBorderOffset, PageBorderProps, PageBorderZOrder, QName, TextBorder};

/// CT_PageBorders' edges in schema order.
const EDGES: [&str; 4] = ["top", "left", "bottom", "right"];

/// The modelled attributes of the element and of an edge, in the order they are written.
const WRITTEN: [&str; 11] = ["offsetFrom", "display", "zOrder", "val", "sz", "space", "color", "themeColor", "themeTint", "themeShade", "shadow"];

pub use crate::format::PageBordersDirect;

/// What the WordprocessingML adapter adds to a section's authored page borders: the `w:pgBorders` they were read from
/// and are written as. The written form stands for the borders only while they are the ones it was read as.
pub trait PageBordersXml {
    /// Borders read from `xml`, the `w:pgBorders` they were parsed from.
    fn read(props: PageBorderProps, xml: Rc<str>) -> Self;

    /// The `w:pgBorders` as read, while the borders are the ones read from it.
    fn as_read(&self) -> Option<&Rc<str>>;

    /// The `w:pgBorders` to write in the part `cx` describes: as read while the borders are those, else written from
    /// them (a theme colour's value as `colours` resolve it); `None` for none.
    fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<Cow<'_, str>>;

    /// The same typed borders, written alike.
    fn written_alike(&self, o: &Self) -> bool;
}

impl PageBordersXml for PageBordersDirect {
    fn read(props: PageBorderProps, xml: Rc<str>) -> PageBordersDirect {
        let props = Rc::new(props);
        Authored::new(Some(props.clone()), AsRead::origin(props, xml))
    }

    fn as_read(&self) -> Option<&Rc<str>> {
        let r = AsRead::<PageBorderProps>::of(self.origin())?;
        let now = self.get().as_ref()?;
        (Rc::ptr_eq(&r.read, now) || *r.read == **now).then_some(&r.xml)
    }

    fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<Cow<'_, str>> {
        if let Some(x) = self.as_read() {
            return Some(Cow::Borrowed(x));
        }
        self.get().as_deref().map(|p| Cow::Owned(serialize(p, colours, cx)))
    }

    fn written_alike(&self, o: &PageBordersDirect) -> bool {
        self.authored_eq(o) && self.as_read() == o.as_read()
    }
}

/// Whether pages `a` and `b` are the same authored page, their borders written alike.
pub fn same_page(a: &SectionPage, b: &SectionPage) -> bool {
    a.authored_eq(b) && a.page_borders_direct.as_read() == b.page_borders_direct.as_read()
}

/// Whether tracked section changes `a` and `b` are the same authored change, the borders before them written alike.
pub fn same_change(a: Option<&SectionChange>, b: Option<&SectionChange>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.authored_eq(b) && a.previous.page_borders_direct.as_read() == b.previous.page_borders_direct.as_read(),
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// Whether sections `a` and `b` are the same authored section, their borders and those before a tracked change
/// written alike.
pub fn same_section(a: &Section, b: &Section) -> bool {
    a.authored_eq(b) && a.page_borders_direct.as_read() == b.page_borders_direct.as_read() && same_change(a.change.as_deref(), b.change.as_deref())
}

/// The page borders `b` (a `w:pgBorders`) draw, their colours as `colours` resolve them, around the header and the
/// footer as given.
pub fn effective(b: Node, colours: &ThemeColours, surround_header: bool, surround_footer: bool) -> PageBorders {
    let side = |n| child(b, n).map(|n| super::props::border(n, colours));
    PageBorders {
        top: side("top"),
        left: side("left"),
        bottom: side("bottom"),
        right: side("right"),
        offset_from_page: attr(b, "offsetFrom") == Some("page"),
        display: attr(b, "display").map_or(PageBorderDisplay::AllPages, super::section::page_border_display),
        surround_header,
        surround_footer,
        behind_text: attr(b, "zOrder") == Some("back"),
    }
}

fn edges(p: &PageBorderProps) -> [&Option<TextBorder>; 4] {
    [&p.top, &p.left, &p.bottom, &p.right]
}

fn edges_mut(p: &mut PageBorderProps) -> [&mut Option<TextBorder>; 4] {
    [&mut p.top, &mut p.left, &mut p.bottom, &mut p.right]
}

/// The edge of [`EDGES`] an element is, by its name.
fn edge_of(ns: &str, local: &str) -> Option<usize> {
    (ns == W).then(|| EDGES.iter().position(|e| *e == local)).flatten()
}

// Parse

/// A `w:pgBorders` as typed borders.
pub fn parse(e: Node) -> PageBorderProps {
    let mut p = PageBorderProps {
        offset_from: attr(e, "offsetFrom").map(|v| if v == "page" { PageBorderOffset::Page } else { PageBorderOffset::Text }),
        display: attr(e, "display").map(super::section::page_border_display),
        z_order: attr(e, "zOrder").map(|v| if v == "back" { PageBorderZOrder::Back } else { PageBorderZOrder::Front }),
        ..PageBorderProps::default()
    };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(e, &mut scratch);
    emit(&p, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let mut seen = [false; EDGES.len()];
    for c in e.children().filter(|c| c.is_element()) {
        let Some(k) = edge_of(c.tag_name().namespace().unwrap_or_default(), c.tag_name().name()).filter(|k| !seen[*k]) else {
            f.children.push(foreign_child(c));
            continue;
        };
        seen[k] = true;
        let b = border(c);
        if c.children().any(|n| n.is_element() || n.text().is_some_and(|t| !t.trim().is_empty())) {
            f.kept.push(foreign_child(c));
        } else {
            let mut cmp = Compare::new(c, &mut scratch);
            border_attrs(&mut cmp, &b);
            cmp.finish(Some(QName { ns: W.into(), local: EDGES[k].into() }), &mut f.attributes);
        }
        *edges_mut(&mut p)[k] = Some(b);
    }
    // In the order they are written: the element's, then each edge's in schema order, modelled ones in their places
    let place = |a: &ForeignAttr| {
        let on = a.on.as_ref().map_or(0, |q| 1 + edge_of(&q.ns, &q.local).unwrap_or(0));
        let tied = a.stands_for.is_some() || a.beside.is_some();
        (on, WRITTEN.iter().position(|n| tied && *n == &*a.name.local).unwrap_or(WRITTEN.len()))
    };
    f.attributes.sort_by_key(place);
    f.kept.sort_by_key(|c| edge_of(&c.name.ns, &c.name.local));
    if f != ForeignProps::default() {
        p.foreign = Some(Rc::new(f));
    }
    p
}

/// The element's own modelled attributes, in the order the Page Borders command writes them.
fn emit(p: &PageBorderProps, s: &mut dyn Sink) {
    if let Some(o) = p.offset_from {
        s.attr(
            W,
            "offsetFrom",
            &match o {
                PageBorderOffset::Text => "text",
                PageBorderOffset::Page => "page",
            },
        );
    }
    if let Some(d) = p.display {
        s.attr(W, "display", &super::section::page_border_display_literal(d));
    }
    if let Some(z) = p.z_order {
        s.attr(
            W,
            "zOrder",
            &match z {
                PageBorderZOrder::Front => "front",
                PageBorderZOrder::Back => "back",
            },
        );
    }
}

/// The edge read from kept edge `c` alone.
fn read_edge(c: &ForeignChild) -> Option<TextBorder> {
    let mut wrapper = String::from("<x");
    for (p, u) in c.bindings.iter() {
        wrapper.push_str(if p.is_empty() { " xmlns" } else { " xmlns:" });
        wrapper.push_str(p);
        wrapper.push_str("=\"");
        push_esc(&mut wrapper, u);
        wrapper.push('"');
    }
    let _ = write!(wrapper, ">{}</x>", c.payload);
    let doc = roxmltree::Document::parse(&wrapper).ok()?;
    doc.root_element().first_element_child().map(border)
}

// Serialize

/// `p` as a `w:pgBorders` in the part `cx` describes; a theme colour's value is written as `colours` resolve it.
pub fn serialize(p: &PageBorderProps, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = p.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}pgBorders{decls}");
    let written = out.attrs(W, attrs, None, |s| emit(p, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    for (k, b) in edges(p).into_iter().enumerate() {
        let Some(b) = b else { continue };
        let side = EDGES[k];
        let kept = foreign.and_then(|f| f.kept.iter().find(|c| edge_of(&c.name.ns, &c.name.local) == Some(k)));
        if let Some(c) = kept.filter(|c| read_edge(c).as_ref() == Some(b)) {
            out.payload(c);
            continue;
        }
        let on = Some((W, side));
        let _ = write!(out.s, "<{w}{side}");
        let written = out.attrs(W, attrs, on, |s| border_attrs(s, b));
        out.free_attrs(attrs, on, &written);
        out.s.push_str("/>");
    }
    for c in foreign.map_or(&[][..], |f| &f.children) {
        out.payload(c);
    }
    let _ = write!(out.s, "</{w}pgBorders>");
    out.s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::line::BorderStyle;
    use crate::format::Color;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:x="urn:x""#;

    fn cx() -> Cx {
        Cx::of_xml(&format!("<w:document {NS}>"))
    }

    fn read(x: &str) -> PageBorderProps {
        cx().parse(x, |_, el| parse(el)).expect("parses")
    }

    /// `x` written again after `edit`, with what the writing lost of it; unedited, the writing parses to the same
    /// typed borders and draws the same borders.
    fn rewritten(x: &str, edit: impl FnOnce(&mut PageBorderProps)) -> (String, Vec<String>) {
        let (cx, colours) = (cx(), ThemeColours::default());
        let read = read(x);
        let mut p = read.clone();
        edit(&mut p);
        let xml = serialize(&p, &colours, &cx);
        if p == read {
            assert_eq!(cx.parse(&xml, |_, el| parse(el)).as_ref(), Some(&read), "typed, written as {xml}");
            let drawn = |x: &str| cx.parse(x, |_, el| effective(el, &colours, true, true));
            assert_eq!(drawn(&xml), drawn(x), "drawn, written as {xml}");
        }
        let lost = cx.parse(x, |_, had| cx.parse(&xml, |_, has| super::super::char_props::tests::lost(had, has))).flatten().expect("parses");
        (xml, lost)
    }

    const ART: &str = concat!(
        r#"<w:pgBorders w:offsetFrom="page" x:a="1" zOrder="back">"#,
        r#"<w:top w:val="apples" w:sz="20" w:space="24" w:color="auto" r:id="rId9" r:topLeft="rId10" r:topRight="rId11" w:frame="1"/>"#,
        r#"<w:left w:val="single" w:sz="0.5" w:space="24" w:color="FF0000" w:themeColor="accent2" w:shadow="true"/>"#,
        r#"<w:bottom w:val="single" w:sz="4" w:space="24" w:color="bogus" w:shadow="0" r:id="rId12"/>"#,
        r#"<w:top w:val="double" w:sz="8"/><x:mine x:b="2"><x:deep/></x:mine>"#,
        r#"</w:pgBorders>"#
    );

    #[test]
    fn what_isnt_modelled_is_kept_beside_the_typed_borders() {
        let p = read(ART);
        assert_eq!((p.offset_from, p.z_order, p.display), (Some(PageBorderOffset::Page), Some(PageBorderZOrder::Back), None));
        let top = p.top.expect("the first top edge");
        assert_eq!((top.style, top.width, top.color), (super::super::line_style::border_style("apples"), 2.5, Some(Color::Auto)));
        assert!(p.left.is_some_and(|l| l.shadow && l.width == 0.125 && matches!(l.color, Some(Color::Theme(_)))));
        assert!(p.right.is_none());
        let (xml, lost) = rewritten(ART, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
    }

    #[test]
    fn an_edge_edited_keeps_what_isnt_modelled_and_values_written_otherwise_until_they_change() {
        let (xml, lost) = rewritten(ART, |p| {
            p.left.as_mut().expect("left").space = 12.0;
            p.bottom.as_mut().expect("bottom").width = 1.0;
            p.offset_from = Some(PageBorderOffset::Text);
        });
        assert!(xml.contains(r#"r:id="rId9" r:topLeft="rId10" r:topRight="rId11" w:frame="1""#) && xml.contains(r#"r:id="rId12""#), "{xml}");
        assert!(xml.contains(r#"<w:left w:val="single" w:sz="0.5" w:space="12" w:color="FF0000" w:themeColor="accent2" w:shadow="true"/>"#), "{xml}");
        assert!(xml.contains(r#"<w:bottom w:val="single" w:sz="8" w:space="24" w:color="bogus" w:shadow="0" r:id="rId12"/>"#), "{xml}");
        assert!(xml.starts_with(r#"<w:pgBorders w:offsetFrom="text" zOrder="back" x:a="1">"#), "{xml}");
        let changed =
            [format!(" @{{{W}}}offsetFrom=\"page\""), format!("{{{W}}}bottom @{{{W}}}sz=\"4\""), format!("{{{W}}}left @{{{W}}}space=\"24\"")];
        assert_eq!(lost, changed, "{xml}");
        let (xml, _) = rewritten(ART, |p| p.left.as_mut().expect("left").color = Some(Color::Rgb(0x00FF00)));
        assert!(xml.contains(r#"<w:left w:val="single" w:sz="0.5" w:space="24" w:color="00FF00" w:shadow="true"/>"#), "{xml}");
    }

    #[test]
    fn edges_and_attributes_out_of_order_read_alike_written_in_order() {
        let x = r#"<w:pgBorders x:a="1" w:zOrder="front" w:display="allpages"><w:right r:id="rId2" w:val="single" w:shadow="off"/><w:left w:val="dotted" w:color="auto" r:id="rId1" w:sz="4"><x:ext/></w:left><w:top w:val="single" w:shadow="false"/></w:pgBorders>"#;
        let (xml, lost) = rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let at = |n: &str| xml.find(n).unwrap_or_else(|| panic!("{n} in {xml}"));
        assert!(at("<w:top") < at("<w:left") && at("<w:left") < at("<w:right"), "{xml}");
    }

    #[test]
    fn an_edge_with_content_is_written_as_read_until_it_changes() {
        let x = r#"<w:pgBorders><w:top w:val="single" w:sz="4"><x:ext/></w:top></w:pgBorders>"#;
        let (xml, lost) = rewritten(x, |_| {});
        assert_eq!((xml.as_str(), lost), (x, Vec::new()));
        let (xml, _) = rewritten(x, |p| p.top.as_mut().expect("top").width = 1.0);
        assert_eq!(xml, r#"<w:pgBorders><w:top w:val="single" w:sz="8" w:space="0"/></w:pgBorders>"#);
    }

    #[test]
    fn borders_are_written_as_read_until_they_change_and_typed_alone_from_the_typed_form() {
        let (cx, colours) = (cx(), ThemeColours::default());
        let x = r#"<w:pgBorders  w:offsetFrom='page'><w:left w:val="single" w:sz="4" w:space="24" w:color="auto"/></w:pgBorders>"#;
        let read = PageBordersDirect::read(super::tests::read(x), x.into());
        assert_eq!(read.xml(&colours, &cx).as_deref(), Some(x));
        let typed = PageBordersDirect::typed(read.get().clone());
        assert!(read.authored_eq(&typed) && !read.written_alike(&typed), "written otherwise");
        assert_eq!(
            typed.xml(&colours, &cx).as_deref(),
            Some(r#"<w:pgBorders w:offsetFrom="page"><w:left w:val="single" w:sz="4" w:space="24" w:color="auto"/></w:pgBorders>"#)
        );
        let theme = crate::docx::model::ThemeColourRef { slot: crate::drawing::colour::SchemeSlot::Accent(1), tint: Some(0x66), shade: None };
        let edge = TextBorder { style: BorderStyle::Wave, width: 0.75, space: 4.0, shadow: true, color: Some(Color::Theme(theme)) };
        let set = PageBordersDirect::typed(Some(Rc::new(PageBorderProps {
            bottom: Some(edge),
            display: Some(PageBorderDisplay::FirstPage),
            z_order: Some(PageBorderZOrder::Back),
            ..PageBorderProps::default()
        })));
        let rgb = colours.resolve(theme);
        assert_eq!(
            set.xml(&colours, &cx).as_deref(),
            Some(&*format!(
                r#"<w:pgBorders w:display="firstPage" w:zOrder="back"><w:bottom w:val="wave" w:sz="6" w:space="4" w:color="{rgb:06X}" w:themeColor="accent1" w:themeTint="66" w:shadow="1"/></w:pgBorders>"#
            ))
        );
        assert_eq!(PageBordersDirect::typed(None).xml(&colours, &cx), None);
    }
}
