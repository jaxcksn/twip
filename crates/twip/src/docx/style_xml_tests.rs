//! Typed style definitions: the adapter's parse, serializer and holders, and what the typed definitions convert to
//! against the XML path's reading of the same part

use std::rc::Rc;

use super::char_props::tests::lost;
use super::props::ThemeColours;
use super::style_xml::{self as sx, DefaultsDef, LatentDef, StyleDef, StylesPart};
use super::styles::{self, Styles};
use super::write::props::Cx;
use super::xml::W;
use crate::format::{Color, ParaProp, ParaSet, RegionKind, StyleKind, StyleProp, StyleSet};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:x="urn:x""#;

fn part(body: &str) -> String {
    format!("<w:styles {NS}>{body}</w:styles>")
}

fn cx() -> Cx {
    Cx::of_xml(&format!("<w:styles {NS}>"))
}

fn style(x: &str) -> crate::format::Style {
    cx().parse(x, |_, el| sx::parse(el)).flatten().expect("a style")
}

/// The XML path's and the typed reader's readings of `xml`, every style converted and compared.
fn both(xml: &str, colours: &ThemeColours) -> (Styles, StylesPart) {
    let frozen = Styles::parse(Some(xml), *colours);
    let typed = StylesPart::parse(Some(xml));
    sheet_matches(&frozen, &typed, colours, "inline");
    (frozen, typed)
}

// Checks

/// What the round trip checked, by element.
#[derive(Default, Debug)]
pub(crate) struct Counts {
    pub(crate) styles: usize,
    pub(crate) regions: usize,
    pub(crate) defaults: usize,
    pub(crate) latent: usize,
}

/// `el` (a `w:style`, `w:docDefaults` or `w:latentStyles` of a part `cx` describes): its holder writes it as read,
/// written over itself with nothing set it is as read, and its typed value written again by the serializer parses to
/// the same typed value, the writing holding every element, text and attribute value of `el`.
pub(crate) fn round_trip(el: roxmltree::Node, cx: &Cx, colours: &ThemeColours, what: &str, n: &mut Counts) {
    let raw = &el.document().input_text()[el.range()];
    macro_rules! check {
        ($holder:ty, $parse:expr, $serialize:expr, $over:expr) => {{
            let typed = $parse(el);
            let held = <$holder>::read(typed.clone(), raw.into());
            assert_eq!(held.xml(colours, cx), raw, "{what}: as read");
            assert_eq!($over(&typed, Some((&typed, raw))), raw, "{what}: written over itself");
            let xml = $serialize(&typed);
            let again = cx.parse(&xml, |_, e| $parse(e)).unwrap_or_else(|| panic!("{what}: written as {xml}, parses"));
            assert_eq!(again, typed, "{what}: typed, written as {xml}");
            let gone = cx.parse(&xml, |_, e| lost(el, e)).unwrap_or_default();
            assert!(gone.is_empty(), "{what}: written as {xml}, loses {gone:?}");
            typed
        }};
    }
    match el.tag_name().name() {
        "style" => {
            let over = |t: &crate::format::Style, o| sx::written_over(t, colours, cx, o, &[]);
            let typed = check!(StyleDef, |e| sx::parse(e).unwrap_or_default(), |t| sx::serialize(t, colours, cx), over);
            edits(&typed, raw, cx, colours, what);
            n.styles += 1;
            n.regions += typed.regions.len();
        }
        "docDefaults" => {
            check!(DefaultsDef, sx::parse_defaults, |t| sx::serialize_defaults(t, colours, cx), |t, o| sx::defaults_over(t, colours, cx, o, &[]));
            n.defaults += 1;
        }
        _ => {
            check!(LatentDef, sx::parse_latent, |t| sx::serialize_latent(t, colours, cx), |t, o| sx::latent_over(t, colours, cx, o));
            n.latent += 1;
        }
    }
}

/// Style `s`, read from `raw`, after each of a few edits: written over `raw`, it parses to the edited style.
fn edits(s: &crate::format::Style, raw: &str, cx: &Cx, colours: &ThemeColours, what: &str) {
    use crate::format::{Prop, Set, Toggle};
    let held = StyleDef::read(s.clone(), raw.into());
    let check = |edited: StyleDef, which: &str| {
        let xml = edited.xml(colours, cx);
        let again = cx.parse(&xml, |_, e| sx::parse(e)).flatten().unwrap_or_else(|| panic!("{what}: {which}: written as {xml}, parses"));
        assert_eq!(&again, &**edited.get(), "{what}: {which}: written as {xml}");
    };
    let mut q = s.clone();
    q.quick = Some(q.quick != Some(true));
    q.based_on = None;
    check(held.edited(q, &[StyleSet::Style(StyleProp::Quick), StyleSet::Style(StyleProp::BasedOn)]), "own properties");
    let mut q = s.clone();
    q.format.para.space_after = Some(7.0);
    q.format.char.toggles.put(Toggle::BOLD, true);
    let sets = [
        StyleSet::Para(None, ParaSet { prop: ParaProp::SpaceAfter, whole: false }),
        StyleSet::Char(None, Set { prop: Prop::Toggle(Toggle::BOLD), whole: true }),
    ];
    check(held.edited(q, &sets), "base");
    if let Some(r) = s.regions.first() {
        let mut q = s.clone();
        q.regions[0].format.char.size = Some(13.0);
        let mut set = vec![StyleSet::Char(Some(r.kind), Set { prop: Prop::Size, whole: true })];
        if q.regions.len() > 1 {
            set.extend(q.regions.pop().map(|gone| StyleSet::Region(gone.kind)));
        }
        check(held.edited(q, &set), "regions");
    }
}

/// What the conversion checked.
#[derive(Default, Debug)]
pub(crate) struct SheetCounts {
    pub(crate) styles: usize,
    pub(crate) table_styles: usize,
    pub(crate) regions: usize,
    pub(crate) frozen: usize,
    pub(crate) run_defaults: usize,
    pub(crate) exceptions: usize,
}

fn region_matches(a: &styles::Region, b: &styles::Region, what: &str) {
    assert_eq!(a.ppr, b.ppr, "{what}: paragraph properties");
    assert_eq!(a.rpr, b.rpr, "{what}: run properties");
    assert_eq!(a.tbl, b.tbl, "{what}: table properties");
    assert_eq!(a.tc, b.tc, "{what}: cell properties");
    assert_eq!(a.tr, b.tr, "{what}: row properties");
    assert_eq!(a.size_over_normal, b.size_over_normal, "{what}: size over Normal");
    assert_eq!(a.sets_text, b.sets_text, "{what}: sets text");
    assert_eq!(a.mark_at_style, b.mark_at_style, "{what}: mark at the style's size");
}

fn style_matches(a: &styles::Style, b: &styles::Style, what: &str) {
    let fields = |s: &styles::Style| {
        (s.id.clone(), s.name.clone(), s.aliases.clone(), s.quick, s.kind, s.based_on.clone(), s.next.clone(), s.default, s.link.clone(), s.priority)
    };
    assert_eq!(fields(a), fields(b), "{what}: fields");
    assert_eq!((a.meta, a.custom, a.origin, a.frozen, a.order), (b.meta, b.custom, b.origin, b.frozen, b.order), "{what}: flags");
    assert_eq!(a.raw, b.raw, "{what}: as written");
    region_matches(&a.base, &b.base, &format!("{what}: base"));
    let keys = |s: &styles::Style| {
        let mut k: Vec<String> = s.regions.keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(a), keys(b), "{what}: regions");
    for (k, r) in &a.regions {
        region_matches(r, &b.regions[k], &format!("{what}: {k}"));
    }
}

/// `typed`, a styles part read by the typed reader, converts to what the XML path's reader holds of the same part.
pub(crate) fn sheet_matches(frozen: &Styles, typed: &StylesPart, colours: &ThemeColours, what: &str) -> SheetCounts {
    let sheet = &typed.sheet;
    let mut ids: Vec<&String> = frozen.by_id.keys().collect();
    ids.sort();
    let mut typed_ids: Vec<String> = sheet.styles.keys().map(|k| k.to_string()).collect();
    typed_ids.sort();
    assert_eq!(ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), typed_ids, "{what}: style ids");
    let mut n = SheetCounts::default();
    for id in ids {
        let e = &sheet.styles[id.as_str()];
        let held = &typed.styles[id.as_str()];
        assert!(Rc::ptr_eq(&e.style, held.get()), "{what}: {id}: the sheet's style is its holder's");
        style_matches(&sx::frozen_style(e, held.as_read().expect("as read"), colours), &frozen.by_id[id], &format!("{what}: {id}"));
        n.styles += 1;
        n.regions += e.style.regions.len();
        n.table_styles += usize::from(e.style.kind == StyleKind::Table);
        n.frozen += usize::from(e.frozen);
        n.run_defaults += usize::from(e.word_run_defaults);
    }
    let text = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
    assert_eq!(sheet.paragraph_order.iter().map(|s| s.to_string()).collect::<Vec<_>>(), frozen.paragraph_order, "{what}: paragraph order");
    assert_eq!(text(&sheet.default_paragraph), frozen.default_paragraph, "{what}: default paragraph style");
    assert_eq!(text(&sheet.default_table), frozen.default_table, "{what}: default table style");
    let (rpr, ppr) = sx::frozen_defaults(sheet.defaults.as_deref(), colours);
    assert_eq!(rpr, frozen.doc_rpr, "{what}: run defaults");
    assert_eq!(ppr, frozen.doc_ppr, "{what}: paragraph defaults");
    let defaults_raw = typed.defaults.as_ref().and_then(|d| d.as_read()).map(|x| x.to_string());
    assert_eq!(defaults_raw, frozen.defaults_raw, "{what}: defaults as written");
    let latent_raw = typed.latent.as_ref().and_then(|d| d.as_read()).map(|x| &**x);
    assert_eq!(sx::frozen_latent(sheet.latent.as_deref(), latent_raw), frozen.latent, "{what}: latent styles");
    n.exceptions += frozen.latent.exceptions.len();
    assert_eq!(&*typed.root, frozen.root, "{what}: root");
    n
}

// The reader's rules

#[test]
fn a_name_splits_into_the_name_and_its_aliases() {
    let x = r#"<w:style w:type="paragraph" w:styleId="M"><w:name w:val="Memo Body, MB,,X "/></w:style>"#;
    let s = style(x);
    assert_eq!(s.name.as_deref(), Some("Memo Body"));
    assert_eq!(s.aliases.iter().map(|a| &**a).collect::<Vec<_>>(), ["MB", "X"]);
    let xml = sx::serialize(&s, &ThemeColours::default(), &cx());
    assert!(xml.contains(r#"<w:name w:val="Memo Body, MB,,X "/>"#), "the name as written while it stands for the value: {xml}");
    let mut renamed = s.clone();
    renamed.name = Some("Memo".into());
    let xml = sx::serialize(&renamed, &ThemeColours::default(), &cx());
    assert!(xml.contains(r#"<w:name w:val="Memo,MB,X"/>"#), "{xml}");
    both(&part(x), &ThemeColours::default());
}

#[test]
fn an_unknown_type_is_a_paragraph_style_written_as_read() {
    let x = r#"<w:style w:type="list" w:default="true" w:customStyle="off" w:styleId="L"><w:name w:val="L"/></w:style>"#;
    let s = style(x);
    assert_eq!((s.kind, s.default, s.custom), (StyleKind::Paragraph, Some(true), Some(false)));
    let xml = sx::serialize(&s, &ThemeColours::default(), &cx());
    assert!(xml.starts_with(r#"<w:style w:type="list" w:default="true" w:customStyle="off" w:styleId="L">"#), "{xml}");
    let mut changed = s.clone();
    changed.kind = StyleKind::Character;
    let xml = sx::serialize(&changed, &ThemeColours::default(), &cx());
    assert!(xml.starts_with(r#"<w:style w:type="character""#), "{xml}");
    let (frozen, _) = both(&part(x), &ThemeColours::default());
    assert_eq!(frozen.default_paragraph.as_deref(), Some("L"));
}

#[test]
fn the_first_child_of_each_name_counts_and_the_last_region_of_each_kind() {
    let x = concat!(
        r#"<w:style w:type="table" w:styleId="T"><w:name w:val="A"/><w:name w:val="B"/><w:basedOn/><w:basedOn w:val="Z"/><w:uiPriority w:val="x"/>"#,
        r#"<w:rPr><w:b/></w:rPr><w:rPr><w:i/></w:rPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr></w:tblStylePr>"#,
        r#"<w:tblStylePr w:type="firstRow"><w:rPr><w:i/></w:rPr></w:tblStylePr><w:tblStylePr w:type="diagonal"><w:rPr><w:u/></w:rPr></w:tblStylePr>"#,
        r#"<w:tblStylePr><w:rPr><w:u/></w:rPr></w:tblStylePr></w:style>"#
    );
    let s = style(x);
    assert_eq!((s.name.as_deref(), s.based_on.as_deref(), s.priority), (Some("A"), None, None));
    assert_eq!(s.format.char.toggles.get(crate::format::Toggle::BOLD), Some(true));
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].format.char.toggles.get(crate::format::Toggle::ITALIC), Some(true));
    let again = cx().parse(&sx::serialize(&s, &ThemeColours::default(), &cx()), |_, e| sx::parse(e)).flatten();
    assert_eq!(again.as_ref(), Some(&s));
    let (frozen, _) = both(&part(x), &ThemeColours::default());
    assert!(frozen.by_id["T"].regions.contains_key("diagonal"), "a region of another type the XML path keeps");
}

#[test]
fn a_repeated_id_is_the_last_style_at_its_place() {
    let x = part(concat!(
        r#"<w:style w:type="paragraph" w:styleId="A"><w:name w:val="first"/></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="B"><w:name w:val="B"/></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="A"><w:name w:val="second"/></w:style>"#
    ));
    let (_, typed) = both(&x, &ThemeColours::default());
    let a = &typed.sheet.styles["A"];
    assert_eq!((a.style.name.as_deref(), a.order), (Some("second"), Some(2)));
    assert_eq!(typed.sheet.paragraph_order.iter().map(|s| &**s).collect::<Vec<_>>(), ["B", "A"]);
}

#[test]
fn a_table_style_read_before_its_base_is_frozen_and_one_with_run_properties_takes_words() {
    let x = part(concat!(
        r#"<w:style w:type="table" w:styleId="Child"><w:name w:val="Child"/><w:basedOn w:val="Base"/><w:rPr><w:b/></w:rPr>"#,
        r#"<w:tblStylePr w:type="firstRow"><w:rPr><w:i/></w:rPr></w:tblStylePr></w:style>"#,
        r#"<w:style w:type="table" w:styleId="Base"><w:name w:val="Base"/><w:tblPr><w:tblInd w:w="100" w:type="dxa"/></w:tblPr></w:style>"#,
        r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:basedOn w:val="Base"/>"#,
        r#"<w:rPr><w:sz w:val="40"/></w:rPr></w:style>"#,
        r#"<w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#
    ));
    let (frozen, typed) = both(&x, &ThemeColours::default());
    let child = &typed.sheet.styles["Child"];
    assert!(child.frozen && child.word_run_defaults);
    assert_eq!(child.style.format.char.size, None, "the typed style holds what it sets");
    assert_eq!(frozen.by_id["Child"].base.rpr.size, Some(10.0));
    let normal = &typed.sheet.styles["TableNormal"];
    assert!(!normal.frozen && sx::word_table_normal(&normal.style));
    assert_eq!(normal.style.based_on.as_deref(), Some("Base"), "as written");
    assert_eq!((typed.sheet.default_table.as_deref(), typed.sheet.default_paragraph.as_deref()), (Some("TableNormal"), Some("Normal")));
}

#[test]
fn property_elements_keep_what_their_adapters_leave_to_other_nodes() {
    let x = concat!(
        r#"<w:style w:type="paragraph" w:styleId="P"><w:name w:val="P"/><w:pPr><w:jc w:val="center"/><w:rPr><w:sz w:val="30"/></w:rPr>"#,
        r#"<w:pPrChange w:id="1" w:author="a"><w:pPr/></w:pPrChange></w:pPr><w:rPr><w:rPrChange w:id="2" w:author="a"><w:rPr><w:b/></w:rPr>"#,
        r#"</w:rPrChange></w:rPr></w:style>"#
    );
    let s = style(x);
    let xml = sx::serialize(&s, &ThemeColours::default(), &cx());
    for kept in ["<w:pPrChange", "<w:rPrChange", r#"<w:sz w:val="30"/>"#] {
        assert!(xml.contains(kept), "{kept} in {xml}");
    }
    let (frozen, _) = both(&part(x), &ThemeColours::default());
    assert_eq!(frozen.by_id["P"].base.ppr.mark_size, Some(15.0));
    assert!(frozen.by_id["P"].base.sets_text);
}

#[test]
fn an_empty_property_element_is_written_as_read() {
    let x = concat!(
        r#"<w:style w:type="table" w:styleId="T"><w:name w:val="T"/><w:rPr/><w:tblStylePr w:type="band1Horz"><w:tblPr/>"#,
        r#"<w:tcPr><w:shd w:val="clear" w:fill="EEEEEE"/></w:tcPr></w:tblStylePr></w:style>"#
    );
    let s = style(x);
    assert_eq!(sx::serialize(&s, &ThemeColours::default(), &cx()), x);
    both(&part(x), &ThemeColours::default());
}

#[test]
fn theme_colours_stay_references() {
    let x =
        r#"<w:style w:type="character" w:styleId="C"><w:name w:val="C"/><w:rPr><w:color w:val="FF0000" w:themeColor="accent1"/></w:rPr></w:style>"#;
    let s = style(x);
    assert!(matches!(s.format.char.color, Some(Color::Theme(_))));
    both(&part(x), &ThemeColours::default());
}

#[test]
fn defaults_and_latent_styles_read_as_the_reader_reads_them() {
    let x = part(concat!(
        r#"<w:docDefaults><w:rPrDefault/><w:pPrDefault w:x="1"><w:pPr><w:spacing w:after="0"/></w:pPr><x:other/></w:pPrDefault></w:docDefaults>"#,
        r#"<w:latentStyles w:defLockedState="false" w:defUIPriority="x" w:count="3"><w:lsdException w:name="Normal" w:uiPriority="0" w:qFormat="1"/>"#,
        r#"<w:lsdException w:uiPriority="9"/><w:lsdException w:name="heading 1" w:semiHidden="true"/></w:latentStyles>"#
    ));
    let (frozen, typed) = both(&x, &ThemeColours::default());
    assert_eq!(frozen.doc_rpr, Default::default(), "a run default without properties sets none");
    let l = typed.sheet.latent.as_deref().expect("latent styles");
    assert_eq!((l.locked, l.priority, l.count, l.exceptions.len()), (Some(false), None, Some(3), 2));
    let doc = roxmltree::Document::parse(&x).expect("parses");
    let mut n = Counts::default();
    for el in doc.root_element().children().filter(|c| c.is_element()) {
        round_trip(el, &cx(), &ThemeColours::default(), "inline", &mut n);
    }
    assert_eq!((n.defaults, n.latent), (1, 1));
}

#[test]
fn no_part_reads_as_words_defaults() {
    let colours = ThemeColours::default();
    sheet_matches(&Styles::parse(None, colours), &StylesPart::parse(None), &colours, "none");
    let broken = "<w:styles";
    sheet_matches(&Styles::parse(Some(broken), colours), &StylesPart::parse(Some(broken)), &colours, "broken");
}

// Holders

#[test]
fn an_edit_writes_over_the_style_as_read() {
    let x = concat!(
        r#"<w:style w:type="paragraph" w:styleId="H"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rsid w:val="00AB"/>"#,
        r#"<w:pPr><w:keepNext/><w:spacing w:before="240" w:after="0"/><x:mine/></w:pPr><w:rPr><w:b/></w:rPr></w:style>"#
    );
    let held = StyleDef::from_xml(x, &cx());
    let colours = ThemeColours::default();
    assert_eq!(held.xml(&colours, &cx()), x);
    let mut s = (**held.get()).clone();
    s.format.para.space_after = Some(6.0);
    let edited = held.edited(s, &[StyleSet::Para(None, ParaSet { prop: ParaProp::SpaceAfter, whole: false })]);
    assert_eq!(edited.xml(&colours, &cx()), x.replace(r#"w:after="0""#, r#"w:after="120""#), "the rest as written");
    let mut s = (**held.get()).clone();
    s.based_on = None;
    s.meta.semi_hidden = Some(true);
    let edited = held.edited(s, &[StyleSet::Style(StyleProp::BasedOn), StyleSet::Style(StyleProp::SemiHidden)]);
    assert_eq!(edited.xml(&colours, &cx()), x.replace(r#"<w:basedOn w:val="Normal"/>"#, "").replace("<w:rsid", "<w:semiHidden/><w:rsid"));
    assert!(!edited.authored_eq(&held));
}

#[test]
fn an_edit_of_a_region_writes_over_it_alone() {
    let x = concat!(
        r#"<w:style w:type="table" w:styleId="T"><w:name w:val="T"/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/></w:tblPr>"#,
        r#"<w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr></w:tblStylePr><w:tblStylePr w:type="lastRow"><w:rPr><w:i/></w:rPr>"#,
        r#"<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/></w:tcPr></w:tblStylePr></w:style>"#
    );
    let held = StyleDef::from_xml(x, &cx());
    let colours = ThemeColours::default();
    let mut s = (**held.get()).clone();
    let last = s.regions.iter_mut().find(|r| r.kind == RegionKind::LastRow).expect("a last row");
    last.format.char.toggles.put(crate::format::Toggle::BOLD, true);
    let set = crate::format::Set { prop: crate::format::Prop::Toggle(crate::format::Toggle::BOLD), whole: true };
    let edited = held.edited(s.clone(), &[StyleSet::Char(Some(RegionKind::LastRow), set)]);
    assert_eq!(edited.xml(&colours, &cx()), x.replace("<w:rPr><w:i/></w:rPr>", "<w:rPr><w:b/><w:i/></w:rPr>"));
    s.regions.retain(|r| r.kind != RegionKind::FirstRow);
    let edited = held.edited(s, &[StyleSet::Region(RegionKind::FirstRow)]);
    let xml = edited.xml(&colours, &cx());
    assert!(!xml.contains("firstRow") && xml.contains(r#"<w:tblInd w:w="0" w:type="dxa"/>"#), "{xml}");
}

#[test]
fn edited_defaults_and_latent_styles_write_over_them_as_read() {
    let colours = ThemeColours::default();
    let d = concat!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>"#,
        r#"<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>"#
    );
    let held = DefaultsDef::from_xml(d, &cx());
    let mut v = (**held.get()).clone();
    v.char.as_mut().expect("run defaults").size = Some(12.0);
    let set = crate::format::Set { prop: crate::format::Prop::Size, whole: true };
    let edited = held.edited(v, &[crate::format::DefaultsSet::Char(set)]);
    assert_eq!(edited.xml(&colours, &cx()), d.replace(r#"w:val="22""#, r#"w:val="24""#));
    let l = concat!(
        r#"<w:latentStyles w:defLockedState="0" w:defUIPriority="99" w:count="2"><w:lsdException w:name="Normal" w:uiPriority="0" w:qFormat="1"/>"#,
        r#"<w:lsdException w:name="heading 1" w:uiPriority="9"/></w:latentStyles>"#
    );
    let held = LatentDef::from_xml(l, &cx());
    let mut v = (**held.get()).clone();
    v.exceptions[1].quick = Some(true);
    v.count = Some(3);
    let edited = held.edited(v, &[]);
    let want = l.replace(r#"w:count="2""#, r#"w:count="3""#).replace(r#"w:uiPriority="9"/>"#, r#"w:uiPriority="9" w:qFormat="1"/>"#);
    assert_eq!(edited.xml(&colours, &cx()), want);
}

#[test]
fn a_style_moved_to_a_part_that_reads_it_otherwise_is_written_from_its_value() {
    let x = r#"<w:style w:type="character" w:styleId="C"><w:name w:val="C"/><w:rPr><x:y/></w:rPr></w:style>"#;
    let held = StyleDef::from_xml(x, &cx());
    assert!(held.moved_to(&cx()).is_none());
    let other = Cx::of_xml(&format!("<w:styles xmlns:w=\"{W}\" xmlns:x=\"urn:other\">"));
    assert!(held.moved_to(&other).is_some());
}
