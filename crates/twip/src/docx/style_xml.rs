//! Typed style definitions from and to `w:style`, `w:docDefaults`, `w:latentStyles` and a whole styles part
//!
//! A style's and a region's property elements (`w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr`, `w:tcPr`) are read and written
//! by their own adapters ([`super::para_props`], [`super::char_props`], [`super::table_props`]); the children those
//! leave to other nodes (a tracked change, a mark's properties, a revision) join that element's unmodelled children
//! here, in source order. As there, the parse keeps in [`ForeignProps`] what the typed value does not hold, so that
//! writing the value again loses nothing of what was read:
//!
//! - unmodelled children whole (`w:aliases`, `w:personal`, `w:rsid`, ...), and the other duplicates of a modelled
//!   child: the reader reads a style's first child of each name, and the last region of each kind;
//! - a region without a type, or of a type that is none of the thirteen, whole;
//! - a modelled child its value doesn't keep (a name, base, next or link without a value, a priority that isn't a
//!   number) whole, and a property element that sets nothing, as written;
//! - unmodelled attributes, and modelled ones written otherwise than the writer writes their value (`w:val="true"`,
//!   an unknown style type, read as a paragraph style), written as read while the value is still the one read;
//! - an exception of the latent styles without a name whole.
//!
//! The serializer writes CT_Style's children in schema order, an unmodelled one at its schema place when CT_Style
//! names it (after the modelled one of its name; before the typed regions for a region) and after all of them
//! otherwise. A holder writes the element as read while its typed value is the one read, else written over it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use roxmltree::Node;

use super::as_read;
use super::char_props::{self, RprRevisions};
use super::para_props::{self, Owned};
use super::prop_xml::{Compare, Out, Sink, foreign_child, props_key, reread, strip, with_attr};
use super::props::ThemeColours;
#[cfg(test)]
use super::props::{self as xml_props};
#[cfg(test)]
use super::styles;
use super::table_props::{self as tp, CellDirect, RowDirect, TableDirect};
use super::write::props::{Cx, STYLE};
use super::xml::{W, attr, child, children, is_w, on_off, on_off_str, val};
use crate::document::authored::Authored;
use crate::docx::table_props::TableXml;
use crate::format::{
    CellSet, CharProps, Defaults, DefaultsSet, ForeignChild, ForeignProps, LatentStyle, LatentStyles, Origin, ParaProps, ParaSet, QName, Region,
    RegionKind, RowSet, Set, Style, StyleEntry, StyleFormat, StyleKind, StyleProp, StyleSet, StyleSheet, TableSet,
};

// Names

const KINDS: [(&str, StyleKind); 4] =
    [("paragraph", StyleKind::Paragraph), ("character", StyleKind::Character), ("table", StyleKind::Table), ("numbering", StyleKind::Numbering)];

const REGIONS: [(&str, RegionKind); 13] = [
    ("wholeTable", RegionKind::WholeTable),
    ("firstRow", RegionKind::FirstRow),
    ("lastRow", RegionKind::LastRow),
    ("firstCol", RegionKind::FirstCol),
    ("lastCol", RegionKind::LastCol),
    ("band1Vert", RegionKind::Band1Vert),
    ("band2Vert", RegionKind::Band2Vert),
    ("band1Horz", RegionKind::Band1Horz),
    ("band2Horz", RegionKind::Band2Horz),
    ("neCell", RegionKind::NeCell),
    ("nwCell", RegionKind::NwCell),
    ("seCell", RegionKind::SeCell),
    ("swCell", RegionKind::SwCell),
];

/// The on/off children and their properties.
const FLAGS: [(&str, StyleProp); 6] = [
    ("autoRedefine", StyleProp::AutoRedefine),
    ("hidden", StyleProp::Hidden),
    ("semiHidden", StyleProp::SemiHidden),
    ("unhideWhenUsed", StyleProp::UnhideWhenUsed),
    ("qFormat", StyleProp::Quick),
    ("locked", StyleProp::Locked),
];

/// The property elements of a style and of a region, in schema order.
const PROPS: [&str; 5] = ["pPr", "rPr", "tblPr", "trPr", "tcPr"];

/// The children of property element `key` its adapter leaves to other nodes.
fn owned(key: &str) -> &'static [&'static str] {
    match key {
        "pPr" => &["rPr", "sectPr", "pPrChange"],
        "rPr" => &["ins", "del", "moveFrom", "moveTo", "rPrChange"],
        "tblPr" => &["tblPrChange"],
        "trPr" => &["ins", "del", "trPrChange"],
        _ => &["cellIns", "cellDel", "cellMerge", "tcPrChange"],
    }
}

/// A `w:type` literal as a style kind: anything else, or none, is a paragraph style.
fn kind_of(v: Option<&str>) -> StyleKind {
    v.and_then(|v| KINDS.iter().find(|(n, _)| *n == v)).map_or(StyleKind::Paragraph, |(_, k)| *k)
}

fn kind_literal(k: StyleKind) -> &'static str {
    KINDS.iter().find(|(_, x)| *x == k).map_or("paragraph", |(n, _)| n)
}

/// A `w:tblStylePr` type literal as a region.
pub fn region_kind(v: Option<&str>) -> Option<RegionKind> {
    v.and_then(|v| REGIONS.iter().find(|(n, _)| *n == v)).map(|(_, k)| *k)
}

/// The regions `names` (`w:tblStylePr` type literals) name, those of other names left out.
pub fn region_kinds<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<RegionKind> {
    names.into_iter().filter_map(|n| region_kind(Some(n))).collect()
}

/// A region's `w:type` literal.
pub fn region_literal(k: RegionKind) -> &'static str {
    REGIONS.iter().find(|(_, x)| *x == k).map_or("wholeTable", |(n, _)| n)
}

fn rank_of(ns: &str, local: &str) -> Option<usize> {
    (ns == W).then(|| STYLE.iter().position(|k| *k == local)).flatten()
}

fn on01(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

fn flag(s: &Style, p: StyleProp) -> Option<bool> {
    match p {
        StyleProp::Quick => s.quick,
        StyleProp::SemiHidden => s.meta.semi_hidden,
        StyleProp::Hidden => s.meta.hidden,
        StyleProp::UnhideWhenUsed => s.meta.unhide_when_used,
        StyleProp::Locked => s.meta.locked,
        _ => s.meta.auto_redefine,
    }
}

fn flag_mut(s: &mut Style, p: StyleProp) -> &mut Option<bool> {
    match p {
        StyleProp::Quick => &mut s.quick,
        StyleProp::SemiHidden => &mut s.meta.semi_hidden,
        StyleProp::Hidden => &mut s.meta.hidden,
        StyleProp::UnhideWhenUsed => &mut s.meta.unhide_when_used,
        StyleProp::Locked => &mut s.meta.locked,
        _ => &mut s.meta.auto_redefine,
    }
}

/// The child of CT_Style a style property is written in; `None` for an attribute of the style itself.
fn key_of(p: StyleProp) -> Option<&'static str> {
    Some(match p {
        StyleProp::Id | StyleProp::Kind | StyleProp::Default | StyleProp::Custom => return None,
        StyleProp::Name => "name",
        StyleProp::BasedOn => "basedOn",
        StyleProp::Next => "next",
        StyleProp::Link => "link",
        StyleProp::Priority => "uiPriority",
        p => FLAGS.iter().find(|(_, x)| *x == p).map_or("qFormat", |(n, _)| n),
    })
}

/// Whether `s` sets property element `key` of the base (`region` `None`) or of a region.
fn sets_props(s: &StyleSet, region: Option<RegionKind>, key: &str) -> bool {
    match *s {
        StyleSet::Para(r, _) => r == region && key == "pPr",
        StyleSet::Char(r, _) => r == region && key == "rPr",
        StyleSet::Table(r, _) => r == region && key == "tblPr",
        StyleSet::Row(r, _) => r == region && key == "trPr",
        StyleSet::Cell(r, _) => r == region && key == "tcPr",
        StyleSet::Style(_) | StyleSet::Region(_) => false,
    }
}

fn is_default(f: &StyleFormat, key: &str) -> bool {
    match key {
        "pPr" => f.para == ParaProps::default(),
        "rPr" => f.char == CharProps::default(),
        "tblPr" => f.table == Default::default(),
        "trPr" => f.row == Default::default(),
        _ => f.cell == Default::default(),
    }
}

fn foreign_of(f: ForeignProps) -> Option<Rc<ForeignProps>> {
    (f != ForeignProps::default()).then(|| Rc::new(f))
}

// Parse

/// Whether `n` is its parent's only child (but for whitespace): an element alone.
pub fn alone(n: Node) -> bool {
    n.parent().is_some_and(|x| x.children().all(|c| c == n || c.is_text() && c.text().is_some_and(|t| t.trim().is_empty())))
}

/// A `w:style` as a typed style; `None` without a `w:styleId`, which the reader skips.
pub fn parse(el: Node) -> Option<Style> {
    attr(el, "styleId")?;
    Some(read(el))
}

/// `el`'s style: its own attributes and its children.
fn read(el: Node) -> Style {
    let on = |v: &str| on_off_str(Some(v));
    let mut s = Style {
        id: attr(el, "styleId").unwrap_or_default().into(),
        kind: kind_of(attr(el, "type")),
        default: attr(el, "default").map(on),
        custom: attr(el, "customStyle").map(on),
        ..Style::default()
    };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(el, &mut scratch);
    emit_own(&s, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let elements = || el.children().filter(|c| c.is_element());
    // The children read: the first of each name, the last region of each kind
    let mut pick = [None::<usize>; STYLE.len()];
    let mut last = [None::<usize>; 13];
    for (i, e) in elements().enumerate() {
        match rank_of(e.tag_name().namespace().unwrap_or_default(), e.tag_name().name()) {
            Some(k) if STYLE[k] == "tblStylePr" => {
                if let Some(r) = region_kind(attr(e, "type")) {
                    last[r as usize] = Some(i);
                }
            }
            Some(k) if pick[k].is_none() => pick[k] = Some(i),
            _ => {}
        }
    }
    for (i, e) in elements().enumerate() {
        let (ns, name) = (e.tag_name().namespace().unwrap_or_default(), e.tag_name().name());
        let Some(key) = rank_of(ns, name).filter(|&k| STYLE[k] == "tblStylePr" || pick[k] == Some(i)).map(|k| STYLE[k]) else {
            f.children.push(foreign_child(e));
            continue;
        };
        if key == "tblStylePr" {
            match region_kind(attr(e, "type")).filter(|r| last[*r as usize] == Some(i)) {
                Some(kind) => s.regions.push(read_region(e, kind)),
                None => f.children.push(foreign_child(e)),
            }
            continue;
        }
        if PROPS.contains(&key) {
            read_props(&mut s.format, e, key, &mut f);
            continue;
        }
        if !modelled(&mut s, e, key) {
            f.children.push(foreign_child(e));
            continue;
        }
        let mut cmp = Compare::new(e, &mut scratch);
        emit(&s, key, &mut cmp);
        cmp.finish(Some(QName { ns: W.into(), local: name.into() }), &mut f.attributes);
        if e.children().any(|c| c.is_element() || c.text().is_some_and(|t| !t.trim().is_empty())) {
            f.kept.push(foreign_child(e));
        }
    }
    s.foreign = foreign_of(f);
    s
}

/// A child of CT_Style other than a property element or a region into `s`, as the reader reads it; false when it
/// isn't modelled or holds no value.
fn modelled(s: &mut Style, e: Node, key: &str) -> bool {
    if let Some(&(_, p)) = FLAGS.iter().find(|(n, _)| *n == key) {
        *flag_mut(s, p) = Some(on_off(e));
        return true;
    }
    let Some(v) = val(e) else { return false };
    match key {
        "name" => {
            // Name, then aliases
            let mut names = v.split(',').map(str::trim);
            s.name = names.next().map(Rc::from);
            s.aliases = names.filter(|a| !a.is_empty()).map(Rc::from).collect();
        }
        "basedOn" => s.based_on = Some(v.into()),
        "next" => s.next = Some(v.into()),
        "link" => s.link = Some(v.into()),
        "uiPriority" => match v.parse() {
            Ok(p) => s.priority = Some(p),
            Err(_) => return false,
        },
        _ => return false,
    }
    true
}

/// Property element `e` (`key`) into `f`'s level, the children its adapter leaves to other nodes among its
/// unmodelled ones in source order; kept as written in `kept` when it sets nothing.
fn read_props(f: &mut StyleFormat, e: Node, key: &str, kept: &mut ForeignProps) {
    match key {
        "pPr" => {
            f.para = para_props::parse(e);
            with_owned(&mut f.para.foreign, e, key);
        }
        "rPr" => {
            f.char = char_props::parse(e).0;
            with_owned(&mut f.char.foreign, e, key);
        }
        "tblPr" => {
            f.table = TableDirect::parse(e);
            with_owned(&mut f.table.foreign, e, key);
        }
        "trPr" => {
            f.row = RowDirect::parse(e);
            with_owned(&mut f.row.foreign, e, key);
        }
        _ => {
            f.cell = CellDirect::parse(e);
            with_owned(&mut f.cell.foreign, e, key);
        }
    }
    if is_default(f, key) {
        kept.kept.push(foreign_child(e));
    }
}

/// `foreign`'s children with the first child of each name `key`'s adapter leaves to other nodes, all in source order.
fn with_owned(foreign: &mut Option<Rc<ForeignProps>>, e: Node, key: &str) {
    let names = owned(key);
    let mine = |c: &Node| c.is_element() && c.tag_name().namespace() == Some(W) && names.contains(&c.tag_name().name());
    if !e.children().any(|c| mine(&c)) {
        return;
    }
    let f = Rc::make_mut(foreign.get_or_insert_with(Default::default));
    let read = std::mem::take(&mut f.children);
    let mut read = read.into_iter().peekable();
    let text = e.document().input_text();
    let mut seen: Vec<&str> = Vec::new();
    for c in e.children().filter(|c| c.is_element()) {
        if mine(&c) && !seen.contains(&c.tag_name().name()) {
            seen.push(c.tag_name().name());
            f.children.push(foreign_child(c));
        } else if read.peek().is_some_and(|r| *r.payload == text[c.range()]) {
            f.children.extend(read.next());
        }
    }
    f.children.extend(read);
}

/// A `w:tblStylePr` of `kind` as a region.
fn read_region(e: Node, kind: RegionKind) -> Region {
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(e, &mut scratch);
    cmp.attr(W, "type", &region_literal(kind));
    cmp.finish(None, &mut f.attributes);
    let mut format = StyleFormat::default();
    region_children(&mut format, e, &mut f);
    Region { kind, format, foreign: foreign_of(f) }
}

/// A region's children into `format`: the first property element of each name; the others into `f`.
fn region_children(format: &mut StyleFormat, e: Node, f: &mut ForeignProps) {
    let mut seen = [false; PROPS.len()];
    for c in e.children().filter(|c| c.is_element()) {
        let k = PROPS.iter().position(|k| is_w(c, k)).filter(|k| !seen[*k]);
        match k {
            Some(k) => {
                seen[k] = true;
                read_props(format, c, PROPS[k], f);
            }
            None => f.children.push(foreign_child(c)),
        }
    }
}

// The attributes a modelled child is written with

/// The style's own attributes.
fn emit_own(s: &Style, sink: &mut dyn Sink) {
    sink.attr(W, "type", &kind_literal(s.kind));
    if let Some(d) = s.default {
        sink.attr(W, "default", &on01(d));
    }
    if let Some(c) = s.custom {
        sink.attr(W, "customStyle", &on01(c));
    }
    sink.attr(W, "styleId", &&*s.id);
}

/// Writes the attributes of child `key` of `s` (not a property element or a region) to `sink`; false when `s` doesn't
/// set it.
fn emit(s: &Style, key: &str, sink: &mut dyn Sink) -> bool {
    if let Some(&(_, p)) = FLAGS.iter().find(|(n, _)| *n == key) {
        return match flag(s, p) {
            Some(true) => {
                sink.implied("val", "1");
                true
            }
            Some(false) => {
                sink.attr(W, "val", &"0");
                true
            }
            None => false,
        };
    }
    let text = |v: &Option<Rc<str>>, sink: &mut dyn Sink| v.as_deref().map(|v| sink.attr(W, "val", &v)).is_some();
    match key {
        "name" if s.name.is_some() || !s.aliases.is_empty() => {
            let mut names = s.name.as_deref().unwrap_or_default().to_string();
            for a in &s.aliases {
                names.push(',');
                names.push_str(a);
            }
            sink.attr(W, "val", &names);
            true
        }
        "basedOn" => text(&s.based_on, sink),
        "next" => text(&s.next, sink),
        "link" => text(&s.link, sink),
        "uiPriority" => s.priority.map(|p| sink.attr(W, "val", &p)).is_some(),
        _ => false,
    }
}

// Serialize

/// `s` as a `w:style` in the part `cx` describes; a theme colour's value is written as `colours` resolve it.
pub fn serialize(s: &Style, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let mut decls = String::new();
    out.declarations(s.foreign.as_deref(), &mut decls);
    for r in &s.regions {
        out.declarations(r.foreign.as_deref(), &mut decls);
    }
    let w = cx.w.clone();
    let attrs = s.foreign.as_deref().map_or(&[][..], |f| &f.attributes);
    let _ = write!(out.s, "<{w}style{decls}");
    let written = out.attrs(W, attrs, None, |sink| emit_own(s, sink));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    let foreign = s.foreign.as_deref();
    let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(|c| rank_of(&c.name.ns, &c.name.local)).collect());
    for (i, &key) in STYLE.iter().enumerate() {
        if key == "tblStylePr" {
            foreign_children(&mut out, foreign, &ranks, Some(i));
            for r in &s.regions {
                write_region(&mut out, r);
            }
        } else {
            modelled_child(&mut out, s, key);
            foreign_children(&mut out, foreign, &ranks, Some(i));
        }
    }
    foreign_children(&mut out, foreign, &ranks, None);
    let _ = write!(out.s, "</{w}style>");
    out.s
}

fn foreign_children(out: &mut Out, foreign: Option<&ForeignProps>, ranks: &[Option<usize>], at: Option<usize>) {
    let Some(f) = foreign else { return };
    for (c, _) in f.children.iter().zip(ranks).filter(|(_, r)| **r == at) {
        out.payload(c);
    }
}

/// The kept child of `f` named `key`.
fn kept<'f>(f: Option<&'f ForeignProps>, key: &str) -> Option<&'f ForeignChild> {
    f?.kept.iter().find(|c| &*c.name.ns == W && &*c.name.local == key)
}

/// Child `key` of `s`: as written when it was kept so and its value is still the one read from it.
fn modelled_child(out: &mut Out, s: &Style, key: &'static str) {
    let Some(c) = kept(s.foreign.as_deref(), key) else {
        typed_child(out, s, key);
        return;
    };
    let at = out.s.len();
    typed_child(out, s, key);
    let now = out.s.split_off(at);
    let same = reread(c, read).is_some_and(|q| {
        let at = out.s.len();
        typed_child(out, &q, key);
        out.s.split_off(at) == now
    });
    if same {
        out.payload(c);
    } else {
        out.s.push_str(&now);
    }
}

/// Child `key` of `s` from its typed value.
fn typed_child(out: &mut Out, s: &Style, key: &'static str) {
    if PROPS.contains(&key) {
        props_child(out, &s.format, key);
        return;
    }
    let attrs = s.foreign.as_deref().map_or(&[][..], |f| &f.attributes);
    let at = out.s.len();
    let _ = write!(out.s, "<{}{key}", out.cx.w);
    let mut present = false;
    let written = out.attrs(W, attrs, Some((W, key)), |sink| present = emit(s, key, sink));
    if !present {
        out.s.truncate(at);
        return;
    }
    out.free_attrs(attrs, Some((W, key)), &written);
    out.s.push_str("/>");
}

/// Property element `key` of a level from its typed value; nothing when it sets nothing.
fn props_child(out: &mut Out, f: &StyleFormat, key: &str) {
    if is_default(f, key) {
        return;
    }
    let (colours, cx) = (out.colours, out.cx);
    let x = match key {
        "pPr" => para_props::serialize(&f.para, Owned::default(), colours, cx),
        "rPr" => char_props::serialize(&f.char, &RprRevisions::default(), colours, cx),
        "tblPr" => TableDirect::serialize(&f.table, &[], colours, cx),
        "trPr" => RowDirect::serialize(&f.row, &[], colours, cx),
        _ => CellDirect::serialize(&f.cell, &[], colours, cx),
    };
    out.s.push_str(&x);
}

/// Region `r` as a `w:tblStylePr`.
fn write_region(out: &mut Out, r: &Region) {
    let w = out.cx.w.clone();
    let foreign = r.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let _ = write!(out.s, "<{w}tblStylePr");
    let written = out.attrs(W, attrs, None, |sink| sink.attr(W, "type", &region_literal(r.kind)));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    let rank = |c: &ForeignChild| (&*c.name.ns == W).then(|| PROPS.iter().position(|k| *k == &*c.name.local)).flatten();
    let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(rank).collect());
    for (i, &key) in PROPS.iter().enumerate() {
        region_child(out, r, key);
        foreign_children(out, foreign, &ranks, Some(i));
    }
    foreign_children(out, foreign, &ranks, None);
    let _ = write!(out.s, "</{w}tblStylePr>");
}

/// Property element `key` of region `r`: as written when it was kept so and its value is still the one read from it.
fn region_child(out: &mut Out, r: &Region, key: &str) {
    let Some(c) = kept(r.foreign.as_deref(), key) else {
        props_child(out, &r.format, key);
        return;
    };
    let at = out.s.len();
    props_child(out, &r.format, key);
    let now = out.s.split_off(at);
    let again = reread(c, |el| {
        let mut f = StyleFormat::default();
        region_children(&mut f, el, &mut ForeignProps::default());
        f
    });
    let same = again.is_some_and(|f| {
        let at = out.s.len();
        props_child(out, &f, key);
        out.s.split_off(at) == now
    });
    if same {
        out.payload(c);
    } else {
        out.s.push_str(&now);
    }
}

// Writing over an element as read

/// An element as written: its start tag, its end tag, and its element children as written, each with its key among
/// its parent's ([`props_key`]) and, for a region, its kind.
struct Written {
    open: String,
    close: String,
    kids: Vec<Kid>,
    /// Whether anything of it was changed.
    changed: bool,
}

struct Kid {
    key: String,
    text: String,
    region: Option<RegionKind>,
}

impl Written {
    fn parse(xml: &str, cx: &Cx) -> Option<Written> {
        cx.parse(xml, |text, el| {
            let start = el.range().start;
            let end = start + super::binding::tag_end(&text[start..])?;
            let mut open = text[start..=end].to_string();
            if let Some(head) = open.strip_suffix("/>") {
                open = format!("{}>", head.trim_end());
            }
            let name: String = open[1..].chars().take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/').collect();
            let kids = el
                .children()
                .filter(|c| c.is_element())
                .map(|c| Kid {
                    key: props_key(&QName { ns: c.tag_name().namespace().unwrap_or_default().into(), local: c.tag_name().name().into() }),
                    text: text[c.range()].to_string(),
                    region: is_w(c, "tblStylePr").then(|| region_kind(attr(c, "type"))).flatten(),
                })
                .collect();
            Some(Written { open, close: format!("</{name}>"), kids, changed: false })
        })
        .flatten()
    }

    fn rank(order: &[&str], key: &str) -> usize {
        order.iter().position(|k| *k == key).unwrap_or(order.len())
    }

    fn first(&self, key: &str) -> Option<usize> {
        self.kids.iter().position(|k| k.key == key)
    }

    /// The first child `key` as `text`, or a new one after those of its rank and before.
    fn set(&mut self, order: &[&str], key: &str, text: String) {
        self.changed = true;
        match self.first(key) {
            Some(i) => self.kids[i].text = text,
            None => self.add(order, key, text, None),
        }
    }

    fn add(&mut self, order: &[&str], key: &str, text: String, region: Option<RegionKind>) {
        self.changed = true;
        let r = Self::rank(order, key);
        let at = self.kids.iter().rposition(|k| Self::rank(order, &k.key) <= r).map_or(0, |i| i + 1);
        self.kids.insert(at, Kid { key: key.to_string(), text, region });
    }

    fn remove_first(&mut self, key: &str) {
        if let Some(i) = self.first(key) {
            self.changed = true;
            self.kids.remove(i);
        }
    }

    /// The unmodelled children `had` (as read) replaced by those `want` holds: the ones gone removed, new ones added.
    fn foreign_over(&mut self, order: &[&str], had: Option<&ForeignProps>, want: Option<&ForeignProps>) {
        let list = |f: Option<&ForeignProps>| {
            f.map_or_else(Vec::new, |f| f.children.iter().map(|c| (props_key(&c.name), c.payload.clone())).collect::<Vec<_>>())
        };
        let (had, mut want) = (list(had), list(want));
        if had == want {
            return;
        }
        for (key, text) in had {
            match want.iter().position(|w| w.0 == key && w.1 == text) {
                Some(k) => {
                    want.remove(k);
                }
                None => {
                    if let Some(k) = self.kids.iter().position(|kid| kid.key == key && *kid.text == *text) {
                        self.changed = true;
                        self.kids.remove(k);
                    }
                }
            }
        }
        for (key, text) in want {
            self.add(order, &key, text.to_string(), None);
        }
    }

    /// Its start tag with WordprocessingML attribute `name` (with its prefix) set to `value`, or removed.
    fn set_attr(&mut self, name: &str, value: Option<&str>) {
        self.changed = true;
        self.open = match value {
            Some(v) => with_attr(&self.open, name, v),
            None => strip(&self.open, name),
        };
    }

    /// Child `i` as `text`.
    fn replace(&mut self, i: usize, text: String) {
        self.changed = true;
        self.kids[i].text = text;
    }

    /// The element as written, with what was changed: `xml`, as read, when nothing was.
    fn written(&self, xml: &str) -> String {
        if self.changed { self.xml() } else { xml.to_string() }
    }

    fn xml(&self) -> String {
        let mut s = self.open.clone();
        for k in &self.kids {
            s.push_str(&k.text);
        }
        s.push_str(&self.close);
        s
    }
}

/// `now`'s property element `key` (of the base, or of `region`) written over `origin` (as written, of `was`), what
/// `set` sets of it written again; `None` when nothing is left.
#[allow(clippy::too_many_arguments)]
fn props_over(
    now: &StyleFormat,
    was: &StyleFormat,
    origin: Option<&str>,
    key: &str,
    region: Option<RegionKind>,
    set: &[StyleSet],
    colours: &ThemeColours,
    cx: &Cx,
) -> Option<String> {
    macro_rules! sets {
        ($variant:ident) => {
            set.iter().filter_map(|s| if let StyleSet::$variant(r, p) = s { (*r == region).then_some(*p) } else { None }).collect::<Vec<_>>()
        };
    }
    match key {
        "pPr" => {
            let s: Vec<ParaSet> = sets!(Para);
            para_props::written_over(&now.para, colours, cx, origin.map(|o| (&was.para, o)), &s)
        }
        "rPr" => {
            let s: Vec<Set> = sets!(Char);
            char_props::written_over(&now.char, &RprRevisions::default(), colours, cx, origin.map(|o| (&was.char, o)), &s)
        }
        "tblPr" => {
            let s: Vec<TableSet> = sets!(Table);
            TableDirect::written_over(&now.table, colours, cx, origin.map(|o| (&was.table, o)), &s)
        }
        "trPr" => {
            let s: Vec<RowSet> = sets!(Row);
            RowDirect::written_over(&now.row, colours, cx, origin.map(|o| (&was.row, o)), &s)
        }
        _ => {
            let s: Vec<CellSet> = sets!(Cell);
            CellDirect::written_over(&now.cell, colours, cx, origin.map(|o| (&was.cell, o)), &s)
        }
    }
}

/// `s` as a `w:style` written as an edit of `origin` (the `w:style` the style it holds was read from, and that style)
/// would: each child the edit leaves alone as written there, each it changes in its place (a property element written
/// over the one there by its adapter), a new one in schema order, one it drops removed, a changed own attribute set in
/// the start tag, its other attributes as written there. With no origin, the serializer's.
pub fn written_over(s: &Style, colours: &ThemeColours, cx: &Cx, origin: Option<(&Style, &str)>, set: &[StyleSet]) -> String {
    let Some((read, xml)) = origin else { return serialize(s, colours, cx) };
    let Some(mut el) = Written::parse(xml, cx) else { return serialize(s, colours, cx) };
    let mut out = Out::new(cx, colours);
    // Own attributes
    let own = |q: &Style, local: &str| -> Option<String> {
        match local {
            "type" => Some(kind_literal(q.kind).to_string()),
            "default" => q.default.map(|d| on01(d).to_string()),
            "customStyle" => q.custom.map(|c| on01(c).to_string()),
            _ => Some(q.id.to_string()),
        }
    };
    for (local, prop) in [("type", StyleProp::Kind), ("default", StyleProp::Default), ("customStyle", StyleProp::Custom), ("styleId", StyleProp::Id)]
    {
        let now = own(s, local);
        if own(read, local) == now && !set.contains(&StyleSet::Style(prop)) {
            continue;
        }
        el.set_attr(&format!("{}{local}", cx.w), now.as_deref());
    }
    // Children
    for &key in STYLE.iter().filter(|k| **k != "tblStylePr") {
        let mut render = |q: &Style| {
            modelled_child(&mut out, q, key);
            std::mem::take(&mut out.s)
        };
        let (was, now) = (render(read), render(s));
        let props = PROPS.contains(&key);
        let touched = set.iter().any(|x| if props { sets_props(x, None, key) } else { matches!(x, StyleSet::Style(p) if key_of(*p) == Some(key)) });
        if was == now && !touched {
            continue;
        }
        let text = if now.is_empty() {
            None
        } else if props {
            let origin = el.first(key).map(|i| el.kids[i].text.clone());
            props_over(&s.format, &read.format, origin.as_deref(), key, None, set, colours, cx)
        } else {
            Some(now)
        };
        match text {
            Some(t) => el.set(STYLE, key, t),
            None => el.remove_first(key),
        }
    }
    // Regions
    for kind in RegionKind::ALL {
        let (was, now) = (read.region(kind), s.region(kind));
        let mut render = |r: Option<&Region>| {
            if let Some(r) = r {
                write_region(&mut out, r);
            }
            std::mem::take(&mut out.s)
        };
        let whole = set.contains(&StyleSet::Region(kind));
        let touched = whole || set.iter().any(|x| PROPS.iter().any(|k| sets_props(x, Some(kind), k)));
        let fresh = render(now);
        if render(was) == fresh && !touched {
            continue;
        }
        let at = el.kids.iter().rposition(|k| k.region == Some(kind));
        match (now, was, at) {
            (None, _, Some(i)) => {
                el.changed = true;
                el.kids.remove(i);
            }
            (Some(r), Some(w), Some(i)) if !whole => {
                let text = region_over(r, w, &el.kids[i].text, set, colours, cx).unwrap_or(fresh);
                el.replace(i, text);
            }
            (Some(_), _, Some(i)) => el.replace(i, fresh),
            (Some(_), _, None) => el.add(STYLE, "tblStylePr", fresh, Some(kind)),
            (None, _, None) => {}
        }
    }
    el.foreign_over(STYLE, read.foreign.as_deref(), s.foreign.as_deref());
    el.written(xml)
}

/// Region `r` written over `text`, the `w:tblStylePr` region `was` was read from; `None` when that doesn't parse.
fn region_over(r: &Region, was: &Region, text: &str, set: &[StyleSet], colours: &ThemeColours, cx: &Cx) -> Option<String> {
    let mut el = Written::parse(text, cx)?;
    let mut out = Out::new(cx, colours);
    for &key in &PROPS {
        let mut render = |q: &Region| {
            region_child(&mut out, q, key);
            std::mem::take(&mut out.s)
        };
        let (before, now) = (render(was), render(r));
        let touched = set.iter().any(|x| sets_props(x, Some(r.kind), key));
        if before == now && !touched {
            continue;
        }
        let origin = el.first(key).map(|i| el.kids[i].text.clone());
        let text = if now.is_empty() { None } else { props_over(&r.format, &was.format, origin.as_deref(), key, Some(r.kind), set, colours, cx) };
        match text {
            Some(t) => el.set(&PROPS, key, t),
            None => el.remove_first(key),
        }
    }
    el.foreign_over(&PROPS, was.foreign.as_deref(), r.foreign.as_deref());
    Some(el.written(text))
}

/// What `s` keeps beside each child an edit set whole or changed: nothing of it as read, as writing the child anew
/// keeps nothing of it as written; the property elements' as their adapters drop.
fn drop_replaced(was: &Style, s: &mut Style, set: &[StyleSet]) {
    for r in std::iter::once(None).chain(RegionKind::ALL.map(Some)) {
        macro_rules! sets {
            ($variant:ident) => {
                set.iter().filter_map(|s| if let StyleSet::$variant(q, p) = s { (*q == r).then_some(*p) } else { None }).collect::<Vec<_>>()
            };
        }
        let (p, c, t, w, l) = (sets!(Para), sets!(Char), sets!(Table), sets!(Row), sets!(Cell));
        if p.is_empty() && c.is_empty() && t.is_empty() && w.is_empty() && l.is_empty() {
            continue;
        }
        let before = match r {
            None => Some(&was.format),
            Some(k) => was.region(k).map(|x| &x.format),
        };
        let now = match r {
            None => Some(&mut s.format),
            Some(k) => s.regions.iter_mut().find(|x| x.kind == k).map(|x| &mut x.format),
        };
        let (Some(before), Some(now)) = (before, now) else { continue };
        if !p.is_empty() && now.para.foreign.is_some() {
            para_props::drop_replaced(&before.para, &mut now.para, &p);
        }
        if !c.is_empty() && now.char.foreign.is_some() {
            char_props::drop_replaced(&before.char, &mut now.char, &c);
        }
        if !t.is_empty() && now.table.foreign.is_some() {
            tp::drop_replaced_table(&before.table, &mut now.table, &t);
        }
        if !w.is_empty() && now.row.foreign.is_some() {
            tp::drop_replaced_row(&before.row, &mut now.row, &w);
        }
        if !l.is_empty() && now.cell.foreign.is_some() {
            tp::drop_replaced_cell(&before.cell, &mut now.cell, &l);
        }
    }
    let keys: Vec<&str> = set.iter().filter_map(|x| if let StyleSet::Style(p) = x { key_of(*p) } else { None }).collect();
    if keys.is_empty() {
        return;
    }
    let Some(f) = s.foreign.as_mut() else { return };
    let f = Rc::make_mut(f);
    let of = |q: &QName| &*q.ns == W && keys.contains(&&*q.local);
    let had = was.foreign.as_deref();
    f.attributes.retain(|a| !a.on.as_ref().is_some_and(of) || !had.is_some_and(|h| h.attributes.contains(a)));
    f.kept.retain(|c| !of(&c.name) || !had.is_some_and(|h| h.kept.contains(c)));
    if *f == ForeignProps::default() {
        s.foreign = None;
    }
}

// Document defaults

/// A `w:docDefaults` as typed defaults.
pub fn parse_defaults(el: Node) -> Defaults {
    let mut d = Defaults::default();
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    Compare::new(el, &mut scratch).finish(None, &mut f.attributes);
    let (mut run, mut para) = (false, false);
    for c in el.children().filter(|c| c.is_element()) {
        if is_w(c, "rPrDefault") && !run {
            run = true;
            d.char = Some(default_level(c, "rPr", &mut f, |e| {
                let mut p = char_props::parse(e).0;
                with_owned(&mut p.foreign, e, "rPr");
                p
            }));
        } else if is_w(c, "pPrDefault") && !para {
            para = true;
            d.para = Some(default_level(c, "pPr", &mut f, |e| {
                let mut p = para_props::parse(e);
                with_owned(&mut p.foreign, e, "pPr");
                p
            }));
        } else {
            f.children.push(foreign_child(c));
        }
    }
    d.foreign = foreign_of(f);
    d
}

/// The properties `c` (a `w:rPrDefault` or `w:pPrDefault`) holds in its first `key` child; `c` kept as written in
/// `f` when it holds anything else.
fn default_level<P: Default>(c: Node, key: &str, f: &mut ForeignProps, read: impl FnOnce(Node) -> P) -> P {
    let first = child(c, key);
    let other = c.attributes().next().is_some()
        || c.children().any(|x| (x.is_element() && Some(x) != first) || x.text().is_some_and(|t| !t.trim().is_empty()));
    if other {
        f.kept.push(foreign_child(c));
    }
    first.map(read).unwrap_or_default()
}

/// The `w:rPrDefault` or `w:pPrDefault` of `d`, from its typed value.
fn default_child(out: &mut Out, d: &Defaults, key: &str) {
    let (colours, cx) = (out.colours, out.cx);
    let inner = match key {
        "rPrDefault" => d.char.as_ref().map(|p| char_props::serialize(p, &RprRevisions::default(), colours, cx)),
        _ => d.para.as_ref().map(|p| para_props::serialize(p, Owned::default(), colours, cx)),
    };
    if let Some(x) = inner {
        let w = &cx.w;
        let _ = write!(out.s, "<{w}{key}>{x}</{w}{key}>");
    }
}

/// The `w:rPrDefault` or `w:pPrDefault` of `d`: as written when it was kept so and its value is still the one read.
fn defaults_child(out: &mut Out, d: &Defaults, key: &str) {
    let Some(c) = kept(d.foreign.as_deref(), key) else {
        default_child(out, d, key);
        return;
    };
    let at = out.s.len();
    default_child(out, d, key);
    let now = out.s.split_off(at);
    let same = reread(c, parse_defaults).is_some_and(|q| {
        let at = out.s.len();
        default_child(out, &q, key);
        out.s.split_off(at) == now
    });
    if same {
        out.payload(c);
    } else {
        out.s.push_str(&now);
    }
}

const DEFAULTS: [&str; 2] = ["rPrDefault", "pPrDefault"];

/// `d` as a `w:docDefaults` in the part `cx` describes.
pub fn serialize_defaults(d: &Defaults, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let mut decls = String::new();
    let foreign = d.foreign.as_deref();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}docDefaults{decls}");
    out.free_attrs(foreign.map_or(&[][..], |f| &f.attributes), None, &[]);
    out.s.push('>');
    let rank = |c: &ForeignChild| (&*c.name.ns == W).then(|| DEFAULTS.iter().position(|k| *k == &*c.name.local)).flatten();
    let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(rank).collect());
    for (i, key) in DEFAULTS.iter().enumerate() {
        defaults_child(&mut out, d, key);
        foreign_children(&mut out, foreign, &ranks, Some(i));
    }
    foreign_children(&mut out, foreign, &ranks, None);
    let _ = write!(out.s, "</{w}docDefaults>");
    out.s
}

/// `d` as a `w:docDefaults` written over `origin` (the one the defaults it holds were read from, and those): a level
/// the edit leaves alone as written there, one it changes with its properties written over the ones there by their
/// adapter, what `set` sets written again.
pub fn defaults_over(d: &Defaults, colours: &ThemeColours, cx: &Cx, origin: Option<(&Defaults, &str)>, set: &[DefaultsSet]) -> String {
    let Some((read, xml)) = origin else { return serialize_defaults(d, colours, cx) };
    let Some(mut el) = Written::parse(xml, cx) else { return serialize_defaults(d, colours, cx) };
    let mut out = Out::new(cx, colours);
    for key in DEFAULTS {
        let mut render = |q: &Defaults| {
            defaults_child(&mut out, q, key);
            std::mem::take(&mut out.s)
        };
        let (was, now) = (render(read), render(d));
        let run = key == "rPrDefault";
        let touched = set.iter().any(|s| matches!(s, DefaultsSet::Char(_)) == run);
        if was == now && !touched {
            continue;
        }
        if now.is_empty() {
            el.remove_first(key);
            continue;
        }
        let Some(mut level) = el.first(key).and_then(|i| Written::parse(&el.kids[i].text, cx)) else {
            el.set(&DEFAULTS, key, now);
            continue;
        };
        let inner = if run { "rPr" } else { "pPr" };
        let origin = level.first(inner).map(|i| level.kids[i].text.clone());
        let text = if run {
            let s: Vec<Set> = set.iter().filter_map(|s| if let DefaultsSet::Char(p) = s { Some(*p) } else { None }).collect();
            let (p, q) = (d.char.as_ref().cloned().unwrap_or_default(), read.char.as_ref().cloned().unwrap_or_default());
            char_props::written_over(&p, &RprRevisions::default(), colours, cx, origin.as_deref().map(|o| (&q, o)), &s)
        } else {
            let s: Vec<ParaSet> = set.iter().filter_map(|s| if let DefaultsSet::Para(p) = s { Some(*p) } else { None }).collect();
            let (p, q) = (d.para.as_ref().cloned().unwrap_or_default(), read.para.as_ref().cloned().unwrap_or_default());
            para_props::written_over(&p, colours, cx, origin.as_deref().map(|o| (&q, o)), &s)
        };
        match text {
            Some(t) => level.set(&[inner], inner, t),
            None => level.remove_first(inner),
        }
        let i = el.first(key).unwrap_or_default();
        let text = level.written(&el.kids[i].text);
        el.replace(i, text);
    }
    el.foreign_over(&DEFAULTS, read.foreign.as_deref(), d.foreign.as_deref());
    el.written(xml)
}

// Latent styles

/// A `w:latentStyles` as typed latent styles.
pub fn parse_latent(el: Node) -> LatentStyles {
    let on = |a: &str| attr(el, a).map(|v| on_off_str(Some(v)));
    let mut l = LatentStyles {
        priority: attr(el, "defUIPriority").and_then(|v| v.parse().ok()),
        semi_hidden: on("defSemiHidden"),
        unhide_when_used: on("defUnhideWhenUsed"),
        quick: on("defQFormat"),
        locked: on("defLockedState"),
        count: attr(el, "count").and_then(|v| v.parse().ok()),
        ..LatentStyles::default()
    };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(el, &mut scratch);
    emit_latent(&l, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    for c in el.children().filter(|c| c.is_element()) {
        match attr(c, "name").filter(|_| is_w(c, "lsdException")) {
            Some(name) => l.exceptions.push(read_exception(c, name, &mut scratch)),
            None => f.children.push(foreign_child(c)),
        }
    }
    l.foreign = foreign_of(f);
    l
}

fn read_exception(e: Node, name: &str, scratch: &mut String) -> LatentStyle {
    let on = |a: &str| attr(e, a).map(|v| on_off_str(Some(v)));
    let mut x = LatentStyle {
        name: name.into(),
        priority: attr(e, "uiPriority").and_then(|v| v.parse().ok()),
        semi_hidden: on("semiHidden"),
        unhide_when_used: on("unhideWhenUsed"),
        quick: on("qFormat"),
        locked: on("locked"),
        foreign: None,
    };
    let mut f = ForeignProps::default();
    let mut cmp = Compare::new(e, scratch);
    emit_exception(&x, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    f.children.extend(e.children().filter(|c| c.is_element()).map(foreign_child));
    x.foreign = foreign_of(f);
    x
}

fn emit_latent(l: &LatentStyles, s: &mut dyn Sink) {
    let flag = |s: &mut dyn Sink, name, v: Option<bool>| {
        if let Some(v) = v {
            s.attr(W, name, &on01(v));
        }
    };
    flag(s, "defLockedState", l.locked);
    if let Some(p) = l.priority {
        s.attr(W, "defUIPriority", &p);
    }
    flag(s, "defSemiHidden", l.semi_hidden);
    flag(s, "defUnhideWhenUsed", l.unhide_when_used);
    flag(s, "defQFormat", l.quick);
    if let Some(c) = l.count {
        s.attr(W, "count", &c);
    }
}

fn emit_exception(x: &LatentStyle, s: &mut dyn Sink) {
    let flag = |s: &mut dyn Sink, name, v: Option<bool>| {
        if let Some(v) = v {
            s.attr(W, name, &on01(v));
        }
    };
    s.attr(W, "name", &&*x.name);
    flag(s, "locked", x.locked);
    flag(s, "semiHidden", x.semi_hidden);
    if let Some(p) = x.priority {
        s.attr(W, "uiPriority", &p);
    }
    flag(s, "unhideWhenUsed", x.unhide_when_used);
    flag(s, "qFormat", x.quick);
}

fn write_exception(out: &mut Out, x: &LatentStyle) {
    let w = out.cx.w.clone();
    let foreign = x.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let _ = write!(out.s, "<{w}lsdException");
    let written = out.attrs(W, attrs, None, |s| emit_exception(x, s));
    out.free_attrs(attrs, None, &written);
    match foreign.filter(|f| !f.children.is_empty()) {
        Some(f) => {
            out.s.push('>');
            for c in &f.children {
                out.payload(c);
            }
            let _ = write!(out.s, "</{w}lsdException>");
        }
        None => out.s.push_str("/>"),
    }
}

/// `l` as a `w:latentStyles` in the part `cx` describes.
pub fn serialize_latent(l: &LatentStyles, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let mut decls = String::new();
    out.declarations(l.foreign.as_deref(), &mut decls);
    for x in &l.exceptions {
        out.declarations(x.foreign.as_deref(), &mut decls);
    }
    let w = cx.w.clone();
    let attrs = l.foreign.as_deref().map_or(&[][..], |f| &f.attributes);
    let _ = write!(out.s, "<{w}latentStyles{decls}");
    let written = out.attrs(W, attrs, None, |s| emit_latent(l, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    for x in &l.exceptions {
        write_exception(&mut out, x);
    }
    for c in l.foreign.as_deref().map_or(&[][..], |f| &f.children) {
        out.payload(c);
    }
    let _ = write!(out.s, "</{w}latentStyles>");
    out.s
}

/// `l` as a `w:latentStyles` written over `origin` (the one the latent styles it holds were read from, and those): its
/// start tag as written there with each changed default set, each exception read alike as written there, the others
/// and the unmodelled children from their typed value.
pub fn latent_over(l: &LatentStyles, colours: &ThemeColours, cx: &Cx, origin: Option<(&LatentStyles, &str)>) -> String {
    let Some((read, xml)) = origin else { return serialize_latent(l, colours, cx) };
    let Some(mut el) = Written::parse(xml, cx) else { return serialize_latent(l, colours, cx) };
    let mut out = Out::new(cx, colours);
    let on = |v: Option<bool>| v.map(|v| on01(v).to_string());
    let num = |v: Option<u32>| v.map(|v| v.to_string());
    let values = |q: &LatentStyles| {
        [
            ("defLockedState", on(q.locked)),
            ("defUIPriority", num(q.priority)),
            ("defSemiHidden", on(q.semi_hidden)),
            ("defUnhideWhenUsed", on(q.unhide_when_used)),
            ("defQFormat", on(q.quick)),
            ("count", num(q.count)),
        ]
    };
    for ((local, was), (_, now)) in values(read).into_iter().zip(values(l)) {
        if was == now {
            continue;
        }
        el.set_attr(&format!("{}{local}", cx.w), now.as_deref());
    }
    let children = |q: &LatentStyles| q.foreign.as_ref().map(|f| f.children.clone()).unwrap_or_default();
    if !el.changed && l.exceptions == read.exceptions && children(l) == children(read) {
        return xml.to_string();
    }
    // Exceptions
    let written: Vec<&str> = el.kids.iter().filter(|k| k.key == "lsdException").map(|k| k.text.as_str()).collect();
    let named: Vec<&str> = written.into_iter().filter(|t| cx.parse(t, |_, e| attr(e, "name").is_some()).unwrap_or(false)).collect();
    let mut taken = vec![false; read.exceptions.len()];
    let mut kids = String::new();
    for x in &l.exceptions {
        let was = (0..read.exceptions.len()).find(|&i| !taken[i] && read.exceptions[i] == *x);
        match was.and_then(|i| named.get(i).map(|t| (i, t))) {
            Some((i, t)) => {
                taken[i] = true;
                kids.push_str(t);
            }
            None => {
                write_exception(&mut out, x);
                kids.push_str(&std::mem::take(&mut out.s));
            }
        }
    }
    for c in l.foreign.as_deref().map_or(&[][..], |f| &f.children) {
        out.payload(c);
    }
    kids.push_str(&out.s);
    format!("{}{kids}{}", el.open, el.close)
}

// Holders

/// One kind of definition element: its typed value's parse, serializer and writer over the element as read.
trait Element: Clone + Default + PartialEq + std::fmt::Debug {
    type Set: Copy + std::fmt::Debug;
    fn parse(el: Node) -> Self;
    fn written_over(&self, colours: &ThemeColours, cx: &Cx, origin: Option<(&Self, &str)>, set: &[Self::Set]) -> String;
    fn drop_replaced(_was: &Self, _now: &mut Self, _set: &[Self::Set]) {}
}

impl Element for Style {
    type Set = StyleSet;
    fn parse(el: Node) -> Style {
        parse(el).unwrap_or_default()
    }
    fn written_over(&self, colours: &ThemeColours, cx: &Cx, origin: Option<(&Style, &str)>, set: &[StyleSet]) -> String {
        written_over(self, colours, cx, origin, set)
    }
    fn drop_replaced(was: &Style, now: &mut Style, set: &[StyleSet]) {
        drop_replaced(was, now, set);
    }
}

impl Element for Defaults {
    type Set = DefaultsSet;
    fn parse(el: Node) -> Defaults {
        parse_defaults(el)
    }
    fn written_over(&self, colours: &ThemeColours, cx: &Cx, origin: Option<(&Defaults, &str)>, set: &[DefaultsSet]) -> String {
        defaults_over(self, colours, cx, origin, set)
    }
    fn drop_replaced(was: &Defaults, now: &mut Defaults, set: &[DefaultsSet]) {
        let c: Vec<Set> = set.iter().filter_map(|s| if let DefaultsSet::Char(p) = s { Some(*p) } else { None }).collect();
        let p: Vec<ParaSet> = set.iter().filter_map(|s| if let DefaultsSet::Para(p) = s { Some(*p) } else { None }).collect();
        if let (Some(b), Some(n)) = (&was.char, &mut now.char)
            && !c.is_empty()
            && n.foreign.is_some()
        {
            char_props::drop_replaced(b, n, &c);
        }
        if let (Some(b), Some(n)) = (&was.para, &mut now.para)
            && !p.is_empty()
            && n.foreign.is_some()
        {
            para_props::drop_replaced(b, n, &p);
        }
    }
}

impl Element for LatentStyles {
    type Set = std::convert::Infallible;
    fn parse(el: Node) -> LatentStyles {
        parse_latent(el)
    }
    fn written_over(&self, colours: &ThemeColours, cx: &Cx, origin: Option<(&LatentStyles, &str)>, _: &[std::convert::Infallible]) -> String {
        latent_over(self, colours, cx, origin)
    }
}

/// What the WordprocessingML adapter adds to a style, the document defaults and the latent styles: the element each
/// was read from, and the element it is written as.
pub trait StyleXml: Sized {
    /// The typed definition.
    type Def;
    /// What an edit sets.
    type Set;

    /// A definition read from `xml`, the element it was parsed from.
    fn read(props: Self::Def, xml: Rc<str>) -> Self;

    /// The definition `xml` (the element in the part `cx` describes) holds; an empty one, the XML kept as written, for
    /// XML that doesn't parse.
    fn from_xml(xml: &str, cx: &Cx) -> Self;

    /// The element as read, while the definition is the one read from it.
    fn as_read(&self) -> Option<&Rc<str>>;

    /// The element it was read from (or last written as), whatever it is now.
    fn origin_xml(&self) -> Option<&Rc<str>>;

    /// The element to write in the part `cx` describes: as read while the definition is that, else written over it (a
    /// theme colour's value as `colours` resolve it).
    fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Cow<'_, str>;

    /// `props` in place of this definition, set by an edit that set `set`, in order: the element it was read from kept
    /// to write it over, what was set written again.
    fn edited(&self, props: Self::Def, set: &[Self::Set]) -> Self;

    /// This definition where `cx` describes the part it is written in: `None` while the element it was read from
    /// reads the same there, else without it, to be written from the definition.
    fn moved_to(&self, cx: &Cx) -> Option<Self>;
}

/// A holder of one kind of definition, with its parse and writer over the XML read.
macro_rules! holder {
    ($(#[$doc:meta])* $name:ident, $t:ty, $set:ty) => {
        $(#[$doc])*
        ///
        /// The written form stands for the definition only while it is the one it was read as; never part of
        /// equality: use [`Authored::authored_eq`].
        pub type $name = Authored<Rc<$t>, $set>;

        impl StyleXml for $name {
            type Def = $t;
            type Set = $set;

            fn read(props: $t, xml: Rc<str>) -> $name {
                as_read::read(Rc::new(props), xml)
            }

            fn from_xml(xml: &str, cx: &Cx) -> $name {
                $name::read(cx.parse(xml, |_, el| <$t as Element>::parse(el)).unwrap_or_default(), xml.into())
            }

            fn as_read(&self) -> Option<&Rc<str>> {
                as_read::as_read(self, |read, now| read == now)
            }

            fn origin_xml(&self) -> Option<&Rc<str>> {
                as_read::origin_xml(self)
            }

            fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Cow<'_, str> {
                if let Some(x) = self.as_read() {
                    return Cow::Borrowed(x);
                }
                Cow::Owned(self.get().written_over(colours, cx, as_read::record(self), self.set()))
            }

            fn edited(&self, mut props: $t, set: &[$set]) -> $name {
                self.edited_with(set, |was, all| {
                    <$t as Element>::drop_replaced(was, &mut props, all);
                    Rc::new(props)
                })
            }

            fn moved_to(&self, cx: &Cx) -> Option<$name> {
                let (read, xml) = as_read::record(self)?;
                if cx.parse(xml, |_, el| <$t as Element>::parse(el)).is_some_and(|p| p == *read) {
                    return None;
                }
                Some(self.with_origin(crate::document::origin::Origin::NONE))
            }
        }
    };
}

holder!(
    /// A style: typed, with the `w:style` it was read from in its origin.
    StyleDef, Style, StyleSet
);
holder!(
    /// The document defaults: typed, with the `w:docDefaults` they were read from in their origin.
    DefaultsDef, Defaults, DefaultsSet
);
holder!(
    /// The latent styles: typed, with the `w:latentStyles` they were read from in their origin. An edit replaces them
    /// whole.
    LatentDef, LatentStyles, std::convert::Infallible
);

// A whole styles part

/// Word's reading of a styles part in order replayed, for its table styles: each style's properties are kept once
/// worked out, and worked out again after the last style only for those whose own properties were stored since; a
/// table style worked out while its base wasn't read yet keeps what it had then.
#[derive(Default)]
struct Expansions {
    /// Per style read: its base if read before it, and whether its properties are worked out.
    read: HashMap<Rc<str>, (Option<Rc<str>>, bool)>,
}

impl Expansions {
    fn expand(&mut self, id: &str) {
        let mut cur: Option<Rc<str>> = Some(id.into());
        let mut seen = 0;
        while let Some(e) = cur.take().and_then(|c| self.read.get_mut(&c)).filter(|e| !e.1 && seen < 32) {
            e.1 = true;
            cur = e.0.clone();
            seen += 1;
        }
    }

    fn clear(&mut self, id: &str) {
        if let Some(e) = self.read.get_mut(id) {
            e.1 = false;
        }
    }

    /// Replays the reader's steps for table style `n`: own run, paragraph and table properties are stored at its first
    /// `w:tblStylePr` and at its end, and each region is applied. Whether its run properties take Word's built-in
    /// defaults (stored at a region while its base is missing).
    fn table_style(&mut self, n: Node, id: &str) -> bool {
        let (mut rpr, mut ppr, mut tbl, mut builtin) = (false, false, false, false);
        let set = |c: Node| c.children().any(|e| e.is_element());
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "rPr" => rpr |= set(c),
                "pPr" => ppr |= set(c),
                "tblPr" | "trPr" | "tcPr" => tbl |= set(c),
                "tblStylePr" => {
                    if rpr {
                        if self.read.get(id).is_some_and(|e| e.0.is_none()) {
                            builtin = true;
                            self.expand(id);
                        } else {
                            self.clear(id);
                        }
                    }
                    if ppr {
                        self.clear(id);
                    }
                    if tbl {
                        self.expand(id);
                    }
                    (rpr, ppr, tbl) = (false, false, false);
                    if attr(c, "type") != Some("wholeTable") && set(c) {
                        self.expand(id);
                    }
                }
                _ => {}
            }
        }
        if tbl {
            self.expand(id);
        }
        if ppr || rpr {
            self.clear(id);
        }
        builtin
    }
}

/// Why a style edit is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleEditError {
    /// The XML isn't one `w:style` element with a `w:styleId`.
    Malformed,
    /// It would be based on itself, through its chain.
    Loop,
    /// No such style.
    Missing,
}

#[cfg(test)]
impl From<styles::StyleEditError> for StyleEditError {
    fn from(e: styles::StyleEditError) -> StyleEditError {
        match e {
            styles::StyleEditError::Malformed => StyleEditError::Malformed,
            styles::StyleEditError::Loop => StyleEditError::Loop,
            styles::StyleEditError::Missing => StyleEditError::Missing,
        }
    }
}

/// A styles part read: its definitions, each style's, the defaults' and the latent styles' holders, and its root start
/// tag.
#[derive(Clone, Debug, Default)]
pub struct StylesPart {
    /// The definitions.
    pub sheet: StyleSheet,
    /// Each style's holder, by id: the one the sheet holds.
    pub styles: HashMap<Rc<str>, StyleDef>,
    /// The defaults' holder.
    pub defaults: Option<DefaultsDef>,
    /// The latent styles' holder.
    pub latent: Option<LatentDef>,
    /// The part's root start tag, whose namespace declarations its definitions are read and written under.
    pub root: Rc<str>,
}

impl StylesPart {
    /// The styles part `xml`, read as the reader reads it: a style without an id skipped, of a repeated id the last
    /// (at its place), of the defaults and latent styles the first; none for no part or one that doesn't parse.
    pub fn parse(xml: Option<&str>) -> StylesPart {
        let mut part = StylesPart::default();
        let Some(text) = xml else { return part };
        let Ok(doc) = roxmltree::Document::parse(text) else { return part };
        let root = doc.root_element();
        let open = &text[root.range().start..];
        part.root = open[..super::binding::tag_end(open).map_or(0, |e| e + 1)].into();
        if let Some(n) = child(root, "latentStyles") {
            let l = LatentDef::read(parse_latent(n), text[n.range()].into());
            part.sheet.latent = Some(l.get().clone());
            part.latent = Some(l);
        }
        if let Some(n) = child(root, "docDefaults") {
            let d = DefaultsDef::read(parse_defaults(n), text[n.range()].into());
            part.sheet.defaults = Some(d.get().clone());
            part.defaults = Some(d);
        }
        let mut nodes: HashMap<Rc<str>, (u32, Node)> = HashMap::new();
        for (k, n) in children(root, "style").enumerate() {
            let Some(style) = parse(n) else { continue };
            let id = style.id.clone();
            part.styles.insert(id.clone(), StyleDef::read(style, text[n.range()].into()));
            nodes.insert(id, (k as u32, n));
        }
        part.sheet.styles = nodes
            .iter()
            .map(|(id, (k, _))| {
                let style = part.styles[id].get().clone();
                (id.clone(), StyleEntry { style, order: Some(*k), origin: Origin::Document, frozen: false, word_run_defaults: false })
            })
            .collect();
        finish(&mut part.sheet, None, &mut |id, f| {
            if let Some((_, n)) = nodes.get(id) {
                f(*n);
            }
        });
        part
    }

    /// The prefix and declarations its definitions are written with.
    pub fn cx(&self) -> Cx {
        if self.root.is_empty() {
            return Cx::of_xml(&format!("<w:styles xmlns:w=\"{}\">", super::write::props::W_NS));
        }
        Cx::of_xml(&self.root)
    }
}

impl StylesPart {
    /// What depends on the styles together worked out again, as Word reads the part again after an edit: over each
    /// style's XML as it writes it now, `fresh` (the styles an edit has just set) read as written and the others as
    /// the last working out left them (Word's built-in default table style without a base); with no `fresh`, as on
    /// first reading.
    pub fn refinish(&mut self, colours: &ThemeColours, fresh: Option<&[&str]>) {
        let cx = self.cx();
        let styles = &self.styles;
        finish(&mut self.sheet, fresh, &mut |id, f| {
            if let Some(d) = styles.get(id) {
                cx.parse(&d.xml(colours, &cx), |_, n| f(n));
            }
        });
    }

    /// `def` in place of the style with its id (at its place in the part, from its part), or added after the styles
    /// read; then what depends on the styles together worked out again. Refused, changing nothing, when it would be
    /// based on itself through the styles as they are ([`Self::loops`]).
    pub fn set(&mut self, def: StyleDef, colours: &ThemeColours) -> Result<(), StyleEditError> {
        if self.loops(def.get()) {
            return Err(StyleEditError::Loop);
        }
        let id = def.get().id.clone();
        self.put(def);
        self.refinish(colours, Some(&[&id]));
        Ok(())
    }

    /// The style `raw` (one `w:style` alone) holds, as [`Self::set`] would take it: `None` when it isn't one, or
    /// would be based on itself.
    pub fn style_from_xml(&self, raw: &str) -> Option<Style> {
        let cx = self.cx();
        let s = cx.parse(raw, |_, el| (alone(el) && is_w(el, "style")).then(|| parse(el)).flatten()).flatten()?;
        (!self.loops(&s)).then_some(s)
    }

    /// Whether `s` would be based on itself: its bases through the sheet's styles (Word's built-in default table
    /// style without one, as the last working out left it) reach its id, or go past 32 of them.
    pub fn loops(&self, s: &Style) -> bool {
        let mut base = s.based_on.as_deref();
        let mut hops = 0;
        while let Some(b) = base {
            if b == &*s.id || hops > 32 {
                return true;
            }
            base = self.sheet.styles.get(b).and_then(|e| e.style.based_on.as_deref().filter(|_| !e.style.word_default_table()));
            hops += 1;
        }
        false
    }

    fn put(&mut self, def: StyleDef) {
        let id = def.get().id.clone();
        let (order, origin) = self.sheet.styles.get(&id).map_or((None, Origin::Document), |e| (e.order, e.origin));
        let entry = StyleEntry { style: def.get().clone(), order, origin, frozen: false, word_run_defaults: false };
        self.sheet.styles.insert(id.clone(), entry);
        self.styles.insert(id, def);
    }

    /// Removes style `id`: the styles based on it take its base, and references to it from a next style or a link
    /// go; then what depends on the styles together is worked out again. False when there is no such style.
    pub fn delete(&mut self, id: &str, colours: &ThemeColours) -> bool {
        let Some(gone) = self.sheet.styles.get(id) else { return false };
        // The base as the last working out left it
        let base = gone.style.based_on.clone().filter(|_| !gone.style.word_default_table());
        let based = |s: &Style| s.based_on.as_deref() == Some(id) && !s.word_default_table();
        let mut refs: Vec<Rc<str>> = self
            .sheet
            .styles
            .values()
            .map(|e| &*e.style)
            .filter(|s| &*s.id != id && (based(s) || s.next.as_deref() == Some(id) || s.link.as_deref() == Some(id)))
            .map(|s| s.id.clone())
            .collect();
        refs.sort();
        for r in &refs {
            let Some(e) = self.sheet.styles.get(r) else { continue };
            let mut s = (*e.style).clone();
            let mut set = Vec::new();
            if based(&s) {
                s.based_on = base.clone();
                set.push(StyleSet::Style(StyleProp::BasedOn));
            }
            if s.next.as_deref() == Some(id) {
                s.next = None;
                set.push(StyleSet::Style(StyleProp::Next));
            }
            if s.link.as_deref() == Some(id) {
                s.link = None;
                set.push(StyleSet::Style(StyleProp::Link));
            }
            match self.styles.get(r) {
                Some(d) => {
                    let d = d.edited(s, &set);
                    self.put(d);
                }
                None => {
                    if let Some(e) = self.sheet.styles.get_mut(r) {
                        e.style = Rc::new(s);
                    }
                }
            }
        }
        self.sheet.styles.remove(id);
        self.styles.remove(id);
        let fresh: Vec<&str> = refs.iter().map(|r| &**r).collect();
        self.refinish(colours, Some(&fresh));
        true
    }
}

/// What depends on the styles together, worked out as reading the part in order does: the table styles' read-order
/// facts, the default styles and the paragraph styles' order, the glossary's after the document's. `fresh`: `None`
/// on first reading, else the styles just set on working it out again; `xml_of` gives each table style's element.
fn finish(sheet: &mut StyleSheet, fresh: Option<&[&str]>, xml_of: &mut dyn FnMut(&str, &mut dyn FnMut(Node))) {
    let mut ids: Vec<(Option<u32>, Rc<str>)> =
        sheet.styles.values().filter(|e| e.origin == Origin::Document).map(|e| (e.order, e.style.id.clone())).collect();
    ids.sort_unstable_by(|a, b| (a.0.is_none(), a.0, &a.1).cmp(&(b.0.is_none(), b.0, &b.1)));
    let glossary: Vec<Rc<str>> =
        sheet.paragraph_order.iter().filter(|id| sheet.styles.get(*id).is_some_and(|e| e.origin == Origin::Glossary)).cloned().collect();
    let mut expansions = Expansions::default();
    let (mut order, mut paragraph, mut table, mut character, mut numbering) = (Vec::new(), None, None, None, None);
    for (_, id) in &ids {
        let Some(e) = sheet.styles.get_mut(id) else { continue };
        let s = e.style.clone();
        // Word's built-in default table style read before without a base, unless just set
        let unbased = fresh.is_some_and(|f| !f.contains(&&**id)) && s.word_default_table();
        let base = s.based_on.clone().filter(|b| !unbased && b != id && expansions.read.contains_key(b));
        expansions.read.entry(id.clone()).or_insert((base, false));
        let default = s.default == Some(true);
        (e.frozen, e.word_run_defaults) = (false, false);
        match s.kind {
            StyleKind::Table => {
                let mut builtin = false;
                xml_of(id, &mut |n| builtin = expansions.table_style(n, id));
                if let Some(e) = sheet.styles.get_mut(id) {
                    e.word_run_defaults = builtin;
                }
                if default && table.is_none() {
                    table = Some(id.clone());
                }
            }
            StyleKind::Paragraph => {
                order.push(id.clone());
                if default {
                    paragraph = Some(id.clone());
                }
            }
            StyleKind::Character if default && character.is_none() => character = Some(id.clone()),
            StyleKind::Numbering if default && numbering.is_none() => numbering = Some(id.clone()),
            _ => {}
        }
    }
    if paragraph.is_none() && sheet.styles.get("Normal").is_some_and(|e| e.style.kind == StyleKind::Paragraph) {
        paragraph = Some("Normal".into());
    }
    // Frozen table styles
    for (id, (base, done)) in expansions.read {
        if let Some(e) =
            sheet.styles.get_mut(&id).filter(|e| e.style.kind == StyleKind::Table && e.style.based_on.is_some() && !e.style.word_default_table())
        {
            e.frozen = done && base.is_none();
        }
    }
    order.extend(glossary);
    (sheet.paragraph_order, sheet.default_paragraph, sheet.default_table, sheet.default_character, sheet.default_numbering) =
        (order, paragraph, table, character, numbering);
}

// Conversion to the XML path's structures

/// Whether `s` writes conditional regions of a type none of [`RegionKind`]'s names: they format nothing, but count as
/// regions it has.
pub fn other_regions(s: &Style) -> bool {
    let regions = s.foreign.as_deref().map_or(&[][..], |f| &f.children).iter().filter(|c| &*c.name.ns == W && &*c.name.local == "tblStylePr");
    regions
        .into_iter()
        .any(|c| reread(c, |x| x.first_element_child().and_then(|r| attr(r, "type")).is_some_and(|t| region_kind(Some(t)).is_none())) == Some(true))
}

/// The character properties of `s`'s conditional regions of the types not modelled: the last of each type.
pub fn other_region_chars(s: &Style) -> Vec<CharProps> {
    let mut by_type: HashMap<String, CharProps> = HashMap::new();
    for c in s.foreign.as_deref().map_or(&[][..], |f| &f.children) {
        if &*c.name.ns != W || &*c.name.local != "tblStylePr" {
            continue;
        }
        let other = reread(c, |x| {
            let r = x.first_element_child()?;
            let t = attr(r, "type").filter(|t| region_kind(Some(t)).is_none())?;
            let mut f = StyleFormat::default();
            region_children(&mut f, r, &mut ForeignProps::default());
            Some((t.to_string(), f.char))
        });
        if let Some((t, c)) = other.flatten() {
            by_type.insert(t, c);
        }
    }
    by_type.into_values().collect()
}

#[cfg(test)]
/// A level as the XML path holds it, a theme colour's value as `colours` resolve it.
fn frozen_region(f: &StyleFormat, colours: &ThemeColours) -> styles::Region {
    let mut tc = tp::tc_props(&f.cell, colours);
    // Text flow as written
    let flow = f.cell.foreign.as_deref().and_then(|x| {
        x.attributes.iter().find(|a| {
            a.on.as_ref().is_some_and(|q| &*q.ns == W && &*q.local == "textDirection")
                && &*a.name.local == "val"
                && a.stands_for.as_deref() == tc.text_direction.as_deref()
        })
    });
    if let Some(a) = flow {
        tc.text_direction = Some(a.value.to_string());
    }
    styles::Region {
        ppr: para_props::to_para_props(&f.para, colours),
        rpr: char_props::to_run_props(&f.char, colours),
        tbl: tp::tbl_props(&f.table, colours),
        tc,
        tr: tp::tr_props(&f.row),
        size_over_normal: None,
        sets_text: crate::format::sets_text(f),
        mark_at_style: false,
    }
}

#[cfg(test)]
/// Style entry `e` of a sheet read from a styles part, written `raw`, as the XML path's reader holds it after reading
/// the part: theme colours resolved by `colours`, a table style's run properties from Word's built-in table defaults
/// where it takes them, and Word's built-in default table style's fixed properties in place of its own.
pub fn frozen_style(e: &StyleEntry, raw: &str, colours: &ThemeColours) -> styles::Style {
    let s = &*e.style;
    let mut base = frozen_region(&s.format, colours);
    let mut regions: HashMap<String, styles::Region> = HashMap::new();
    for c in s.foreign.as_deref().map_or(&[][..], |f| &f.children) {
        if &*c.name.ns != W || &*c.name.local != "tblStylePr" {
            continue;
        }
        // Regions of other types
        let other = reread(c, |x| {
            let r = x.first_element_child()?;
            let t = attr(r, "type").filter(|t| region_kind(Some(t)).is_none())?;
            let mut f = StyleFormat::default();
            region_children(&mut f, r, &mut ForeignProps::default());
            Some((t.to_string(), f))
        });
        if let Some((t, f)) = other.flatten() {
            regions.insert(t, frozen_region(&f, colours));
        }
    }
    for r in &s.regions {
        regions.insert(region_literal(r.kind).to_string(), frozen_region(&r.format, colours));
    }
    if e.word_run_defaults {
        let r = &mut base.rpr;
        r.size = r.size.or(Some(10.0));
        r.color = r.color.or(Some(None));
        r.spacing = r.spacing.or(Some(0.0));
        r.scale = r.scale.or(Some(100.0));
    }
    let mut based_on = s.based_on.as_deref().map(str::to_string);
    if s.word_default_table() {
        let margins = xml_props::Margins { top: Some(0.0), left: Some(5.4), bottom: Some(0.0), right: Some(5.4) };
        let tbl = xml_props::TblProps { ind: Some(crate::document::format::Width::Dxa(0.0)), margins, ..xml_props::TblProps::default() };
        base = styles::Region { tbl, ..styles::Region::default() };
        regions.clear();
        based_on = None;
    }
    let kind = match s.kind {
        StyleKind::Paragraph => styles::StyleKind::Paragraph,
        StyleKind::Character => styles::StyleKind::Character,
        StyleKind::Table => styles::StyleKind::Table,
        StyleKind::Numbering => styles::StyleKind::Numbering,
    };
    let text = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
    styles::Style {
        id: s.id.to_string(),
        name: s.name.as_deref().unwrap_or_default().to_string(),
        aliases: s.aliases.iter().map(|a| a.to_string()).collect(),
        quick: s.quick.unwrap_or(false),
        kind,
        based_on,
        next: text(&s.next),
        default: s.default.unwrap_or(false),
        link: text(&s.link),
        priority: s.priority,
        meta: styles::StyleMeta {
            semi_hidden: s.meta.semi_hidden.unwrap_or(false),
            hidden: s.meta.hidden.unwrap_or(false),
            unhide_when_used: s.meta.unhide_when_used.unwrap_or(false),
            locked: s.meta.locked.unwrap_or(false),
            auto_redefine: s.meta.auto_redefine.unwrap_or(false),
        },
        custom: s.custom.unwrap_or(false),
        base,
        regions,
        origin: match e.origin {
            Origin::Document => styles::StyleOrigin::Document,
            Origin::Glossary => styles::StyleOrigin::Glossary,
        },
        raw: raw.to_string(),
        frozen: e.frozen,
        order: e.order,
    }
}

#[cfg(test)]
/// The document's run and paragraph defaults as the XML path holds them: the defaults' own, else Word's.
pub fn frozen_defaults(d: Option<&Defaults>, colours: &ThemeColours) -> (xml_props::RunProps, xml_props::ParaProps) {
    let run = d
        .and_then(|d| d.char.as_ref())
        .map_or_else(|| char_props::to_run_props(&Defaults::word_char(), colours), |c| char_props::to_run_props(c, colours));
    let para = d
        .and_then(|d| d.para.as_ref())
        .map_or_else(|| para_props::to_para_props(&Defaults::word_para(), colours), |p| para_props::to_para_props(p, colours));
    (run, para)
}

#[cfg(test)]
/// Latent styles as the XML path holds them, written `raw`.
pub fn frozen_latent(l: Option<&LatentStyles>, raw: Option<&str>) -> styles::LatentStyles {
    let Some(l) = l else { return styles::LatentStyles::default() };
    styles::LatentStyles {
        priority: l.priority,
        semi_hidden: l.semi_hidden.unwrap_or(false),
        unhide_when_used: l.unhide_when_used.unwrap_or(false),
        quick: l.quick.unwrap_or(false),
        locked: l.locked.unwrap_or(false),
        exceptions: l
            .exceptions
            .iter()
            .map(|x| styles::LatentStyle {
                name: x.name.to_string(),
                priority: x.priority,
                semi_hidden: x.semi_hidden,
                unhide_when_used: x.unhide_when_used,
                quick: x.quick,
                locked: x.locked,
            })
            .collect(),
        raw: raw.map(str::to_string),
    }
}
