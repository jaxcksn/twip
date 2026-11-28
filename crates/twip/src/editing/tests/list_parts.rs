//! Every list definition, list and picture bullet in every fixture's numbering parts: as read, written again, and as
//! the XML path reads it; each part whole, and its level lookups, against the XML path's

use std::io::Cursor;

use roxmltree::Node;

use crate::document::paragraph::ListId;
use crate::docx::definitions::ListHolders;
use crate::docx::list_xml::ListXml;
use crate::docx::list_xml::tests::{abstract_diff, level_diff, num_diff};
use crate::docx::list_xml::{
    self, Element, ListDefDirect, ListDirect, NumberingXml, PictureDirect, to_abstract_num, to_level, to_num, to_pic_bullet,
};
use crate::docx::numbering::{Numbering, parse_abstract, parse_num};
use crate::docx::props::ThemeColours;
use crate::docx::styles::Styles;
use crate::docx::write::props::Cx;
use crate::docx::xml::{children, is_w};
use crate::docx::{char_props, para_props};
use crate::format::{CharProps, Color, ListLevel, ParaProps};

/// A fixture's numbering part, with the theme colours of its document and the styles part beside it.
struct Part {
    what: String,
    xml: String,
    colours: ThemeColours,
    styles: Styles,
}

fn parts() -> Vec<Part> {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    let mut out = Vec::new();
    for path in paths {
        let bytes = std::fs::read(&path).expect("reads");
        let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(&bytes[..])) else { continue };
        let names: Vec<String> = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().and_then(|f| f.name().ok().map(|n| n.to_string())))
            .filter(|n| n.ends_with("numbering.xml"))
            .collect();
        if names.is_empty() {
            continue;
        }
        let colours = crate::docx::read_editable(&bytes).ok().and_then(|(d, _)| d.definitions.as_deref().map(|r| r.colours())).unwrap_or_default();
        let mut text = |name: &str| -> Option<String> { crate::docx::package::text(zip.by_name(name).ok()?, name).ok() };
        for name in names {
            let Some(xml) = text(&name) else { continue };
            let styles = text(&name.replace("numbering.xml", "styles.xml")).map_or_else(Styles::default, |s| Styles::parse(Some(&s), colours));
            out.push(Part { what: format!("{}: {name}", path.display()), xml, colours, styles });
        }
    }
    out
}

/// What the XML path's level properties, converted to typed ones, can hold of a level's.
fn comparable(l: &ListLevel) -> (ParaProps, CharProps) {
    let auto = |c: &mut Option<Color>| *c = c.filter(|c| *c != Color::Auto);
    let mut p = l.para.clone();
    p.foreign = None;
    if let Some(s) = &mut p.shading {
        auto(&mut s.color);
        auto(&mut s.fill);
    }
    let b = &mut p.borders;
    for side in [&mut b.top, &mut b.bottom, &mut b.left, &mut b.right, &mut b.between].into_iter().flatten() {
        auto(&mut side.color);
    }
    let mut c = l.char.clone();
    (c.foreign, c.lang.complex, c.spec_vanish) = (None, None, None);
    if let Some(u) = &mut c.underline {
        auto(&mut u.color);
    }
    if let Some(s) = &mut c.shading {
        auto(&mut s.color);
        auto(&mut s.fill);
    }
    if let Some(b) = &mut c.border {
        auto(&mut b.color);
    }
    (p, c)
}

/// `typed` against `path`, the XML path's level: the conversion each way.
fn level_agrees(typed: &ListLevel, path: &crate::docx::numbering::Level, colours: &ThemeColours, cx: &Cx, what: &str) {
    assert_eq!(level_diff(&to_level(typed, colours, cx), path), Vec::<&str>::new(), "{what}: converted to the XML path's");
    let (p, c) = comparable(typed);
    assert_eq!(para_props::from_para_props(&path.ppr), p, "{what}: paragraph properties converted from the XML path");
    assert_eq!(char_props::from_run_props(&path.rpr), c, "{what}: run properties converted from the XML path");
}

/// `el`'s holder writes it as read, and written over itself with nothing set it is as read; written again by the typed
/// serializer it reads the same and loses nothing. The typed value and the serializer's writing, read on the XML path.
fn round_trip<T: Element>(
    el: Node,
    cx: &Cx,
    colours: &ThemeColours,
    what: &str,
    serialize: fn(&T, &ThemeColours, &Cx) -> String,
) -> Option<(T, String)> {
    let raw = &el.document().input_text()[el.range()];
    let typed = T::parse(el)?;
    let held = crate::docx::list_xml::Direct::read(typed.clone(), raw.into());
    assert_eq!(held.xml(colours, cx), raw, "{what}: as read");
    assert_eq!(typed.written_over(colours, cx, (&typed, raw), &[]), raw, "{what}: written over itself");
    let again = serialize(&typed, colours, cx);
    assert_eq!(cx.parse(&again, |_, e| T::parse(e)).flatten().as_ref(), Some(&typed), "{what}: typed, written as {again}");
    let lost = cx.parse(&again, |_, e| char_props::tests::lost(el, e)).unwrap_or_default();
    assert!(lost.is_empty(), "{what}: written as {again}, loses {lost:?}");
    Some((typed, again))
}

#[derive(Debug, Default)]
struct Counts {
    parts: usize,
    defs: usize,
    levels: usize,
    lists: usize,
    overrides: usize,
    pictures: usize,
    lookups: usize,
}

#[test]
fn every_list_element_round_trips_and_reads_as_the_xml_path_reads_it() {
    let mut n = Counts::default();
    for part in parts() {
        let Ok(doc) = roxmltree::Document::parse(&part.xml) else { continue };
        n.parts += 1;
        let cx = ListHolders::parse(Some(&part.xml)).cx();
        let colours = &part.colours;
        let root = doc.root_element();
        for a in children(root, "abstractNum") {
            let what = format!("{}: {}", part.what, &part.xml[a.range()]);
            let path = parse_abstract(a, &part.xml, colours).map(|f| f.1);
            let Some((typed, again)) = round_trip(a, &cx, colours, &what, list_xml::serialize_def) else {
                assert!(path.is_none(), "{what}: read on the XML path");
                continue;
            };
            let path = path.expect("read on the XML path");
            let held = ListDefDirect::read(typed.clone(), part.xml[a.range()].into());
            assert_eq!(abstract_diff(&to_abstract_num(&held, colours, &cx), &path, true), Vec::<String>::new(), "{what}");
            let written = cx.parse(&again, |text, e| parse_abstract(e, text, colours)).flatten().expect("written, read on the XML path").1;
            assert_eq!(abstract_diff(&written, &path, false), Vec::<String>::new(), "{what}: on the XML path, written as {again}");
            for (k, l) in typed.levels.iter().enumerate() {
                if let (Some(l), Some(f)) = (l, &path.levels[k]) {
                    level_agrees(l, f, colours, &cx, &format!("{what}: level {k}"));
                    n.levels += 1;
                }
            }
            n.defs += 1;
        }
        for m in children(root, "num") {
            let what = format!("{}: {}", part.what, &part.xml[m.range()]);
            let path = parse_num(m, &part.xml, colours).map(|f| f.1);
            let Some((typed, again)) = round_trip(m, &cx, colours, &what, list_xml::serialize_list) else {
                assert!(path.is_none(), "{what}: read on the XML path");
                continue;
            };
            let path = path.expect("read on the XML path");
            let held = ListDirect::read(typed.clone(), part.xml[m.range()].into());
            assert_eq!(num_diff(&to_num(&held, colours, &cx), &path, true), Vec::<String>::new(), "{what}");
            let written = cx.parse(&again, |text, e| parse_num(e, text, colours)).flatten().expect("written, read on the XML path").1;
            assert_eq!(num_diff(&written, &path, false), Vec::<String>::new(), "{what}: on the XML path, written as {again}");
            for o in &typed.overrides {
                if let (Some(l), Some(f)) = (&o.def, path.level_overrides.get(&o.level)) {
                    level_agrees(l, f, colours, &cx, &format!("{what}: override {}", o.level));
                }
            }
            n.overrides += typed.overrides.len();
            n.lists += 1;
        }
        for p in root.children().filter(|c| is_w(*c, "numPicBullet")) {
            let what = format!("{}: {}", part.what, &part.xml[p.range()]);
            let Some((typed, _)) = round_trip(p, &cx, colours, &what, list_xml::serialize_picture) else { continue };
            let path = Numbering::parse(Some(&part.xml), colours);
            let held = PictureDirect::read(typed.clone(), part.xml[p.range()].into());
            assert_eq!(Some(&to_pic_bullet(&held, colours, &cx)), path.pic_bullets.get(&typed.id), "{what}");
            n.pictures += 1;
        }
    }
    assert!(n.parts >= 17 && n.defs >= 173 && n.levels >= 1_500 && n.lists >= 182 && n.overrides >= 20 && n.pictures >= 1, "only {n:?} checked");
}

/// Each fixture's numbering part, typed, converts to the XML path's numbering of it, and every list's level lookups
/// (with its numbering style links resolved through the styles part beside it) find the levels that path finds.
#[test]
fn every_numbering_part_reads_as_the_xml_path_reads_it() {
    let mut n = Counts::default();
    for part in parts() {
        let colours = &part.colours;
        let typed = ListHolders::parse(Some(&part.xml));
        let path = Numbering::parse(Some(&part.xml), colours);
        let cx = typed.cx();
        let converted = typed.to_numbering(colours);
        let what = &part.what;
        assert_eq!(converted.root, path.root, "{what}: root");
        let mut ids: Vec<&i64> = path.abstracts.keys().collect();
        ids.sort();
        let mut typed_ids: Vec<&i64> = converted.abstracts.keys().collect();
        typed_ids.sort();
        assert_eq!(typed_ids, ids, "{what}: definitions");
        for id in ids {
            assert_eq!(abstract_diff(&converted.abstracts[id], &path.abstracts[id], true), Vec::<String>::new(), "{what}: definition {id}");
        }
        let mut ids: Vec<&i64> = path.nums.keys().collect();
        ids.sort();
        let mut typed_ids: Vec<&i64> = converted.nums.keys().collect();
        typed_ids.sort();
        assert_eq!(typed_ids, ids, "{what}: lists");
        for id in &ids {
            assert_eq!(num_diff(&converted.nums[id], &path.nums[id], true), Vec::<String>::new(), "{what}: list {id}");
        }
        assert_eq!(converted.pic_bullets, path.pic_bullets, "{what}: picture bullets");
        let lists = typed.lists();
        let linked = |s: &str| part.styles.chain_ppr(Some(s)).num_id.map(ListId);
        let probe: Vec<i64> = ids.iter().map(|i| **i).chain([0, -1, i64::MAX]).collect();
        for id in probe {
            let what = format!("{what}: list {id}");
            assert_eq!(lists.def_of(ListId(id), linked).map(|d| d.0), path.abstract_of(id, &part.styles), "{what}: definition");
            for il in -1..=9 {
                let (t, f) = (lists.level(ListId(id), il, linked), path.level(id, il, &part.styles));
                assert_eq!(t.is_some(), f.is_some(), "{what}: level {il}");
                if let (Some(t), Some(f)) = (t, f) {
                    assert_eq!(level_diff(&to_level(t, colours, &cx), f), Vec::<&str>::new(), "{what}: level {il}");
                }
                n.lookups += 1;
            }
            let (t, f) = (lists.levels(ListId(id), linked), path.levels(id, &part.styles));
            assert_eq!(t.is_some(), f.is_some(), "{what}: levels");
            for (t, f) in t.into_iter().flatten().zip(f.into_iter().flatten()) {
                assert_eq!(t.is_some(), f.is_some(), "{what}: levels");
                if let (Some(t), Some(f)) = (t, f) {
                    assert_eq!(level_diff(&to_level(t, colours, &cx), f), Vec::<&str>::new(), "{what}: levels");
                }
            }
        }
        n.parts += 1;
    }
    assert!(n.parts >= 17 && n.lookups >= 2_000, "only {n:?} checked");
}
