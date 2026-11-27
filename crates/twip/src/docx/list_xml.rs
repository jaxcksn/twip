//! Typed list definitions from and to the numbering part: `w:abstractNum`, `w:num`, `w:numPicBullet` and their
//! `w:lvl`s
//!
//! As for paragraph properties ([`super::para_props`]), the parse keeps in [`ForeignProps`] what the typed value does
//! not hold, so that writing the value again loses nothing of what was read: unmodelled children and attributes,
//! modelled attributes written otherwise than the writer writes their value, a modelled child with content whole
//! beside its typed value, and the children that don't count whole. Which child counts is the XML path's
//! ([`super::numbering`]): of a level's, a definition's and a list's single children the first; of a definition's
//! levels the last of each `w:ilvl` (a level past the ninth never counts); of a list's overrides the last of each
//! `w:ilvl` for its level, each of its start and its level from the last that sets it. The serializer writes the
//! children that don't count where they keep not counting: before the ones that count where the last counts, after
//! them where the first does.
//!
//! A level's `w:pPr` and `w:rPr` are typed by the paragraph and run adapters; one the typed value would not write again
//! (empty, or with a tracked change, a mark's properties or revisions) is kept whole beside it too.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::rc::Rc;

use roxmltree::Node;

use super::char_props::{self, RprRevisions};
use super::numbering::{AbstractNum, Level, Num, Numbering, PicBullet};
use super::para_props::{self, Owned};
use super::prop_xml::{Compare, Out, Sink, Twips, foreign_child, push_esc, read_attr, strip, with_attr};
use super::props::{ParaProps as XmlParaProps, RunProps, ThemeColours};
use super::styles::StyleOrigin;
use super::write::props::{Cx, W_NS};
use super::xml::{R, W, attr, attr_twips, child, children, int, on_off, on_off_str, val};
use crate::document::paragraph::{ListId, ListJc, ListSuffix};
use crate::format::{
    CharProps, ForeignChild, ForeignProps, LegacyLabel, LevelOverride, LevelProp, List, ListDef, ListDefId, ListDefProp, ListLevel, ListProp, Lists,
    Literal, MultiLevelType, NumberFormat, Origin, OverrideProp, ParaProps, ParaSet, PictureBullet, QName, Set,
};

/// CT_Lvl's children in schema order.
const LEVEL: [&str; 12] = ["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff", "lvlText", "lvlPicBulletId", "legacy", "lvlJc", "pPr", "rPr"];

/// CT_AbstractNum's children in schema order.
const DEF: [&str; 7] = ["nsid", "multiLevelType", "tmpl", "name", "styleLink", "numStyleLink", "lvl"];

/// CT_Num's children in schema order.
const LIST: [&str; 2] = ["abstractNumId", "lvlOverride"];

/// CT_NumLvl's children in schema order.
const OVERRIDE: [&str; 2] = ["startOverride", "lvl"];

/// The modelled attributes of the elements and their children, in the order they are written.
const WRITTEN: [&str; 11] =
    ["ilvl", "tplc", "tentative", "abstractNumId", "numId", "numPicBulletId", "val", "format", "legacy", "legacySpace", "legacyIndent"];

/// The children of a `w:pPr` other nodes own where it is a paragraph's.
const PPR_OWNED: [&str; 3] = ["rPr", "sectPr", "pPrChange"];

/// ST_NumberFormat's values.
const FORMATS: [(&str, NumberFormat); 63] = {
    use NumberFormat::*;
    [
        ("decimal", Decimal),
        ("upperRoman", UpperRoman),
        ("lowerRoman", LowerRoman),
        ("upperLetter", UpperLetter),
        ("lowerLetter", LowerLetter),
        ("ordinal", Ordinal),
        ("cardinalText", CardinalText),
        ("ordinalText", OrdinalText),
        ("hex", Hex),
        ("chicago", Chicago),
        ("ideographDigital", IdeographDigital),
        ("japaneseCounting", JapaneseCounting),
        ("aiueo", Aiueo),
        ("iroha", Iroha),
        ("decimalFullWidth", DecimalFullWidth),
        ("decimalHalfWidth", DecimalHalfWidth),
        ("japaneseLegal", JapaneseLegal),
        ("japaneseDigitalTenThousand", JapaneseDigitalTenThousand),
        ("decimalEnclosedCircle", DecimalEnclosedCircle),
        ("decimalFullWidth2", DecimalFullWidth2),
        ("aiueoFullWidth", AiueoFullWidth),
        ("irohaFullWidth", IrohaFullWidth),
        ("decimalZero", DecimalZero),
        ("bullet", Bullet),
        ("ganada", Ganada),
        ("chosung", Chosung),
        ("decimalEnclosedFullstop", DecimalEnclosedFullstop),
        ("decimalEnclosedParen", DecimalEnclosedParen),
        ("decimalEnclosedCircleChinese", DecimalEnclosedCircleChinese),
        ("ideographEnclosedCircle", IdeographEnclosedCircle),
        ("ideographTraditional", IdeographTraditional),
        ("ideographZodiac", IdeographZodiac),
        ("ideographZodiacTraditional", IdeographZodiacTraditional),
        ("taiwaneseCounting", TaiwaneseCounting),
        ("ideographLegalTraditional", IdeographLegalTraditional),
        ("taiwaneseCountingThousand", TaiwaneseCountingThousand),
        ("taiwaneseDigital", TaiwaneseDigital),
        ("chineseCounting", ChineseCounting),
        ("chineseLegalSimplified", ChineseLegalSimplified),
        ("chineseCountingThousand", ChineseCountingThousand),
        ("koreanDigital", KoreanDigital),
        ("koreanCounting", KoreanCounting),
        ("koreanLegal", KoreanLegal),
        ("koreanDigital2", KoreanDigital2),
        ("vietnameseCounting", VietnameseCounting),
        ("russianLower", RussianLower),
        ("russianUpper", RussianUpper),
        ("none", None),
        ("numberInDash", NumberInDash),
        ("hebrew1", Hebrew1),
        ("hebrew2", Hebrew2),
        ("arabicAlpha", ArabicAlpha),
        ("arabicAbjad", ArabicAbjad),
        ("hindiVowels", HindiVowels),
        ("hindiConsonants", HindiConsonants),
        ("hindiNumbers", HindiNumbers),
        ("hindiCounting", HindiCounting),
        ("thaiLetters", ThaiLetters),
        ("thaiNumbers", ThaiNumbers),
        ("thaiCounting", ThaiCounting),
        ("bahtText", BahtText),
        ("dollarText", DollarText),
        ("custom", Custom),
    ]
};

/// ST_Jc's values a level's label takes.
const JCS: [(&str, ListJc); 12] = [
    ("left", ListJc::Left),
    ("center", ListJc::Center),
    ("right", ListJc::Right),
    ("start", ListJc::Start),
    ("end", ListJc::End),
    ("both", ListJc::Both),
    ("mediumKashida", ListJc::MediumKashida),
    ("distribute", ListJc::Distribute),
    ("numTab", ListJc::NumTab),
    ("highKashida", ListJc::HighKashida),
    ("lowKashida", ListJc::LowKashida),
    ("thaiDistribute", ListJc::ThaiDistribute),
];

/// ST_LevelSuffix's values.
const SUFFIXES: [(&str, ListSuffix); 3] = [("tab", ListSuffix::Tab), ("space", ListSuffix::Space), ("nothing", ListSuffix::Nothing)];

/// ST_MultiLevelType's values.
const MULTI: [(&str, MultiLevelType); 3] = [
    ("singleLevel", MultiLevelType::SingleLevel),
    ("multilevel", MultiLevelType::Multilevel),
    ("hybridMultilevel", MultiLevelType::HybridMultilevel),
];

/// The value `v` names among `table`'s, else `v` as written.
fn literal<T: Copy>(v: &str, table: &[(&str, T)]) -> Literal<T> {
    table.iter().find(|(n, _)| *n == v).map_or_else(|| Literal::Other(v.into()), |(_, t)| Literal::Known(*t))
}

/// The name `l` is written as.
fn literal_name<'a, T: Copy + PartialEq>(l: &'a Literal<T>, table: &[(&'static str, T)]) -> &'a str {
    match l {
        Literal::Known(v) => table.iter().find(|(_, t)| t == v).map_or("", |(n, _)| *n),
        Literal::Other(s) => s,
    }
}

/// The name a level's number format is written as; `decimal`, the reader's, when it sets none.
pub fn format_name(f: Option<&Literal<NumberFormat>>) -> &str {
    f.map_or("decimal", |f| literal_name(f, &FORMATS))
}

/// The name a level's label alignment is written as; `left`, the reader's, when it sets none.
pub fn jc_name(j: Option<&Literal<ListJc>>) -> &str {
    j.map_or("left", |j| literal_name(j, &JCS))
}

/// The name what follows a level's label is written as; `tab`, the reader's, when it sets none.
pub fn suffix_name(v: Option<&Literal<ListSuffix>>) -> &str {
    v.map_or("tab", |v| literal_name(v, &SUFFIXES))
}

/// A legacy level's space and indent (points), as the reader gives them: when its placement is on.
pub fn legacy_of(l: &ListLevel) -> Option<(f64, f64)> {
    l.legacy.filter(|g| g.on == Some(true)).map(|g| (g.space.unwrap_or(0.0), g.indent.unwrap_or(0.0)))
}

/// An ST_LongHexNumber.
fn hex(v: &str) -> Option<u32> {
    u32::from_str_radix(v.trim(), 16).ok()
}

fn on_off_val(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

fn w_name(local: &str) -> QName {
    QName { ns: W.into(), local: local.into() }
}

/// An element's rank among `order`'s names.
fn rank(order: &[&str], e: Node) -> Option<usize> {
    (e.tag_name().namespace() == Some(W)).then(|| order.iter().position(|k| *k == e.tag_name().name())).flatten()
}

fn has_content(e: Node) -> bool {
    e.children().any(|c| c.is_element() || c.text().is_some_and(|t| !t.trim().is_empty()))
}

/// Where a child that doesn't count is written among an element's: in its name's place, before the one that counts
/// for a name in `last` (the last of which counts), else after it; one of no modelled name after all.
fn place(order: &[&str], last: &[&str], name: &QName) -> (usize, i64) {
    match (&*name.ns == W).then(|| order.iter().position(|k| **k == *name.local)).flatten() {
        Some(r) if last.contains(&order[r]) => (r, i64::MIN),
        Some(r) => (r, i64::MAX),
        None => (usize::MAX, i64::MAX),
    }
}

/// `f` in the order the serializer writes it, so that what is read again is in the same order.
fn settle(f: &mut ForeignProps, order: &[&str], last: &[&str]) {
    f.children.sort_by_key(|c| place(order, last, &c.name));
    f.kept.sort_by_key(|c| place(order, last, &c.name));
    let at = |a: &crate::format::ForeignAttr| {
        let on = a.on.as_ref().map_or(0, |q| 1 + order.iter().position(|k| **k == *q.local).unwrap_or(order.len()));
        let tied = a.stands_for.is_some() || a.beside.is_some();
        (on, WRITTEN.iter().position(|n| tied && *n == &*a.name.local).unwrap_or(WRITTEN.len()))
    };
    f.attributes.sort_by_key(at);
}

fn finish(f: ForeignProps) -> Option<Rc<ForeignProps>> {
    (f != ForeignProps::default()).then(|| Rc::new(f))
}

/// What reading kept child `c` alone gives.
fn read_kept<T>(c: &ForeignChild, f: impl FnOnce(Node) -> T) -> Option<T> {
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
    doc.root_element().first_element_child().map(f)
}

/// The child kept whole beside the typed value of modelled child `key`.
fn kept<'f>(foreign: Option<&'f ForeignProps>, key: &str) -> Option<&'f ForeignChild> {
    foreign.and_then(|f| f.kept.iter().find(|c| &*c.name.ns == W && &*c.name.local == key))
}

/// The foreign children of `foreign` whose place is `at` (a rank of `order`, or `None` for past them all).
fn foreign_at(out: &mut Out, foreign: Option<&ForeignProps>, order: &[&str], last: &[&str], at: Option<usize>, before: bool) {
    let Some(f) = foreign else { return };
    for c in &f.children {
        let (r, sub) = place(order, last, &c.name);
        let here = match at {
            Some(k) => r == k && (sub == i64::MIN) == before,
            None => r == usize::MAX,
        };
        if here {
            out.payload(c);
        }
    }
}

/// The values `emit` writes, and whether it writes any.
fn values(emit: &dyn Fn(&mut dyn Sink) -> bool) -> (bool, Values) {
    let mut v = Values(Vec::new());
    (emit(&mut v), v)
}

/// A modelled child with attributes only, written from its typed value by `emit` (false: nothing to write), or as kept
/// while reading it alone (`read`) gives the value it has.
fn simple_child(
    out: &mut Out,
    foreign: Option<&ForeignProps>,
    key: &'static str,
    emit: &dyn Fn(&mut dyn Sink) -> bool,
    read: &dyn Fn(Node) -> (bool, Values),
) {
    if let Some(c) = kept(foreign, key)
        && read_kept(c, read).is_some_and(|was| was == values(emit))
    {
        out.payload(c);
        return;
    }
    let at = out.s.len();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let prefix = out.prefix(W);
    let _ = write!(out.s, "<{prefix}{key}");
    let mut present = false;
    let written = out.attrs(W, attrs, Some((W, key)), |s| present = emit(s));
    if !present {
        out.s.truncate(at);
        return;
    }
    out.free_attrs(attrs, Some((W, key)), &written);
    out.s.push_str("/>");
}

// Levels

/// A `w:lvl` as a typed level; `ilvl` is the level it is read as, which its `w:ilvl` stands for.
pub fn parse_level(l: Node, ilvl: i64) -> ListLevel {
    let mut v =
        ListLevel { template: attr(l, "tplc").and_then(hex), tentative: attr(l, "tentative").map(|t| on_off_str(Some(t))), ..ListLevel::default() };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(l, &mut scratch);
    level_attrs(&v, ilvl, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let mut seen = [false; LEVEL.len()];
    for e in l.children().filter(|c| c.is_element()) {
        let Some(k) = rank(&LEVEL, e).filter(|k| !seen[*k]) else {
            f.children.push(foreign_child(e));
            continue;
        };
        seen[k] = true;
        let key = LEVEL[k];
        match key {
            "pPr" => {
                v.para = para_props::parse(e);
                let owned = e.children().any(|c| rank(&PPR_OWNED, c).is_some());
                if owned || v.para == ParaProps::default() {
                    f.kept.push(foreign_child(e));
                }
            }
            "rPr" => {
                let (c, rev) = char_props::parse(e);
                if !rev.is_empty() || c == CharProps::default() {
                    f.kept.push(foreign_child(e));
                }
                v.char = c;
            }
            _ => {
                read_level_child(&mut v, e, key);
                let mut cmp = Compare::new(e, &mut scratch);
                if !level_child(&v, key, &mut cmp) {
                    f.children.push(foreign_child(e));
                    continue;
                }
                cmp.finish(Some(w_name(key)), &mut f.attributes);
                if has_content(e) {
                    f.kept.push(foreign_child(e));
                }
            }
        }
    }
    settle(&mut f, &LEVEL, &[]);
    v.foreign = finish(f);
    v
}

/// A level's child with attributes only into `v`, as the XML path reads it.
fn read_level_child(v: &mut ListLevel, e: Node, key: &str) {
    let text = |e: Node| val(e).map(Rc::from);
    match key {
        "start" => v.start = int(val(e)),
        "numFmt" => {
            v.format = val(e).map(|s| literal(s, &FORMATS));
            v.format_code = attr(e, "format").map(Rc::from);
        }
        "lvlRestart" => v.restart = int(val(e)),
        "pStyle" => v.style = text(e),
        "isLgl" => v.legal = Some(on_off(e)),
        "suff" => v.suffix = val(e).map(|s| literal(s, &SUFFIXES)),
        "lvlText" => v.text = text(e),
        "lvlPicBulletId" => v.pic_bullet = int(val(e)),
        "legacy" => {
            v.legacy = Some(LegacyLabel {
                on: attr(e, "legacy").map(|s| on_off_str(Some(s))),
                space: attr_twips(e, "legacySpace"),
                indent: attr_twips(e, "legacyIndent"),
            })
        }
        "lvlJc" => v.jc = val(e).map(|s| literal(s, &JCS)),
        _ => {}
    }
}

fn level_attrs(v: &ListLevel, ilvl: i64, s: &mut dyn Sink) {
    s.attr(W, "ilvl", &ilvl);
    if let Some(t) = v.template {
        s.attr(W, "tplc", &format_args!("{t:08X}"));
    }
    if let Some(t) = v.tentative {
        s.attr(W, "tentative", &on_off_val(t));
    }
}

/// Writes a level's child `key` with attributes only to `s`; false when `v` doesn't set it.
fn level_child(v: &ListLevel, key: &str, s: &mut dyn Sink) -> bool {
    let val = |s: &mut dyn Sink, x: &dyn std::fmt::Display| s.attr(W, "val", x);
    match key {
        "start" => v.start.map(|x| val(s, &x)).is_some(),
        "numFmt" => {
            if let Some(f) = &v.format {
                val(s, &literal_name(f, &FORMATS));
            }
            if let Some(c) = &v.format_code {
                s.attr(W, "format", &&**c);
            }
            v.format.is_some() || v.format_code.is_some()
        }
        "lvlRestart" => v.restart.map(|x| val(s, &x)).is_some(),
        "pStyle" => v.style.as_deref().map(|x| val(s, &x)).is_some(),
        "isLgl" => match v.legal {
            Some(true) => {
                s.implied("val", "1");
                true
            }
            Some(false) => {
                val(s, &"0");
                true
            }
            None => false,
        },
        "suff" => v.suffix.as_ref().map(|x| val(s, &literal_name(x, &SUFFIXES))).is_some(),
        "lvlText" => v.text.as_deref().map(|x| val(s, &x)).is_some(),
        "lvlPicBulletId" => v.pic_bullet.map(|x| val(s, &x)).is_some(),
        "legacy" => {
            let Some(g) = v.legacy else { return false };
            if let Some(on) = g.on {
                s.attr(W, "legacy", &on_off_val(on));
            }
            if let Some(x) = g.space {
                s.attr(W, "legacySpace", &Twips(x));
            }
            if let Some(x) = g.indent {
                s.attr(W, "legacyIndent", &Twips(x));
            }
            true
        }
        "lvlJc" => v.jc.as_ref().map(|x| val(s, &literal_name(x, &JCS))).is_some(),
        _ => false,
    }
}

/// The children other nodes own of a kept `w:pPr`, as written.
fn kept_owned(c: Option<&ForeignChild>) -> [Option<String>; 3] {
    c.and_then(|c| read_kept(c, |e| PPR_OWNED.map(|n| child(e, n).map(|x| e.document().input_text()[x.range()].to_string())))).unwrap_or_default()
}

/// Modelled child `key` of a level.
fn level_child_xml(out: &mut Out, v: &ListLevel, key: &'static str, colours: &ThemeColours, cx: &Cx) {
    let foreign = v.foreign.as_deref();
    match key {
        "pPr" => {
            let c = kept(foreign, key);
            if let Some(c) = c.filter(|c| read_kept(c, para_props::parse).as_ref() == Some(&v.para)) {
                out.payload(c);
                return;
            }
            let owned = kept_owned(c);
            if v.para != ParaProps::default() || owned.iter().any(Option::is_some) {
                let [mark, section, change] = owned.each_ref().map(Option::as_deref);
                out.s.push_str(&para_props::serialize(&v.para, Owned { mark, section, change }, colours, cx));
            }
        }
        "rPr" => {
            let c = kept(foreign, key);
            let read = c.and_then(|c| read_kept(c, char_props::parse));
            if let Some(c) = c.filter(|_| read.as_ref().is_some_and(|r| r.0 == v.char)) {
                out.payload(c);
                return;
            }
            let rev = read.map(|r| r.1).unwrap_or_default();
            if v.char != CharProps::default() || !rev.is_empty() {
                out.s.push_str(&char_props::serialize(&v.char, &rev, colours, cx));
            }
        }
        _ => {
            let read = |e: Node| {
                let mut w = ListLevel::default();
                read_level_child(&mut w, e, key);
                values(&|s| level_child(&w, key, s))
            };
            simple_child(out, foreign, key, &|s| level_child(v, key, s), &read);
        }
    }
}

/// `v` as a `w:lvl` of level `ilvl` in the part `cx` describes; a theme colour's value is written as `colours`
/// resolve it.
pub fn serialize_level(v: &ListLevel, ilvl: i64, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = v.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}lvl{decls}");
    let written = out.attrs(W, attrs, None, |s| level_attrs(v, ilvl, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    for (k, &key) in LEVEL.iter().enumerate() {
        level_child_xml(&mut out, v, key, colours, cx);
        foreign_at(&mut out, foreign, &LEVEL, &[], Some(k), false);
    }
    foreign_at(&mut out, foreign, &LEVEL, &[], None, false);
    let _ = write!(out.s, "</{w}lvl>");
    out.s
}

// Definitions

/// The level each counting `w:lvl` of a `w:abstractNum` is: the last of each `w:ilvl` (missing or not a number: 0)
/// from 0 to 8.
fn counting_levels(a: Node) -> [Option<roxmltree::NodeId>; 9] {
    let mut counts = [None; 9];
    for l in children(a, "lvl") {
        let k = int(attr(l, "ilvl")).unwrap_or(0);
        if (0..9).contains(&k) {
            counts[k as usize] = Some(l.id());
        }
    }
    counts
}

/// The level a `w:lvl` of a `w:abstractNum` is read as, when it counts.
fn level_of(l: Node, counts: &[Option<roxmltree::NodeId>; 9]) -> Option<usize> {
    let k = int(attr(l, "ilvl")).unwrap_or(0);
    (0..9).contains(&k).then_some(k as usize).filter(|k| counts[*k] == Some(l.id()))
}

/// A `w:abstractNum` as a typed definition; `None` without a `w:abstractNumId`.
pub fn parse_def(a: Node) -> Option<ListDef> {
    let id = int(attr(a, "abstractNumId"))?;
    let mut d = ListDef { id: ListDefId(id), ..ListDef::default() };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(a, &mut scratch);
    def_attrs(&d, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let counts = counting_levels(a);
    let mut seen = [false; DEF.len()];
    for e in a.children().filter(|c| c.is_element()) {
        let Some(k) = rank(&DEF, e) else {
            f.children.push(foreign_child(e));
            continue;
        };
        if DEF[k] == "lvl" {
            match level_of(e, &counts) {
                Some(il) => d.levels[il] = Some(Rc::new(parse_level(e, il as i64))),
                None => f.children.push(foreign_child(e)),
            }
            continue;
        }
        if seen[k] {
            f.children.push(foreign_child(e));
            continue;
        }
        seen[k] = true;
        let key = DEF[k];
        read_def_child(&mut d, e, key);
        let mut cmp = Compare::new(e, &mut scratch);
        if !def_child(&d, key, &mut cmp) {
            f.children.push(foreign_child(e));
            continue;
        }
        cmp.finish(Some(w_name(key)), &mut f.attributes);
        if has_content(e) {
            f.kept.push(foreign_child(e));
        }
    }
    settle(&mut f, &DEF, &["lvl"]);
    d.foreign = finish(f);
    Some(d)
}

fn def_attrs(d: &ListDef, s: &mut dyn Sink) {
    s.attr(W, "abstractNumId", &d.id.0);
}

fn read_def_child(d: &mut ListDef, e: Node, key: &str) {
    let text = |e: Node| val(e).map(Rc::from);
    match key {
        "nsid" => d.nsid = val(e).and_then(hex),
        "multiLevelType" => d.multi_level_type = val(e).map(|s| literal(s, &MULTI)),
        "tmpl" => d.template = val(e).and_then(hex),
        "name" => d.name = text(e),
        "styleLink" => d.style_link = text(e),
        "numStyleLink" => d.num_style_link = text(e),
        _ => {}
    }
}

fn def_child(d: &ListDef, key: &str, s: &mut dyn Sink) -> bool {
    let val = |s: &mut dyn Sink, x: &dyn std::fmt::Display| s.attr(W, "val", x);
    match key {
        "nsid" => d.nsid.map(|x| val(s, &format_args!("{x:08X}"))).is_some(),
        "multiLevelType" => d.multi_level_type.as_ref().map(|x| val(s, &literal_name(x, &MULTI))).is_some(),
        "tmpl" => d.template.map(|x| val(s, &format_args!("{x:08X}"))).is_some(),
        "name" => d.name.as_deref().map(|x| val(s, &x)).is_some(),
        "styleLink" => d.style_link.as_deref().map(|x| val(s, &x)).is_some(),
        "numStyleLink" => d.num_style_link.as_deref().map(|x| val(s, &x)).is_some(),
        _ => false,
    }
}

fn def_child_xml(out: &mut Out, d: &ListDef, key: &'static str) {
    let read = |e: Node| {
        let mut w = ListDef::default();
        read_def_child(&mut w, e, key);
        values(&|s| def_child(&w, key, s))
    };
    simple_child(out, d.foreign.as_deref(), key, &|s| def_child(d, key, s), &read);
}

/// `d` as a `w:abstractNum` in the part `cx` describes; a theme colour's value is written as `colours` resolve it.
pub fn serialize_def(d: &ListDef, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = d.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}abstractNum{decls}");
    let written = out.attrs(W, attrs, None, |s| def_attrs(d, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    for (k, &key) in DEF.iter().enumerate() {
        if key == "lvl" {
            foreign_at(&mut out, foreign, &DEF, &["lvl"], Some(k), true);
            for (il, l) in d.levels.iter().enumerate() {
                if let Some(l) = l {
                    out.s.push_str(&serialize_level(l, il as i64, colours, cx));
                }
            }
        } else {
            def_child_xml(&mut out, d, key);
        }
        foreign_at(&mut out, foreign, &DEF, &["lvl"], Some(k), false);
    }
    foreign_at(&mut out, foreign, &DEF, &["lvl"], None, false);
    let _ = write!(out.s, "</{w}abstractNum>");
    out.s
}

// Lists

/// Each level's overrides of a `w:num`: the last `w:lvlOverride` of the level, the start of the last that sets one and
/// the level of the last that defines one.
struct Overrides<'a, 'i> {
    by_level: BTreeMap<i64, (roxmltree::NodeId, Option<i64>, Option<Node<'a, 'i>>)>,
}

fn overrides<'a, 'i>(m: Node<'a, 'i>) -> Overrides<'a, 'i> {
    let mut by_level: BTreeMap<i64, (roxmltree::NodeId, Option<i64>, Option<Node>)> = BTreeMap::new();
    for o in children(m, "lvlOverride") {
        let Some(il) = int(attr(o, "ilvl")) else { continue };
        let g = by_level.entry(il).or_insert((o.id(), None, None));
        g.0 = o.id();
        if let Some(s) = child(o, "startOverride").and_then(|s| int(val(s))) {
            g.1 = Some(s);
        }
        if let Some(l) = child(o, "lvl") {
            g.2 = Some(l);
        }
    }
    Overrides { by_level }
}

impl Overrides<'_, '_> {
    /// The level `o` overrides, when it is the `w:lvlOverride` that counts for it.
    fn counts(&self, o: Node) -> Option<i64> {
        let il = int(attr(o, "ilvl"))?;
        (self.by_level.get(&il)?.0 == o.id()).then_some(il)
    }
}

fn empty_list() -> List {
    List { id: ListId(0), def: None, overrides: Vec::new(), origin: Origin::Document, foreign: None }
}

/// A `w:num` as a typed list; `None` without a `w:numId`.
pub fn parse_list(m: Node) -> Option<List> {
    let id = int(attr(m, "numId"))?;
    let mut n = List { id: ListId(id), ..empty_list() };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(m, &mut scratch);
    list_attrs(&n, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let groups = overrides(m);
    let mut seen = false;
    for e in m.children().filter(|c| c.is_element()) {
        match rank(&LIST, e).map(|k| LIST[k]) {
            Some("abstractNumId") if !seen => {
                seen = true;
                n.def = int(val(e)).map(ListDefId);
                let mut cmp = Compare::new(e, &mut scratch);
                if !list_child(&n, &mut cmp) {
                    f.children.push(foreign_child(e));
                    continue;
                }
                cmp.finish(Some(w_name("abstractNumId")), &mut f.attributes);
                if has_content(e) {
                    f.kept.push(foreign_child(e));
                }
            }
            Some("lvlOverride") => match groups.counts(e) {
                Some(il) => {
                    let (_, start, lvl) = groups.by_level[&il];
                    n.overrides.push(parse_override(e, il, start, lvl));
                }
                None => f.children.push(foreign_child(e)),
            },
            _ => f.children.push(foreign_child(e)),
        }
    }
    n.overrides.sort_by_key(|o| o.level);
    settle(&mut f, &LIST, &["lvlOverride"]);
    n.foreign = finish(f);
    Some(n)
}

/// The `w:lvlOverride` that counts for level `il`, with the start and the level that count for it.
fn parse_override(o: Node, il: i64, start: Option<i64>, lvl: Option<Node>) -> LevelOverride {
    let mut v = LevelOverride { level: il, start, def: lvl.map(|l| Rc::new(parse_level(l, il))), foreign: None };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(o, &mut scratch);
    override_attrs(&v, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    let (mut seen_start, mut seen_lvl) = (false, false);
    for e in o.children().filter(|c| c.is_element()) {
        match rank(&OVERRIDE, e).map(|k| OVERRIDE[k]) {
            Some("startOverride") if !seen_start => {
                seen_start = true;
                if int(val(e)).is_none() {
                    f.children.push(foreign_child(e));
                    continue;
                }
                let mut cmp = Compare::new(e, &mut scratch);
                override_child(&v, &mut cmp);
                cmp.finish(Some(w_name("startOverride")), &mut f.attributes);
                if has_content(e) {
                    f.kept.push(foreign_child(e));
                }
            }
            Some("lvl") if !seen_lvl && lvl.is_some_and(|l| l.id() == e.id()) => seen_lvl = true,
            _ => f.children.push(foreign_child(e)),
        }
    }
    settle(&mut f, &OVERRIDE, &[]);
    v.foreign = finish(f);
    v
}

fn list_attrs(n: &List, s: &mut dyn Sink) {
    s.attr(W, "numId", &n.id.0);
}

fn list_child(n: &List, s: &mut dyn Sink) -> bool {
    n.def.map(|d| s.attr(W, "val", &d.0)).is_some()
}

fn override_attrs(o: &LevelOverride, s: &mut dyn Sink) {
    s.attr(W, "ilvl", &o.level);
}

fn override_child(o: &LevelOverride, s: &mut dyn Sink) -> bool {
    o.start.map(|v| s.attr(W, "val", &v)).is_some()
}

fn list_child_xml(out: &mut Out, n: &List) {
    let read = |e: Node| {
        let w = List { def: int(val(e)).map(ListDefId), ..empty_list() };
        values(&|s| list_child(&w, s))
    };
    simple_child(out, n.foreign.as_deref(), "abstractNumId", &|s| list_child(n, s), &read);
}

fn override_child_xml(out: &mut Out, o: &LevelOverride) {
    let read = |e: Node| {
        let w = LevelOverride { start: int(val(e)), ..LevelOverride::default() };
        values(&|s| override_child(&w, s))
    };
    simple_child(out, o.foreign.as_deref(), "startOverride", &|s| override_child(o, s), &read);
}

/// `o` as a `w:lvlOverride` in the part `cx` describes.
pub fn serialize_override(o: &LevelOverride, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = o.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}lvlOverride{decls}");
    let written = out.attrs(W, attrs, None, |s| override_attrs(o, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    override_child_xml(&mut out, o);
    foreign_at(&mut out, foreign, &OVERRIDE, &[], Some(0), false);
    if let Some(l) = &o.def {
        out.s.push_str(&serialize_level(l, o.level, colours, cx));
    }
    foreign_at(&mut out, foreign, &OVERRIDE, &[], Some(1), false);
    foreign_at(&mut out, foreign, &OVERRIDE, &[], None, false);
    let _ = write!(out.s, "</{w}lvlOverride>");
    out.s
}

/// `n` as a `w:num` in the part `cx` describes; a theme colour's value is written as `colours` resolve it.
pub fn serialize_list(n: &List, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = n.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}num{decls}");
    let written = out.attrs(W, attrs, None, |s| list_attrs(n, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    list_child_xml(&mut out, n);
    foreign_at(&mut out, foreign, &LIST, &["lvlOverride"], Some(0), false);
    foreign_at(&mut out, foreign, &LIST, &["lvlOverride"], Some(1), true);
    for o in &n.overrides {
        out.s.push_str(&serialize_override(o, colours, cx));
    }
    foreign_at(&mut out, foreign, &LIST, &["lvlOverride"], Some(1), false);
    foreign_at(&mut out, foreign, &LIST, &["lvlOverride"], None, false);
    let _ = write!(out.s, "</{w}num>");
    out.s
}

// Picture bullets

/// A `w:numPicBullet` as a typed picture bullet; `None` without a `w:numPicBulletId`.
pub fn parse_picture(p: Node) -> Option<PictureBullet> {
    let id = int(attr(p, "numPicBulletId"))?;
    let rel = p
        .descendants()
        .find_map(|d| d.attributes().find(|a| matches!(a.name(), "id" | "embed") && a.namespace() == Some(R)).map(|a| Rc::from(a.value())));
    let mut b = PictureBullet { id, rel, image: None, foreign: None };
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    let mut cmp = Compare::new(p, &mut scratch);
    picture_attrs(&b, &mut cmp);
    cmp.finish(None, &mut f.attributes);
    f.children = p.children().filter(|c| c.is_element()).map(foreign_child).collect();
    b.foreign = finish(f);
    Some(b)
}

fn picture_attrs(b: &PictureBullet, s: &mut dyn Sink) {
    s.attr(W, "numPicBulletId", &b.id);
}

/// `b` as a `w:numPicBullet` in the part `cx` describes.
pub fn serialize_picture(b: &PictureBullet, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let foreign = b.foreign.as_deref();
    let attrs = foreign.map_or(&[][..], |f| &f.attributes);
    let mut decls = String::new();
    out.declarations(foreign, &mut decls);
    let w = cx.w.clone();
    let _ = write!(out.s, "<{w}numPicBullet{decls}");
    let written = out.attrs(W, attrs, None, |s| picture_attrs(b, s));
    out.free_attrs(attrs, None, &written);
    out.s.push('>');
    for c in foreign.map_or(&[][..], |f| &f.children) {
        out.payload(c);
    }
    let _ = write!(out.s, "</{w}numPicBullet>");
    out.s
}

// Written over the element as read

/// Where a child of an element as read goes: its place among the element's children, and whether it counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kid {
    at: (usize, i64),
    counts: bool,
}

/// An element as written: its start and end tags, its modelled attributes' names as written, and its children.
struct Read {
    open: String,
    close: String,
    names: Vec<(&'static str, String)>,
    kids: Vec<(Kid, String)>,
}

impl Read {
    fn of(xml: &str, cx: &Cx, kids: for<'a, 'i> fn(Node<'a, 'i>) -> Vec<(Node<'a, 'i>, Kid)>) -> Option<Read> {
        cx.parse(xml, |text, el| {
            let tag_end = el.first_child().map_or(el.range().end, |c| c.range().start);
            let tag = &text[el.range().start..tag_end];
            let open = match tag.strip_suffix("/>") {
                Some(t) => format!("{}>", t.trim_end()),
                None => tag.to_string(),
            };
            let qname: String = tag[1..].chars().take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/').collect();
            let names = WRITTEN
                .iter()
                .filter_map(|n| {
                    let a = el.attributes().nth(read_attr(el, W, n)?)?;
                    Some((*n, text[a.range_qname()].to_string()))
                })
                .collect();
            let kids = kids(el).into_iter().map(|(n, k)| (k, text[n.range()].to_string())).collect();
            Read { open, close: format!("</{qname}>"), names, kids }
        })
    }

    /// The text of the child that counts at `at`.
    fn kid(&self, at: (usize, i64)) -> Option<&str> {
        self.kids.iter().find(|(k, _)| k.counts && k.at == at).map(|(_, t)| t.as_str())
    }

    /// The start tag with the modelled attributes `now` gives where they differ from what `was` gives.
    fn open_over(&self, was: &dyn Fn(&mut dyn Sink), now: &dyn Fn(&mut dyn Sink), cx: &Cx) -> String {
        let (mut a, mut b) = (Values(Vec::new()), Values(Vec::new()));
        was(&mut a);
        now(&mut b);
        let mut open = self.open.clone();
        for n in WRITTEN {
            let (x, y) = (a.get(n), b.get(n));
            if x == y {
                continue;
            }
            let name = self.names.iter().find(|(k, _)| *k == n).map(|(_, q)| q.clone());
            open = match (y, name) {
                (Some(v), Some(q)) => with_attr(&open, &q, v),
                (Some(v), None) => with_attr(&open, &format!("{}{n}", cx.w), v),
                (None, Some(q)) => strip(&open, &q),
                (None, None) => open,
            };
        }
        open
    }
}

/// The modelled attributes a writer gives, as values; one left out as the value it reads as.
#[derive(Debug, PartialEq)]
struct Values(Vec<(&'static str, String)>);

impl Values {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
    }
}

impl Sink for Values {
    fn attr(&mut self, _: &'static str, local: &'static str, v: &dyn std::fmt::Display) {
        self.0.push((local, v.to_string()));
    }

    fn implied(&mut self, local: &'static str, means: &'static str) {
        self.0.push((local, means.to_string()));
    }

    fn decided(&mut self, _: &'static str, _: &'static str, _: Option<super::model::ThemeColourRef>) {}
}

/// What is written now of a modelled child: its place, its text (`None`: nothing), whether it differs from the one read.
struct Now {
    at: (usize, i64),
    text: Option<String>,
    changed: bool,
}

/// `read` with each modelled child `now` changes written in its place (or removed), a new one in schema order, the
/// children that don't count kept while `foreign` still has them and those it adds in their places.
fn over(read: &Read, open: String, now: &[Now], foreign: Option<&ForeignProps>, place: impl Fn(&ForeignChild) -> (usize, i64)) -> String {
    let mut kids: Vec<((usize, i64), Cow<str>)> = Vec::new();
    let mut pending: Vec<&ForeignChild> = foreign.map_or_else(Vec::new, |f| f.children.iter().collect());
    for (kid, text) in &read.kids {
        if !kid.counts {
            if let Some(i) = pending.iter().position(|c| *c.payload == **text) {
                pending.remove(i);
                kids.push((kid.at, Cow::Borrowed(text)));
            }
            continue;
        }
        match now.iter().find(|n| n.at == kid.at) {
            Some(n) if n.changed => {
                if let Some(t) = &n.text {
                    kids.push((kid.at, Cow::Borrowed(t)));
                }
            }
            _ => kids.push((kid.at, Cow::Borrowed(text))),
        }
    }
    fn insert(kids: &mut Vec<((usize, i64), Cow<'_, str>)>, at: (usize, i64), text: String) {
        let k = kids.iter().rposition(|(a, _)| *a <= at).map_or(0, |k| k + 1);
        kids.insert(k, (at, Cow::Owned(text)));
    }
    for n in now {
        if read.kid(n.at).is_none()
            && let Some(t) = &n.text
        {
            insert(&mut kids, n.at, t.clone());
        }
    }
    for c in pending {
        insert(&mut kids, place(c), c.payload.to_string());
    }
    let mut s = open;
    for (_, t) in &kids {
        s.push_str(t);
    }
    s.push_str(&read.close);
    s
}

/// The children of a `w:lvl` with their places.
fn level_kids<'a, 'i>(l: Node<'a, 'i>) -> Vec<(Node<'a, 'i>, Kid)> {
    let mut seen = [false; LEVEL.len()];
    l.children()
        .filter(|c| c.is_element())
        .map(|e| match rank(&LEVEL, e) {
            Some(k) if !seen[k] => {
                seen[k] = true;
                (e, Kid { at: (k, 0), counts: true })
            }
            Some(k) => (e, Kid { at: (k, i64::MAX), counts: false }),
            None => (e, Kid { at: (usize::MAX, i64::MAX), counts: false }),
        })
        .collect()
}

/// `now`, level `ilvl`, as a `w:lvl` written as an edit of `xml` (the `w:lvl` level `read` was read from) would: each
/// child the change leaves alone, and each `set` doesn't set, as written there.
fn level_over(read: &ListLevel, now: &ListLevel, ilvl: i64, xml: &str, set: &[LevelProp], colours: &ThemeColours, cx: &Cx) -> String {
    let Some(was) = Read::of(xml, cx, level_kids) else { return serialize_level(now, ilvl, colours, cx) };
    let sets = |key: &str| {
        set.iter().any(|p| {
            let k = match p {
                LevelProp::Whole | LevelProp::Para(_) | LevelProp::Char(_) | LevelProp::Template | LevelProp::Tentative => return false,
                LevelProp::Start => "start",
                LevelProp::Format => "numFmt",
                LevelProp::Text => "lvlText",
                LevelProp::Jc => "lvlJc",
                LevelProp::Suffix => "suff",
                LevelProp::Legal => "isLgl",
                LevelProp::Restart => "lvlRestart",
                LevelProp::Style => "pStyle",
                LevelProp::PicBullet => "lvlPicBulletId",
                LevelProp::Legacy => "legacy",
            };
            k == key
        })
    };
    let para_sets: Vec<ParaSet> = set.iter().filter_map(|p| if let LevelProp::Para(s) = p { Some(*s) } else { None }).collect();
    let char_sets: Vec<Set> = set.iter().filter_map(|p| if let LevelProp::Char(s) = p { Some(*s) } else { None }).collect();
    let mut kids = Vec::new();
    for (k, &key) in LEVEL.iter().enumerate() {
        let at = (k, 0);
        let origin = was.kid(at);
        let (changed, text) = match key {
            "pPr" => {
                let changed = read.para != now.para || !para_sets.is_empty();
                let origin = origin.map(|x| (&read.para, x));
                (changed, if changed { para_props::written_over(&now.para, colours, cx, origin, &para_sets) } else { None })
            }
            "rPr" => {
                let changed = read.char != now.char || !char_sets.is_empty();
                let origin = origin.map(|x| (&read.char, x));
                let text =
                    if changed { char_props::written_over(&now.char, &RprRevisions::default(), colours, cx, origin, &char_sets) } else { None };
                (changed, text)
            }
            _ => {
                let render = |v: &ListLevel| {
                    let mut out = Out::new(cx, colours);
                    level_child_xml(&mut out, v, key, colours, cx);
                    out.s
                };
                let (a, b) = (render(read), render(now));
                (a != b || sets(key), (!b.is_empty()).then_some(b))
            }
        };
        kids.push(Now { at, text, changed });
    }
    let open = was.open_over(&|s| level_attrs(read, ilvl, s), &|s| level_attrs(now, ilvl, s), cx);
    over(&was, open, &kids, now.foreign.as_deref(), |c| place(&LEVEL, &[], &c.name))
}

/// The children of a `w:abstractNum` with their places.
fn def_kids<'a, 'i>(a: Node<'a, 'i>) -> Vec<(Node<'a, 'i>, Kid)> {
    let counts = counting_levels(a);
    let mut seen = [false; DEF.len()];
    let lvl = DEF.len() - 1;
    a.children()
        .filter(|c| c.is_element())
        .map(|e| match rank(&DEF, e) {
            Some(k) if k == lvl => match level_of(e, &counts) {
                Some(il) => (e, Kid { at: (k, il as i64), counts: true }),
                None => (e, Kid { at: (k, i64::MIN), counts: false }),
            },
            Some(k) if !seen[k] => {
                seen[k] = true;
                (e, Kid { at: (k, 0), counts: true })
            }
            Some(k) => (e, Kid { at: (k, i64::MAX), counts: false }),
            None => (e, Kid { at: (usize::MAX, i64::MAX), counts: false }),
        })
        .collect()
}

/// `now` as a `w:abstractNum` written as an edit of `xml` (the one `read` was read from) would.
fn def_over(read: &ListDef, now: &ListDef, xml: &str, set: &[ListDefProp], colours: &ThemeColours, cx: &Cx) -> String {
    let Some(was) = Read::of(xml, cx, def_kids) else { return serialize_def(now, colours, cx) };
    let mut kids = Vec::new();
    for (k, &key) in DEF.iter().enumerate() {
        if key == "lvl" {
            for il in 0..9 {
                let at = (k, il as i64);
                let level_set: Vec<LevelProp> =
                    set.iter().filter_map(|p| if let ListDefProp::Level(i, l) = p { (*i as usize == il).then_some(*l) } else { None }).collect();
                let (a, b) = (&read.levels[il], &now.levels[il]);
                let same = match (a, b) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a == b,
                    (None, None) => true,
                    _ => false,
                };
                let changed = !same || !level_set.is_empty();
                let text = b.as_ref().filter(|_| changed).map(|b| match (a, was.kid(at)) {
                    (Some(a), Some(x)) if !level_set.contains(&LevelProp::Whole) => level_over(a, b, il as i64, x, &level_set, colours, cx),
                    _ => serialize_level(b, il as i64, colours, cx),
                });
                kids.push(Now { at, text, changed });
            }
            continue;
        }
        let prop = match key {
            "nsid" => ListDefProp::Nsid,
            "multiLevelType" => ListDefProp::MultiLevelType,
            "tmpl" => ListDefProp::Template,
            "name" => ListDefProp::Name,
            "styleLink" => ListDefProp::StyleLink,
            _ => ListDefProp::NumStyleLink,
        };
        let render = |d: &ListDef| {
            let mut out = Out::new(cx, colours);
            def_child_xml(&mut out, d, key);
            out.s
        };
        let (a, b) = (render(read), render(now));
        kids.push(Now { at: (k, 0), changed: a != b || set.contains(&prop), text: (!b.is_empty()).then_some(b) });
    }
    let open = was.open_over(&|s| def_attrs(read, s), &|s| def_attrs(now, s), cx);
    over(&was, open, &kids, now.foreign.as_deref(), |c| place(&DEF, &["lvl"], &c.name))
}

/// The children of a `w:num` with their places.
fn list_kids<'a, 'i>(m: Node<'a, 'i>) -> Vec<(Node<'a, 'i>, Kid)> {
    let groups = overrides(m);
    let mut seen = false;
    m.children()
        .filter(|c| c.is_element())
        .map(|e| match rank(&LIST, e) {
            Some(0) if !seen => {
                seen = true;
                (e, Kid { at: (0, 0), counts: true })
            }
            Some(0) => (e, Kid { at: (0, i64::MAX), counts: false }),
            Some(k) => match groups.counts(e) {
                Some(il) => (e, Kid { at: (k, il), counts: true }),
                None => (e, Kid { at: (k, i64::MIN), counts: false }),
            },
            None => (e, Kid { at: (usize::MAX, i64::MAX), counts: false }),
        })
        .collect()
}

/// The children of a `w:lvlOverride` with their places.
fn override_kids<'a, 'i>(o: Node<'a, 'i>) -> Vec<(Node<'a, 'i>, Kid)> {
    let (mut start, mut lvl) = (false, false);
    o.children()
        .filter(|c| c.is_element())
        .map(|e| match rank(&OVERRIDE, e) {
            Some(0) if !start && int(val(e)).is_some() => {
                start = true;
                (e, Kid { at: (0, 0), counts: true })
            }
            Some(1) if !lvl => {
                lvl = true;
                (e, Kid { at: (1, 0), counts: true })
            }
            Some(k) => {
                if k == 0 {
                    start = true;
                }
                (e, Kid { at: (k, i64::MAX), counts: false })
            }
            None => (e, Kid { at: (usize::MAX, i64::MAX), counts: false }),
        })
        .collect()
}

/// `now` as a `w:lvlOverride` written as an edit of `xml` (the one `read` was read from) would.
fn override_over(read: &LevelOverride, now: &LevelOverride, xml: &str, set: &[OverrideProp], colours: &ThemeColours, cx: &Cx) -> String {
    let Some(was) = Read::of(xml, cx, override_kids) else { return serialize_override(now, colours, cx) };
    let render = |o: &LevelOverride| {
        let mut out = Out::new(cx, colours);
        override_child_xml(&mut out, o);
        out.s
    };
    let (a, b) = (render(read), render(now));
    let start = Now { at: (0, 0), changed: a != b || set.contains(&OverrideProp::Start), text: (!b.is_empty()).then_some(b) };
    let level_set: Vec<LevelProp> = set.iter().filter_map(|p| if let OverrideProp::Level(l) = p { Some(*l) } else { None }).collect();
    let same = match (&read.def, &now.def) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a == b,
        (None, None) => true,
        _ => false,
    };
    let changed = !same || !level_set.is_empty();
    let text = now.def.as_ref().filter(|_| changed).map(|b| match (&read.def, was.kid((1, 0))) {
        (Some(a), Some(x)) if !level_set.contains(&LevelProp::Whole) => level_over(a, b, now.level, x, &level_set, colours, cx),
        _ => serialize_level(b, now.level, colours, cx),
    });
    let kids = [start, Now { at: (1, 0), text, changed }];
    let open = was.open_over(&|s| override_attrs(read, s), &|s| override_attrs(now, s), cx);
    over(&was, open, &kids, now.foreign.as_deref(), |c| place(&OVERRIDE, &[], &c.name))
}

/// `now` as a `w:num` written as an edit of `xml` (the one `read` was read from) would.
fn list_over(read: &List, now: &List, xml: &str, set: &[ListProp], colours: &ThemeColours, cx: &Cx) -> String {
    let Some(was) = Read::of(xml, cx, list_kids) else { return serialize_list(now, colours, cx) };
    let render = |n: &List| {
        let mut out = Out::new(cx, colours);
        list_child_xml(&mut out, n);
        out.s
    };
    let (a, b) = (render(read), render(now));
    let mut kids = vec![Now { at: (0, 0), changed: a != b || set.contains(&ListProp::Def), text: (!b.is_empty()).then_some(b) }];
    let mut levels: Vec<i64> = read.overrides.iter().chain(&now.overrides).map(|o| o.level).collect();
    levels.sort_unstable();
    levels.dedup();
    for il in levels {
        let o_set: Vec<OverrideProp> =
            set.iter().filter_map(|p| if let ListProp::Override(i, o) = p { (*i == il).then_some(*o) } else { None }).collect();
        let (x, y) = (read.override_of(il), now.override_of(il));
        let changed = x != y || !o_set.is_empty();
        let text = y.filter(|_| changed).map(|y| match (x, was.kid((1, il))) {
            (Some(x), Some(t)) if !o_set.contains(&OverrideProp::Whole) => override_over(x, y, t, &o_set, colours, cx),
            _ => serialize_override(y, colours, cx),
        });
        kids.push(Now { at: (1, il), text, changed });
    }
    let open = was.open_over(&|s| list_attrs(read, s), &|s| list_attrs(now, s), cx);
    over(&was, open, &kids, now.foreign.as_deref(), |c| place(&LIST, &["lvlOverride"], &c.name))
}

// Holders

/// A numbering part element's typed value, as its holder reads and writes it.
pub trait Element: Clone + std::fmt::Debug + PartialEq {
    /// What an edit sets, written again over the element as read.
    type Set: Copy + std::fmt::Debug + PartialEq;

    /// The element `el` as a typed value; `None` when it lacks its id.
    fn parse(el: Node) -> Option<Self>;

    /// The value as the element in the part `cx` describes; a theme colour's value is written as `colours` resolve it.
    fn serialize(&self, colours: &ThemeColours, cx: &Cx) -> String;

    /// The value written as an edit of `origin` (the element as read, and the value read from it) that set `set`
    /// would: what the edit leaves alone as written there.
    fn written_over(&self, colours: &ThemeColours, cx: &Cx, origin: (&Self, &str), set: &[Self::Set]) -> String;

    /// Whether the element as read from `self` stands for `o`.
    fn written_alike(&self, o: &Self) -> bool {
        self == o
    }
}

impl Element for ListDef {
    type Set = ListDefProp;

    fn parse(el: Node) -> Option<ListDef> {
        parse_def(el)
    }

    fn serialize(&self, colours: &ThemeColours, cx: &Cx) -> String {
        serialize_def(self, colours, cx)
    }

    fn written_over(&self, colours: &ThemeColours, cx: &Cx, (read, xml): (&ListDef, &str), set: &[ListDefProp]) -> String {
        def_over(read, self, xml, set, colours, cx)
    }

    fn written_alike(&self, o: &ListDef) -> bool {
        let (a, b) = (self, o);
        a.id == b.id
            && a.nsid == b.nsid
            && a.multi_level_type == b.multi_level_type
            && a.template == b.template
            && a.name == b.name
            && a.style_link == b.style_link
            && a.num_style_link == b.num_style_link
            && a.levels == b.levels
            && a.foreign == b.foreign
    }
}

impl Element for List {
    type Set = ListProp;

    fn parse(el: Node) -> Option<List> {
        parse_list(el)
    }

    fn serialize(&self, colours: &ThemeColours, cx: &Cx) -> String {
        serialize_list(self, colours, cx)
    }

    fn written_over(&self, colours: &ThemeColours, cx: &Cx, (read, xml): (&List, &str), set: &[ListProp]) -> String {
        list_over(read, self, xml, set, colours, cx)
    }

    fn written_alike(&self, o: &List) -> bool {
        self.id == o.id && self.def == o.def && self.overrides == o.overrides && self.foreign == o.foreign
    }
}

impl Element for PictureBullet {
    type Set = std::convert::Infallible;

    fn parse(el: Node) -> Option<PictureBullet> {
        parse_picture(el)
    }

    fn serialize(&self, colours: &ThemeColours, cx: &Cx) -> String {
        serialize_picture(self, colours, cx)
    }

    fn written_over(&self, colours: &ThemeColours, cx: &Cx, _: (&PictureBullet, &str), _: &[std::convert::Infallible]) -> String {
        serialize_picture(self, colours, cx)
    }

    fn written_alike(&self, o: &PictureBullet) -> bool {
        self.id == o.id && self.rel == o.rel && self.foreign == o.foreign
    }
}

/// A definition, list or picture bullet: typed, with the element it was read from (or last written as) beside it. The
/// written form stands for the value only while it is written alike; never part of equality.
#[derive(Clone, Debug)]
pub struct Direct<T: Element> {
    value: Rc<T>,
    origin: Option<(Rc<T>, Rc<str>)>,
    /// What edits since reading set, in order: written again over the element as read.
    set: Vec<T::Set>,
}

/// A `w:abstractNum`'s holder.
pub type ListDefDirect = Direct<ListDef>;

/// A `w:num`'s holder.
pub type ListDirect = Direct<List>;

/// A `w:numPicBullet`'s holder.
pub type PictureDirect = Direct<PictureBullet>;

impl<T: Element> Direct<T> {
    /// A value read from `xml`, the element it was parsed from.
    pub fn read(value: T, xml: Rc<str>) -> Direct<T> {
        let value = Rc::new(value);
        Direct { origin: Some((value.clone(), xml)), value, set: Vec::new() }
    }

    /// A typed value with no written form.
    pub fn typed(value: Rc<T>) -> Direct<T> {
        Direct { value, origin: None, set: Vec::new() }
    }

    /// The value `xml` (an element in the part `cx` describes) holds; `None` when it isn't one.
    pub fn from_xml(xml: &str, cx: &Cx) -> Option<Direct<T>> {
        let v = cx.parse(xml, |_, el| T::parse(el)).flatten()?;
        Some(Direct::read(v, xml.into()))
    }

    /// The typed value.
    pub fn get(&self) -> &Rc<T> {
        &self.value
    }

    /// The element it was read from (or last written as), whatever the value is now.
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        self.origin.as_ref().map(|(_, x)| x)
    }

    /// The element as read, while the value is written alike.
    pub fn as_read(&self) -> Option<&Rc<str>> {
        let (read, xml) = self.origin.as_ref()?;
        (Rc::ptr_eq(read, &self.value) || read.written_alike(&self.value)).then_some(xml)
    }

    /// The element to write in the part `cx` describes: as read while the value is written alike, else written over
    /// it (or from the value, with none), a theme colour's value as `colours` resolve it.
    pub fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Cow<'_, str> {
        if let Some(x) = self.as_read() {
            return Cow::Borrowed(x);
        }
        match &self.origin {
            Some((read, xml)) => Cow::Owned(self.value.written_over(colours, cx, (read, xml), &self.set)),
            None => Cow::Owned(self.value.serialize(colours, cx)),
        }
    }

    /// `value` in place of this one, set by an edit that set `set`, in order: the element it was read from kept to
    /// write it over, what was set written again.
    pub fn edited(&self, value: T, set: &[T::Set]) -> Direct<T> {
        let mut all = self.set.clone();
        all.extend_from_slice(set);
        Direct { value: Rc::new(value), origin: self.origin.clone(), set: all }
    }

    /// Authored equality: the typed values.
    pub fn authored_eq(&self, o: &Direct<T>) -> bool {
        Rc::ptr_eq(&self.value, &o.value) || self.value == o.value
    }
}

// The numbering part

/// A numbering part's definitions, lists and picture bullets, each with the element it was read from.
#[derive(Clone, Debug, Default)]
pub struct NumberingXml {
    /// The definitions, by id.
    pub defs: HashMap<ListDefId, ListDefDirect>,
    /// The lists, by id.
    pub lists: HashMap<ListId, ListDirect>,
    /// The picture bullets, by id.
    pub pictures: BTreeMap<i64, PictureDirect>,
    /// The `w:numbering` start tag, with its namespace declarations.
    pub root: String,
}

impl NumberingXml {
    /// The numbering part `xml`; empty for none, or for one that doesn't parse. Of elements with the same id the last
    /// counts; one without an id is left out.
    pub fn parse(xml: Option<&str>) -> NumberingXml {
        let mut n = NumberingXml::default();
        let Some(text) = xml else { return n };
        let Ok(doc) = roxmltree::Document::parse(text) else { return n };
        let root = doc.root_element();
        let start = root.range().start;
        n.root = text[start..start + text[start..].find('>').map_or(0, |e| e + 1)].to_string();
        for p in children(root, "numPicBullet") {
            if let Some(b) = parse_picture(p) {
                n.pictures.insert(b.id, Direct::read(b, text[p.range()].into()));
            }
        }
        for a in children(root, "abstractNum") {
            if let Some(d) = parse_def(a) {
                n.defs.insert(d.id, Direct::read(d, text[a.range()].into()));
            }
        }
        for m in children(root, "num") {
            if let Some(l) = parse_list(m) {
                n.lists.insert(l.id, Direct::read(l, text[m.range()].into()));
            }
        }
        n
    }

    /// The namespaces to parse and write this part's elements with.
    pub fn cx(&self) -> Cx {
        if self.root.is_empty() {
            return Cx::of_xml(&format!("<w:numbering xmlns:w=\"{W_NS}\">"));
        }
        Cx::of_xml(&self.root)
    }

    /// The typed definitions, lists and picture bullets.
    pub fn lists(&self) -> Lists {
        Lists {
            defs: self.defs.iter().map(|(k, d)| (*k, d.get().clone())).collect(),
            lists: self.lists.iter().map(|(k, l)| (*k, l.get().clone())).collect(),
            pictures: self.pictures.iter().map(|(k, p)| (*k, p.get().clone())).collect(),
        }
    }

    /// The XML path's numbering of the part: each element as this part writes it, read as that path reads it.
    pub fn to_numbering(&self, colours: &ThemeColours) -> Numbering {
        let cx = self.cx();
        Numbering {
            abstracts: self.defs.iter().map(|(k, d)| (k.0, to_abstract_num(d, colours, &cx))).collect(),
            nums: self.lists.iter().map(|(k, l)| (k.0, to_num(l, colours, &cx))).collect(),
            pic_bullets: self.pictures.iter().map(|(k, p)| (*k, to_pic_bullet(p, colours, &cx))).collect(),
            root: self.root.clone(),
        }
    }
}

// Conversion to the XML path's numbering

fn origin_of(o: Origin) -> StyleOrigin {
    match o {
        Origin::Document => StyleOrigin::Document,
        Origin::Glossary => StyleOrigin::Glossary,
    }
}

/// `l` as the XML path's level: each value as that path gives it, its paragraph and run properties as that path reads
/// them from the `w:pPr` and `w:rPr` the level is written with.
pub fn to_level(l: &ListLevel, colours: &ThemeColours, cx: &Cx) -> Level {
    let xml = |key| {
        let mut out = Out::new(cx, colours);
        level_child_xml(&mut out, l, key, colours, cx);
        out.s
    };
    let ppr = cx.parse(&xml("pPr"), |_, el| XmlParaProps::parse(Some(el), colours)).unwrap_or_default();
    let rpr = cx.parse(&xml("rPr"), |_, el| RunProps::parse(Some(el), colours)).unwrap_or_default();
    let name = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
    Level {
        start: l.start.unwrap_or(0),
        fmt: format_name(l.format.as_ref()).to_string(),
        text: l.text.as_deref().unwrap_or_default().to_string(),
        jc: jc_name(l.jc.as_ref()).to_string(),
        suff: suffix_name(l.suffix.as_ref()).to_string(),
        is_lgl: l.legal.unwrap_or(false),
        restart: l.restart,
        pstyle: name(&l.style),
        pic_bullet: l.pic_bullet,
        legacy: legacy_of(l),
        ppr,
        rpr,
    }
}

/// A definition as the XML path's, with the element it writes.
pub fn to_abstract_num(d: &ListDefDirect, colours: &ThemeColours, cx: &Cx) -> AbstractNum {
    let v = d.get();
    let name = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
    AbstractNum {
        levels: v.levels.iter().map(|l| l.as_ref().map(|l| to_level(l, colours, cx))).collect(),
        num_style_link: name(&v.num_style_link),
        style_link: name(&v.style_link),
        multi_level_type: v.multi_level_type.as_ref().map(|m| literal_name(m, &MULTI).to_string()),
        name: name(&v.name),
        origin: origin_of(v.origin),
        raw: d.xml(colours, cx).into_owned(),
    }
}

/// A list as the XML path's, with the element it writes.
pub fn to_num(n: &ListDirect, colours: &ThemeColours, cx: &Cx) -> Num {
    let v = n.get();
    Num {
        abstract_id: v.def.map_or(-1, |d| d.0),
        start_overrides: v.overrides.iter().filter_map(|o| Some((o.level, o.start?))).collect(),
        level_overrides: v.overrides.iter().filter_map(|o| Some((o.level, to_level(o.def.as_ref()?, colours, cx)))).collect(),
        origin: origin_of(v.origin),
        raw: n.xml(colours, cx).into_owned(),
    }
}

/// A picture bullet as the XML path's, with the element it writes.
pub fn to_pic_bullet(p: &PictureDirect, colours: &ThemeColours, cx: &Cx) -> PicBullet {
    let v = p.get();
    PicBullet { raw: p.xml(colours, cx).into_owned(), rel: v.rel.as_deref().map(str::to_string), image: v.image }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::numbering::{parse_abstract, parse_num};
    use super::*;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml""#;

    fn cx() -> Cx {
        Cx::of_xml(&format!("<w:numbering {NS}>"))
    }

    /// Where the XML path's levels differ, field by field.
    pub(crate) fn level_diff(a: &Level, b: &Level) -> Vec<&'static str> {
        let mut d = Vec::new();
        let mut check = |same: bool, name| {
            if !same {
                d.push(name);
            }
        };
        check(a.start == b.start, "start");
        check(a.fmt == b.fmt, "fmt");
        check(a.text == b.text, "text");
        check(a.jc == b.jc, "jc");
        check(a.suff == b.suff, "suff");
        check(a.is_lgl == b.is_lgl, "is_lgl");
        check(a.restart == b.restart, "restart");
        check(a.pstyle == b.pstyle, "pstyle");
        check(a.pic_bullet == b.pic_bullet, "pic_bullet");
        check(a.legacy == b.legacy, "legacy");
        check(a.ppr == b.ppr, "ppr");
        check(a.rpr == b.rpr, "rpr");
        d
    }

    /// Where the XML path's definitions differ (their XML too, when `raw`).
    pub(crate) fn abstract_diff(a: &AbstractNum, b: &AbstractNum, raw: bool) -> Vec<String> {
        let mut d = Vec::new();
        if a.levels.len() != b.levels.len() {
            d.push("levels".to_string());
        }
        for (k, (x, y)) in a.levels.iter().zip(&b.levels).enumerate() {
            match (x, y) {
                (Some(x), Some(y)) => d.extend(level_diff(x, y).into_iter().map(|f| format!("level {k}: {f}"))),
                (None, None) => {}
                _ => d.push(format!("level {k}")),
            }
        }
        let fields = [
            (a.num_style_link == b.num_style_link, "num_style_link"),
            (a.style_link == b.style_link, "style_link"),
            (a.multi_level_type == b.multi_level_type, "multi_level_type"),
            (a.name == b.name, "name"),
            (a.origin == b.origin, "origin"),
            (!raw || a.raw == b.raw, "raw"),
        ];
        d.extend(fields.into_iter().filter(|f| !f.0).map(|f| f.1.to_string()));
        d
    }

    /// Where the XML path's lists differ (their XML too, when `raw`).
    pub(crate) fn num_diff(a: &Num, b: &Num, raw: bool) -> Vec<String> {
        let mut d = Vec::new();
        let fields = [
            (a.abstract_id == b.abstract_id, "abstract_id"),
            (a.start_overrides == b.start_overrides, "start_overrides"),
            (a.origin == b.origin, "origin"),
            (!raw || a.raw == b.raw, "raw"),
        ];
        d.extend(fields.into_iter().filter(|f| !f.0).map(|f| f.1.to_string()));
        let mut keys: Vec<&i64> = a.level_overrides.keys().collect();
        keys.sort();
        let mut other: Vec<&i64> = b.level_overrides.keys().collect();
        other.sort();
        if keys != other {
            d.push("level_overrides".to_string());
        }
        for k in keys {
            if let (Some(x), Some(y)) = (a.level_overrides.get(k), b.level_overrides.get(k)) {
                d.extend(level_diff(x, y).into_iter().map(|f| format!("override {k}: {f}")));
            }
        }
        d
    }

    fn def(x: &str) -> (ListDefDirect, AbstractNum) {
        let cx = cx();
        let held = ListDefDirect::from_xml(x, &cx).expect("a definition");
        let path = cx.parse(x, |text, el| parse_abstract(el, text, &ThemeColours::default())).flatten().expect("the XML path's").1;
        (held, path)
    }

    fn list(x: &str) -> (ListDirect, Num) {
        let cx = cx();
        let held = ListDirect::from_xml(x, &cx).expect("a list");
        let path = cx.parse(x, |text, el| parse_num(el, text, &ThemeColours::default())).flatten().expect("the XML path's").1;
        (held, path)
    }

    /// `x` (a `w:abstractNum`) converts to what the XML path reads, and written again reads the same on both paths and
    /// loses nothing.
    fn def_agrees(x: &str) -> ListDefDirect {
        let (cx, colours) = (cx(), ThemeColours::default());
        let (held, path) = def(x);
        assert_eq!(held.xml(&colours, &cx), x, "as read");
        assert_eq!(abstract_diff(&to_abstract_num(&held, &colours, &cx), &path, true), Vec::<String>::new(), "{x}");
        let again = serialize_def(held.get(), &colours, &cx);
        let (re, written_path) = def(&again);
        assert_eq!(re.get(), held.get(), "typed, written as {again}");
        assert_eq!(abstract_diff(&written_path, &path, false), Vec::<String>::new(), "on the XML path, written as {again}");
        let lost = cx.parse(x, |_, a| cx.parse(&again, |_, b| char_props::tests::lost(a, b))).flatten().unwrap_or_default();
        assert_eq!(lost, Vec::<String>::new(), "written as {again}");
        held
    }

    fn list_agrees(x: &str) -> ListDirect {
        let (cx, colours) = (cx(), ThemeColours::default());
        let (held, path) = list(x);
        assert_eq!(held.xml(&colours, &cx), x, "as read");
        assert_eq!(num_diff(&to_num(&held, &colours, &cx), &path, true), Vec::<String>::new(), "{x}");
        let again = serialize_list(held.get(), &colours, &cx);
        let (re, written_path) = list(&again);
        assert_eq!(re.get(), held.get(), "typed, written as {again}");
        assert_eq!(num_diff(&written_path, &path, false), Vec::<String>::new(), "on the XML path, written as {again}");
        let lost = cx.parse(x, |_, a| cx.parse(&again, |_, b| char_props::tests::lost(a, b))).flatten().unwrap_or_default();
        assert_eq!(lost, Vec::<String>::new(), "written as {again}");
        held
    }

    #[test]
    fn a_level_reads_as_the_xml_path_reads_it() {
        let held = def_agrees(
            r#"<w:abstractNum w:abstractNumId="3"><w:nsid w:val="5A1B2C3D"/><w:multiLevelType w:val="hybridMultilevel"/><w:tmpl w:val="0409000f"/><w:name w:val="Outline"/><w:styleLink w:val="Headings"/><w:lvl w:ilvl="0" w:tplc="04090013" w:tentative="1"><w:start w:val="2"/><w:numFmt w:val="upperRoman"/><w:lvlRestart w:val="0"/><w:pStyle w:val="Heading1"/><w:isLgl/><w:suff w:val="space"/><w:lvlText w:val="%1."/><w:lvlPicBulletId w:val="4"/><w:legacy w:legacy="1" w:legacySpace="120" w:legacyIndent="360"/><w:lvlJc w:val="right"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr><w:rPr><w:b/><w:color w:val="FF0000"/></w:rPr></w:lvl></w:abstractNum>"#,
        );
        let d = held.get();
        let l = d.levels[0].as_deref().expect("level 0");
        assert_eq!((d.nsid, d.template), (Some(0x5A1B2C3D), Some(0x0409000F)));
        assert_eq!(d.multi_level_type, Some(Literal::Known(MultiLevelType::HybridMultilevel)));
        assert_eq!(l.format, Some(Literal::Known(NumberFormat::UpperRoman)));
        assert_eq!((l.jc.clone(), l.suffix.clone()), (Some(Literal::Known(ListJc::Right)), Some(Literal::Known(ListSuffix::Space))));
        assert_eq!(l.legacy, Some(LegacyLabel { on: Some(true), space: Some(6.0), indent: Some(18.0) }));
        assert_eq!((l.template, l.tentative, l.legal, l.restart), (Some(0x04090013), Some(true), Some(true), Some(0)));
        assert_eq!(l.para.indent_first, Some(-18.0));
    }

    #[test]
    fn unset_values_read_as_the_xml_path_defaults_them() {
        let held = def_agrees(
            r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"/><w:lvl w:ilvl="1"><w:legacy w:legacySpace="120"/></w:lvl></w:abstractNum>"#,
        );
        let level = to_level(held.get().levels[0].as_deref().expect("level 0"), &ThemeColours::default(), &cx());
        assert_eq!((level.start, level.fmt.as_str(), level.text.as_str(), level.jc.as_str(), level.suff.as_str()), (0, "decimal", "", "left", "tab"));
        assert!(!level.is_lgl && level.legacy.is_none());
        let level = to_level(held.get().levels[1].as_deref().expect("level 1"), &ThemeColours::default(), &cx());
        assert_eq!(level.legacy, None, "the earlier placement only when on");
    }

    #[test]
    fn names_this_model_does_not_know_are_kept_as_written() {
        let held = def_agrees(
            r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="tiered"/><w:lvl w:ilvl="0"><w:numFmt w:val="decimalEnclosedSquare"/><w:suff w:val="dash"/><w:lvlJc w:val="middle"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="custom" w:format="001, 002, 003, ..."/><w:lvlJc w:val="start"/></w:lvl><w:lvl w:ilvl="2"><w:numFmt w:val=""/></w:lvl></w:abstractNum>"#,
        );
        let l = held.get().levels[0].as_deref().expect("level 0");
        assert_eq!(l.format, Some(Literal::Other("decimalEnclosedSquare".into())));
        assert_eq!(l.jc, Some(Literal::Other("middle".into())));
        let custom = held.get().levels[1].as_deref().expect("level 1");
        assert_eq!((custom.format.clone(), custom.format_code.as_deref()), (Some(Literal::Known(NumberFormat::Custom)), Some("001, 002, 003, ...")));
        assert_eq!(held.get().levels[2].as_deref().and_then(|l| l.format.clone()), Some(Literal::Other("".into())));
    }

    #[test]
    fn the_last_level_of_each_index_counts_and_the_first_of_each_child() {
        let held = def_agrees(
            r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="7"/></w:lvl><w:lvl w:ilvl="12"><w:start w:val="9"/></w:lvl><w:lvl><w:start w:val="3"/><w:start w:val="4"/></w:lvl><w:lvl w:ilvl="x"><w:numFmt/><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1.6"><w:start w:val="5"/></w:lvl><w:name w:val="A"/><w:name w:val="B"/></w:abstractNum>"#,
        );
        let d = held.get();
        let level = d.levels[0].as_deref().expect("level 0");
        assert_eq!((level.start, level.format.clone()), (None, None), "the last without an index, its first numFmt without a value");
        assert_eq!(d.levels[2].as_deref().and_then(|l| l.start), Some(5), "1.6 rounds to 2");
        assert_eq!(d.name.as_deref(), Some("A"));
    }

    #[test]
    fn a_level_keeps_what_its_properties_hold_beyond_their_typed_value() {
        let held = def_agrees(
            r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:pPr/><w:rPr><w:b/><w:rPrChange w:id="1" w:author="A"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr></w:lvl><w:lvl w:ilvl="1"><w:pPr><w:ind w:left="720"/><w:pPrChange w:id="2"><w:pPr/></w:pPrChange></w:pPr><w:rPr/></w:lvl><w:lvl w:ilvl="2"><w:start w:val="01"><x:ext xmlns:x="urn:x"/></w:start><x:other xmlns:x="urn:x" w:val="1"/></w:lvl></w:abstractNum>"#,
        );
        let mut l = (*held.get().levels[1].clone().expect("level 1")).clone();
        l.para.indent_left = Some(18.0);
        let again = serialize_level(&l, 1, &ThemeColours::default(), &cx());
        assert!(again.contains(r#"<w:ind w:left="360"/><w:pPrChange w:id="2"><w:pPr/></w:pPrChange>"#), "{again}");
    }

    #[test]
    fn a_list_overrides_each_level_with_the_last_override_that_sets_each_value() {
        let held = list_agrees(
            r#"<w:num w:numId="4"><w:abstractNumId w:val="2"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride><w:lvlOverride w:ilvl="0"><w:startOverride w:val="x"/><w:lvl w:ilvl="0"><w:start w:val="3"/></w:lvl></w:lvlOverride><w:lvlOverride><w:startOverride w:val="8"/></w:lvlOverride><w:lvlOverride w:ilvl="12"><w:startOverride w:val="6"/></w:lvlOverride><w:lvlOverride w:ilvl="1"><w:lvl w:ilvl="3"><w:numFmt w:val="lowerLetter"/></w:lvl></w:lvlOverride></w:num>"#,
        );
        let n = held.get();
        assert_eq!(n.overrides.iter().map(|o| (o.level, o.start)).collect::<Vec<_>>(), [(0, Some(5)), (1, None), (12, Some(6))]);
        assert_eq!(n.override_of(0).and_then(|o| o.def.as_deref()).and_then(|l| l.start), Some(3));
        list_agrees(r#"<w:num w:numId="1"/>"#);
        assert_eq!(to_num(&list(r#"<w:num w:numId="1"/>"#).0, &ThemeColours::default(), &cx()).abstract_id, -1);
    }

    #[test]
    fn a_picture_bullet_keeps_its_picture() {
        let x = r#"<w:numPicBullet w:numPicBulletId="0"><w:pict><v:shape id="_x0000_i1025"><v:imagedata r:id="rId1" o:title=""/></v:shape></w:pict></w:numPicBullet>"#
            .replace(" o:title=\"\"", "");
        let cx = cx();
        let held = PictureDirect::from_xml(&x, &cx).expect("a picture bullet");
        assert_eq!(held.get().rel.as_deref(), Some("rId1"));
        let again = serialize_picture(held.get(), &ThemeColours::default(), &cx);
        assert_eq!(again, x);
        let mut with_image = (**held.get()).clone();
        with_image.image = Some(3);
        assert_eq!(held.edited(with_image, &[]).as_read().map(|x| &**x), Some(x.as_str()), "its image isn't written");
    }

    #[test]
    fn an_edit_writes_over_the_element_it_was_read_from() {
        let (cx, colours) = (cx(), ThemeColours::default());
        let x = r#"<w:abstractNum w:abstractNumId='1' w:other='kept'> <w:nsid w:val='00ab12cd'/><w:lvl w:ilvl='0'><w:start w:val='1'/><w:numFmt w:val='decimal'/><w:lvlText w:val='%1.'/><w:pPr><w:ind w:left='720'  w:hanging='360'/></w:pPr></w:lvl><w:lvl w:ilvl='1'><w:start w:val='1'/></w:lvl></w:abstractNum>"#;
        let held = ListDefDirect::from_xml(x, &cx).expect("a definition");
        let mut d = (**held.get()).clone();
        let mut l = (*d.levels[0].clone().expect("level 0")).clone();
        l.start = Some(4);
        l.para.indent_left = Some(72.0);
        d.levels[0] = Some(Rc::new(l));
        d.id = ListDefId(7);
        let set = [ListDefProp::Level(0, LevelProp::Para(ParaSet { prop: crate::format::ParaProp::IndentLeft, whole: false }))];
        let edited = held.edited(d, &set);
        assert_eq!(
            edited.xml(&colours, &cx),
            r#"<w:abstractNum w:abstractNumId='7' w:other='kept'><w:nsid w:val='00ab12cd'/><w:lvl w:ilvl='0'><w:start w:val="4"/><w:numFmt w:val='decimal'/><w:lvlText w:val='%1.'/><w:pPr><w:ind w:left="1440" w:hanging='360'/></w:pPr></w:lvl><w:lvl w:ilvl='1'><w:start w:val='1'/></w:lvl></w:abstractNum>"#
        );
        let mut d = (**held.get()).clone();
        d.levels[1] = None;
        d.levels[3] = Some(Rc::new(ListLevel { start: Some(2), ..ListLevel::default() }));
        d.name = Some("Steps".into());
        assert_eq!(
            held.edited(d, &[]).xml(&colours, &cx),
            r#"<w:abstractNum w:abstractNumId='1' w:other='kept'><w:nsid w:val='00ab12cd'/><w:name w:val="Steps"/><w:lvl w:ilvl='0'><w:start w:val='1'/><w:numFmt w:val='decimal'/><w:lvlText w:val='%1.'/><w:pPr><w:ind w:left='720'  w:hanging='360'/></w:pPr></w:lvl><w:lvl w:ilvl="3"><w:start w:val="2"/></w:lvl></w:abstractNum>"#
        );
        let (list, _) =
            list(r#"<w:num w:numId="2"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride></w:num>"#);
        let mut n = (**list.get()).clone();
        n.def = Some(ListDefId(7));
        n.overrides[0].start = Some(3);
        assert_eq!(
            list.edited(n, &[]).xml(&colours, &cx),
            r#"<w:num w:numId="2"><w:abstractNumId w:val="7"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="3"/></w:lvlOverride></w:num>"#
        );
    }

    #[test]
    fn a_level_lookup_follows_a_numbering_style_link_once() {
        let x = format!(
            r#"<w:numbering {NS}><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:numStyleLink w:val="Outline"/></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="3"><w:lvl w:ilvl="3"><w:start w:val="9"/></w:lvl></w:lvlOverride></w:num></w:numbering>"#
        );
        let lists = NumberingXml::parse(Some(&x)).lists();
        let linked = |s: &str| (s == "Outline").then_some(ListId(1));
        assert_eq!(lists.def_of(ListId(2), linked), Some(ListDefId(0)));
        assert_eq!(lists.def_of(ListId(2), |_| None), None, "a link to a style without a list");
        assert_eq!(lists.level(ListId(2), -4, linked).and_then(|l| l.start), Some(1));
        assert_eq!(lists.level(ListId(2), 3, |_| None).and_then(|l| l.start), Some(9), "an override without the definition");
        let levels = lists.levels(ListId(2), linked).expect("levels");
        assert_eq!(levels.map(|l| l.and_then(|l| l.start)), [Some(1), None, None, Some(9), None, None, None, None, None]);
        assert_eq!(lists.free_ids(), (ListDefId(2), ListId(3)));
    }
}
