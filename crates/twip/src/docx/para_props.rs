//! Typed paragraph properties from and to `w:pPr`
//!
//! As for run properties ([`super::char_props`]), the parse keeps in [`ForeignProps`] what the typed value does not
//! hold, so that writing the value again loses nothing of what was read: unmodelled children whole, earlier duplicates
//! of a modelled child (the last one counts), a modelled child its value doesn't keep whole, unmodelled attributes of
//! modelled children and of the element itself, modelled attributes written otherwise than the writer writes their
//! value, and the children with content (tab stops, the list, borders) whole beside their typed value.
//!
//! Three children belong to other nodes and are not part of the typed value: the paragraph mark's `w:rPr`, the
//! section break's `w:sectPr` and the tracked change's `w:pPrChange`. The serializer writes them where CT_PPr places
//! them when it is given them ([`Owned`]); an edit written over the `w:pPr` as read keeps them as they are there.

use std::fmt::Write as _;
use std::rc::Rc;

use roxmltree::Node;

use super::prop_xml::{Compare, FILL, LINE_COLOUR, Out, Sink, Twips, border, border_attrs, colour, colour_attrs, foreign_child, props_key};
use super::props::{self as xml_props, FramePr, ThemeColours};
use super::write::props::Cx;
use super::xml::{W, W15, attr, attr_twips, children, int, num, on_off, on_off_str, val};
use crate::document::format::{Jc, Leader, LineRule, TabKind};
use crate::document::paragraph::{ListId, TextAlignment};
use crate::format::{
    ForeignChild, ForeignProps, ListChoice, ParaBorders, ParaEdit, ParaFlag, ParaFlags, ParaProp, ParaProps, ParaSet, QName, Shading, TabStop,
    TextBorder, TightWrap,
};

/// CT_PPr's children in schema order, Word 2012's collapsed flag before the tracked change.
const ORDER: [&str; 37] = [
    "pStyle",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "framePr",
    "widowControl",
    "numPr",
    "suppressLineNumbers",
    "pBdr",
    "shd",
    "tabs",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "adjustRightInd",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "jc",
    "textDirection",
    "textAlignment",
    "textboxTightWrap",
    "outlineLvl",
    "divId",
    "cnfStyle",
    "rPr",
    "sectPr",
    COLLAPSED,
    "pPrChange",
];

/// Word 2012's collapsed flag, as [`ORDER`] names it.
const COLLAPSED: &str = "w15:collapsed";

/// The on/off children and their flags.
const FLAGS: [(&str, ParaFlag); 17] = [
    ("keepNext", ParaFlag::KEEP_NEXT),
    ("keepLines", ParaFlag::KEEP_LINES),
    ("pageBreakBefore", ParaFlag::PAGE_BREAK_BEFORE),
    ("widowControl", ParaFlag::WIDOW_CONTROL),
    ("suppressLineNumbers", ParaFlag::SUPPRESS_LINE_NUMBERS),
    ("suppressAutoHyphens", ParaFlag::SUPPRESS_AUTO_HYPHENS),
    ("kinsoku", ParaFlag::KINSOKU),
    ("wordWrap", ParaFlag::WORD_WRAP),
    ("overflowPunct", ParaFlag::OVERFLOW_PUNCT),
    ("topLinePunct", ParaFlag::TOP_LINE_PUNCT),
    ("autoSpaceDE", ParaFlag::AUTO_SPACE_DE),
    ("autoSpaceDN", ParaFlag::AUTO_SPACE_DN),
    ("bidi", ParaFlag::BIDI),
    ("snapToGrid", ParaFlag::SNAP_TO_GRID),
    ("contextualSpacing", ParaFlag::CONTEXTUAL),
    ("mirrorIndents", ParaFlag::MIRROR_INDENTS),
    ("suppressOverlap", ParaFlag::SUPPRESS_OVERLAP),
];

/// Children another node owns: the mark's properties, the section break, the tracked change.
const OWNED: [&str; 3] = ["rPr", "sectPr", "pPrChange"];

/// Children with content the parse reads; they are kept as written too.
const WHOLE: [&str; 3] = ["tabs", "numPr", "pBdr"];

/// The text flow codes and their `ST_TextDirection` values, as written (the first) or read.
const FLOWS: [(&str, u8); 11] =
    [("lrTb", 0), ("tbRl", 1), ("rl", 1), ("btLr", 3), ("lr", 3), ("lrTbV", 4), ("tbV", 4), ("tbRlV", 5), ("rlV", 5), ("tbLrV", 7), ("lrV", 7)];

const TIGHT: [(&str, TightWrap); 5] = [
    ("none", TightWrap::None),
    ("allLines", TightWrap::AllLines),
    ("firstAndLastLine", TightWrap::FirstAndLastLine),
    ("firstLineOnly", TightWrap::FirstLineOnly),
    ("lastLineOnly", TightWrap::LastLineOnly),
];

/// The place in [`ORDER`] of an element's name, if it has one.
fn rank_of(ns: &str, local: &str) -> Option<usize> {
    match ns {
        W => ORDER.iter().position(|k| *k == local),
        W15 if local == "collapsed" => ORDER.iter().position(|k| *k == COLLAPSED),
        _ => None,
    }
}

/// The children of a `w:pPr` other nodes own, as written, for the serializer to place.
#[derive(Clone, Copy, Debug, Default)]
pub struct Owned<'a> {
    /// The mark's `w:rPr`.
    pub mark: Option<&'a str>,
    /// The section break's `w:sectPr`.
    pub section: Option<&'a str>,
    /// The tracked change's `w:pPrChange`.
    pub change: Option<&'a str>,
}

impl<'a> Owned<'a> {
    fn get(&self, key: &str) -> Option<&'a str> {
        match key {
            "rPr" => self.mark,
            "sectPr" => self.section,
            _ => self.change,
        }
    }
}

/// A paragraph's authored properties: typed, with the `w:pPr` they were read from (or last written as) beside them.
/// The written form stands for the properties only while they are the ones it was read as; never part of equality.
#[derive(Clone, Debug)]
pub struct ParaDirect {
    props: Rc<ParaProps>,
    origin: Option<(Rc<ParaProps>, Rc<str>)>,
    /// What edits since reading set, in order: written again over the `w:pPr` as read.
    set: Vec<ParaSet>,
}

thread_local! {
    /// No properties, shared.
    static EMPTY: Rc<ParaProps> = Rc::new(ParaProps::default());
}

impl Default for ParaDirect {
    fn default() -> ParaDirect {
        ParaDirect { props: EMPTY.with(Rc::clone), origin: None, set: Vec::new() }
    }
}

impl ParaDirect {
    /// Properties read from `xml`, the `w:pPr` they were parsed from.
    pub fn read(props: ParaProps, xml: Rc<str>) -> ParaDirect {
        let props = Rc::new(props);
        ParaDirect { origin: Some((props.clone(), xml)), props, set: Vec::new() }
    }

    /// The properties of `xml`, a `w:pPr` with the main namespace bound to `w`.
    #[cfg(test)]
    pub(crate) fn in_w(xml: &str) -> ParaDirect {
        ParaDirect::from_xml(Some(xml), &Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">")))
    }

    /// Properties read from `xml`, shared with other paragraphs read alike.
    pub fn shared(props: Rc<ParaProps>, xml: Rc<str>) -> ParaDirect {
        ParaDirect { origin: Some((props.clone(), xml)), props, set: Vec::new() }
    }

    /// The `w:pPr` they were read from, while they are the properties read from it, with those properties.
    pub fn read_from(&self) -> Option<(&Rc<ParaProps>, &Rc<str>)> {
        let xml = self.as_read()?;
        Some((&self.props, xml))
    }

    /// The `w:pPr` they were read from (or last written as), whatever they are now.
    pub fn origin_xml(&self) -> Option<&Rc<str>> {
        self.origin.as_ref().map(|(_, x)| x)
    }

    /// These properties with `xml` in place of the `w:pPr` they were read from: the same element, written otherwise
    /// (its namespaces declared on it).
    pub fn with_origin_xml(&self, xml: Rc<str>) -> ParaDirect {
        let origin = self.origin.as_ref().map(|(p, _)| (p.clone(), xml));
        ParaDirect { props: self.props.clone(), origin, set: self.set.clone() }
    }

    /// Typed properties with no written form.
    pub fn typed(props: Rc<ParaProps>) -> ParaDirect {
        ParaDirect { props, origin: None, set: Vec::new() }
    }

    /// The properties `xml` (a `w:pPr` in the part `cx` describes) holds; none for no XML, and nothing typed (the
    /// XML kept as written) for XML that doesn't parse.
    pub fn from_xml(xml: Option<&str>, cx: &Cx) -> ParaDirect {
        let Some(x) = xml else { return ParaDirect::default() };
        ParaDirect::read(cx.parse(x, |_, el| parse(el)).unwrap_or_default(), x.into())
    }

    /// The typed properties.
    pub fn get(&self) -> &Rc<ParaProps> {
        &self.props
    }

    /// The `w:pPr` as read, while the properties are the ones read from it.
    pub fn as_read(&self) -> Option<&Rc<str>> {
        let (read, xml) = self.origin.as_ref()?;
        (Rc::ptr_eq(read, &self.props) || **read == *self.props).then_some(xml)
    }

    /// The `w:pPr` to write in the part `cx` describes: as read while the properties are those, else written from
    /// them (a theme colour's value as `colours` resolve it), the children other nodes own as they were read; `None`
    /// when nothing is left.
    pub fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<std::borrow::Cow<'_, str>> {
        if let Some(x) = self.as_read() {
            return Some(std::borrow::Cow::Borrowed(x));
        }
        let origin = self.origin.as_ref().map(|(read, xml)| (&**read, &**xml));
        if origin.is_none() && *self.props == ParaProps::default() {
            return None;
        }
        written_over(&self.props, colours, cx, origin, &self.set).map(std::borrow::Cow::Owned)
    }

    /// `props` in place of these properties, set by an edit that set `set`, in order: the `w:pPr` they were read from
    /// kept to write them over, what was set written again.
    pub fn edited(&self, mut props: ParaProps, set: &[ParaSet]) -> ParaDirect {
        let mut all = self.set.clone();
        all.extend_from_slice(set);
        if props.foreign.is_some() {
            drop_replaced(&self.props, &mut props, &all);
        }
        ParaDirect { props: Rc::new(props), origin: self.origin.clone(), set: all }
    }

    /// An edit's result as these properties' successor.
    pub fn edit(&self, e: ParaEdit) -> ParaDirect {
        self.edited(e.props, &e.set)
    }

    /// These properties where `cx` describes the part they are written in: `None` while the `w:pPr` they were read
    /// from reads the same there, else without it, to be written from the properties.
    pub fn moved_to(&self, cx: &Cx) -> Option<ParaDirect> {
        let (read, xml) = self.origin.as_ref()?;
        if cx.parse(xml, |_, el| parse(el)).is_some_and(|p| p == **read) {
            return None;
        }
        Some(ParaDirect { props: self.props.clone(), origin: None, set: self.set.clone() })
    }

    /// Authored equality: the typed properties.
    pub fn authored_eq(&self, o: &ParaDirect) -> bool {
        Rc::ptr_eq(&self.props, &o.props) || self.props == o.props
    }
}

/// `p` reset: nothing but its style (when `style`), its HTML division and its table style conditional formatting
/// record (`w:cnfStyle`, which is not formatting of its own).
pub fn reset(p: &ParaProps, style: bool) -> ParaProps {
    let foreign = p.foreign.as_deref().and_then(|f| {
        // What is written of the children kept: the record whole, the style and division as written
        let stays = |q: &QName| &*q.ns == W && (q.local.as_ref() == "cnfStyle" || &*q.local == "divId" || style && &*q.local == "pStyle");
        let cnf = |q: &QName| &*q.ns == W && &*q.local == "cnfStyle";
        let kept = ForeignProps {
            children: f.children.iter().filter(|c| cnf(&c.name)).cloned().collect(),
            attributes: f.attributes.iter().filter(|a| a.on.as_ref().is_none_or(stays)).cloned().collect(),
            kept: f.kept.iter().filter(|c| stays(&c.name)).cloned().collect(),
        };
        (kept != ForeignProps::default()).then(|| Rc::new(kept))
    });
    ParaProps { style: p.style.clone().filter(|_| style), division: p.division, foreign, ..ParaProps::default() }
}

// Parse

/// A `w:pPr` as typed properties, without the children other nodes own.
pub fn parse(ppr: Node) -> ParaProps {
    let mut p = ParaProps::default();
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    Compare::new(ppr, &mut scratch).finish(None, &mut f.attributes);
    let elements = || ppr.children().filter(|c| c.is_element());
    // The child each name is read from: the last, but the first of those other nodes own and of the collapsed flag.
    let mut pick = [None::<usize>; ORDER.len()];
    for (i, e) in elements().enumerate() {
        if let Some(k) = rank_of(e.tag_name().namespace().unwrap_or_default(), e.tag_name().name())
            && (!(OWNED.contains(&ORDER[k]) || ORDER[k] == COLLAPSED) || pick[k].is_none())
        {
            pick[k] = Some(i);
        }
    }
    for (i, e) in elements().enumerate() {
        let (ns, name) = (e.tag_name().namespace().unwrap_or_default(), e.tag_name().name());
        let Some(key) = rank_of(ns, name).filter(|k| pick[*k] == Some(i)).map(|k| ORDER[k]) else {
            f.children.push(foreign_child(e));
            continue;
        };
        if OWNED.contains(&key) {
            continue;
        }
        if !modelled(&mut p, e, key) {
            f.children.push(foreign_child(e));
            continue;
        }
        if WHOLE.contains(&key) {
            f.kept.push(foreign_child(e));
            continue;
        }
        let mut cmp = Compare::new(e, &mut scratch);
        if !emit(&p, key, &mut cmp) {
            f.children.push(foreign_child(e));
            continue;
        }
        cmp.finish(Some(QName { ns: ns.into(), local: name.into() }), &mut f.attributes);
        if e.children().any(|c| c.is_element() || c.text().is_some_and(|t| !t.trim().is_empty())) {
            f.kept.push(foreign_child(e));
        }
    }
    if f != ForeignProps::default() {
        p.foreign = Some(Rc::new(f));
    }
    p
}

/// A child into `p`, as the XML path reads it; false when it isn't modelled.
fn modelled(p: &mut ParaProps, e: Node, key: &str) -> bool {
    if let Some(&(_, flag)) = FLAGS.iter().find(|(n, _)| *n == key) {
        p.flags.put(flag, on_off(e));
        return true;
    }
    match key {
        "pStyle" => p.style = val(e).map(Rc::from),
        "jc" => {
            p.align = Some(match val(e) {
                Some("center") => Jc::Center,
                Some("right" | "end") => Jc::Right,
                Some("both" | "lowKashida" | "mediumKashida" | "highKashida" | "thaiDistribute") => Jc::Both,
                Some("distribute") => Jc::Distribute,
                _ => Jc::Left,
            })
        }
        "ind" => {
            p.indent_left = attr_twips(e, "left").or_else(|| attr_twips(e, "start"));
            p.indent_right = attr_twips(e, "right").or_else(|| attr_twips(e, "end"));
            p.indent_first = attr_twips(e, "hanging").map(|h| -h).or_else(|| attr_twips(e, "firstLine"));
        }
        "spacing" => {
            p.space_before = attr_twips(e, "before");
            p.space_after = attr_twips(e, "after");
            p.space_before_auto = attr(e, "beforeAutospacing").map(|v| on_off_str(Some(v)));
            p.space_after_auto = attr(e, "afterAutospacing").map(|v| on_off_str(Some(v)));
            p.line = num(attr(e, "line"));
            p.line_rule = attr(e, "lineRule").map(|r| match r {
                "exact" => LineRule::Exact,
                "atLeast" => LineRule::AtLeast,
                _ => LineRule::Auto,
            });
        }
        "textboxTightWrap" => p.tight_wrap = Some(val(e).and_then(|v| TIGHT.iter().find(|(n, _)| *n == v)).map_or(TightWrap::None, |(_, t)| *t)),
        "divId" => p.division = int(val(e)),
        "tabs" => {
            p.tabs.clear();
            for t in children(e, "tab") {
                let Some(pos) = attr_twips(t, "pos") else { continue };
                let kind = match val(t) {
                    Some("center") => TabKind::Center,
                    Some("right" | "end") => TabKind::Right,
                    Some("decimal") => TabKind::Decimal,
                    Some("bar") => TabKind::Bar,
                    Some("num") => TabKind::Num,
                    Some("clear") => TabKind::Clear,
                    _ => TabKind::Left,
                };
                let leader = match attr(t, "leader") {
                    Some("dot") => Leader::Dot,
                    Some("hyphen") => Leader::Hyphen,
                    Some("underscore") => Leader::Underscore,
                    Some("heavy") => Leader::Heavy,
                    Some("middleDot") => Leader::MiddleDot,
                    _ => Leader::None,
                };
                p.tabs.push(TabStop { pos, kind, leader });
            }
        }
        "numPr" => {
            let id = super::xml::child(e, "numId").and_then(|n| int(val(n)));
            p.list = id.map(|n| if n == 0 { ListChoice::None } else { ListChoice::In(ListId(n)) });
            p.list_level = super::xml::child(e, "ilvl").and_then(|n| int(val(n)));
        }
        "pBdr" => {
            let side = |name| super::xml::child(e, name).map(border);
            p.borders = ParaBorders {
                top: side("top"),
                bottom: side("bottom"),
                left: side("left").or_else(|| side("start")),
                right: side("right").or_else(|| side("end")),
                between: side("between"),
            };
        }
        "shd" => {
            let pattern = val(e).map_or(crate::document::format::ShadingPattern::Clear, super::line_style::shading_pattern);
            p.shading = Some(Shading { pattern, color: colour(e, LINE_COLOUR), fill: colour(e, FILL) });
        }
        "framePr" => p.frame = Some(frame(e)),
        "textAlignment" => p.text_alignment = val(e).map(super::paragraph::text_alignment),
        "textDirection" => p.text_flow = val(e).map(super::model::text_flow),
        "outlineLvl" => p.outline_level = int(val(e)),
        COLLAPSED => p.collapsed = Some(!matches!(e.attribute((W15, "val")), Some("0" | "false" | "off"))),
        _ => return false,
    }
    true
}

/// A `w:framePr`, as the XML path reads it.
fn frame(e: Node) -> FramePr {
    let a = |n| attr(e, n).map(str::to_string);
    let mut key: Vec<String> = e.attributes().map(|a| format!("{}={}", a.name(), a.value())).collect();
    key.sort();
    FramePr {
        w: attr_twips(e, "w"),
        h: attr_twips(e, "h"),
        h_rule: a("hRule"),
        x: attr_twips(e, "x"),
        y: attr_twips(e, "y"),
        x_align: a("xAlign"),
        y_align: a("yAlign"),
        h_anchor: a("hAnchor"),
        v_anchor: a("vAnchor"),
        wrap: a("wrap"),
        h_space: attr_twips(e, "hSpace").unwrap_or(0.0),
        v_space: attr_twips(e, "vSpace").unwrap_or(0.0),
        drop_cap: a("dropCap"),
        lines: int(attr(e, "lines")),
        key: key.join(" "),
    }
}

// Conversion from the XML path's properties

/// The XML path's paragraph properties (one level, or levels folded) as typed properties.
pub fn from_para_props(r: &xml_props::ParaProps) -> ParaProps {
    let mut flags = ParaFlags::default();
    let pairs = [
        (r.keep_next, ParaFlag::KEEP_NEXT),
        (r.keep_lines, ParaFlag::KEEP_LINES),
        (r.page_break_before, ParaFlag::PAGE_BREAK_BEFORE),
        (r.widow_control, ParaFlag::WIDOW_CONTROL),
        (r.suppress_line_numbers, ParaFlag::SUPPRESS_LINE_NUMBERS),
        (r.suppress_auto_hyphens, ParaFlag::SUPPRESS_AUTO_HYPHENS),
        (r.kinsoku, ParaFlag::KINSOKU),
        (r.word_wrap, ParaFlag::WORD_WRAP),
        (r.overflow_punct, ParaFlag::OVERFLOW_PUNCT),
        (r.top_line_punct, ParaFlag::TOP_LINE_PUNCT),
        (r.auto_space_de, ParaFlag::AUTO_SPACE_DE),
        (r.auto_space_dn, ParaFlag::AUTO_SPACE_DN),
        (r.bidi, ParaFlag::BIDI),
        (r.snap_to_grid, ParaFlag::SNAP_TO_GRID),
        (r.contextual, ParaFlag::CONTEXTUAL),
        (r.mirror_indents, ParaFlag::MIRROR_INDENTS),
        (r.suppress_overlap, ParaFlag::SUPPRESS_OVERLAP),
    ];
    for (v, f) in pairs {
        if let Some(on) = v {
            flags.put(f, on);
        }
    }
    let rgb =
        |c: Option<u32>, theme: Option<super::model::ThemeColourRef>| theme.map(crate::format::Color::Theme).or(c.map(crate::format::Color::Rgb));
    let side = |b: &Option<xml_props::Border>| {
        b.as_ref().map(|b| TextBorder { style: b.style, width: b.width, space: b.space, shadow: b.shadow, color: rgb(b.color, b.color_theme) })
    };
    ParaProps {
        style: r.style.as_deref().map(Rc::from),
        align: r.jc,
        indent_left: r.ind_left,
        indent_right: r.ind_right,
        indent_first: r.ind_first,
        space_before: r.space_before,
        space_before_auto: r.before_auto,
        space_after: r.space_after,
        space_after_auto: r.after_auto,
        line: r.line,
        line_rule: r.line_rule,
        flags,
        tight_wrap: r.tight_wrap.map(|c| TIGHT.iter().map(|(_, t)| *t).find(|t| t.code() == c).unwrap_or(TightWrap::None)),
        division: r.div_id,
        tabs: r.tabs.iter().map(|t| TabStop { pos: t.pos, kind: t.kind, leader: t.leader }).collect(),
        list: r.num_id.map(|n| if n == 0 { ListChoice::None } else { ListChoice::In(ListId(n)) }),
        list_level: r.ilvl,
        borders: ParaBorders {
            top: side(&r.border_top),
            bottom: side(&r.border_bottom),
            left: side(&r.border_left),
            right: side(&r.border_right),
            between: side(&r.border_between),
        },
        shading: r.shd.as_ref().map(|s| Shading { pattern: s.pattern, color: rgb(s.color, s.color_theme), fill: rgb(s.fill, s.fill_theme) }),
        frame: r.frame.clone(),
        text_flow: r.text_flow,
        text_alignment: r.text_alignment,
        outline_level: r.outline_lvl,
        collapsed: r.collapsed,
        foreign: None,
    }
}

// The attributes a modelled child is written with

/// Writes modelled child `key` of `p` to `s`; false when `p` doesn't set it. The content of a child with content is
/// written apart.
fn emit(p: &ParaProps, key: &str, s: &mut dyn Sink) -> bool {
    let val = |s: &mut dyn Sink, v: &dyn std::fmt::Display| s.attr(W, "val", v);
    if let Some(&(_, flag)) = FLAGS.iter().find(|(n, _)| *n == key) {
        return match p.flags.get(flag) {
            Some(true) => {
                s.implied("val", "1");
                true
            }
            Some(false) => {
                val(s, &"0");
                true
            }
            None => false,
        };
    }
    match key {
        "pStyle" => p.style.as_deref().map(|v| val(s, &v)).is_some(),
        "jc" => p
            .align
            .map(|j| {
                let v = match j {
                    Jc::Left => "left",
                    Jc::Center => "center",
                    Jc::Right => "right",
                    Jc::Both => "both",
                    Jc::Distribute => "distribute",
                };
                val(s, &v)
            })
            .is_some(),
        "ind" => {
            if let Some(v) = p.indent_left {
                s.attr(W, "left", &Twips(v));
            }
            if let Some(v) = p.indent_right {
                s.attr(W, "right", &Twips(v));
            }
            match p.indent_first {
                Some(v) if v.is_sign_negative() => s.attr(W, "hanging", &Twips(-v)),
                Some(v) => s.attr(W, "firstLine", &Twips(v)),
                None => {}
            }
            p.indent_left.is_some() || p.indent_right.is_some() || p.indent_first.is_some()
        }
        "spacing" => {
            let on = |b: bool| if b { "1" } else { "0" };
            if let Some(v) = p.space_before {
                s.attr(W, "before", &Twips(v));
            }
            if let Some(b) = p.space_before_auto {
                s.attr(W, "beforeAutospacing", &on(b));
            }
            if let Some(v) = p.space_after {
                s.attr(W, "after", &Twips(v));
            }
            if let Some(b) = p.space_after_auto {
                s.attr(W, "afterAutospacing", &on(b));
            }
            if let Some(v) = p.line {
                s.attr(W, "line", &v);
            }
            if let Some(r) = p.line_rule {
                let v = match r {
                    LineRule::Auto => "auto",
                    LineRule::Exact => "exact",
                    LineRule::AtLeast => "atLeast",
                };
                s.attr(W, "lineRule", &v);
            }
            p.space_before.is_some()
                || p.space_before_auto.is_some()
                || p.space_after.is_some()
                || p.space_after_auto.is_some()
                || p.line.is_some()
                || p.line_rule.is_some()
        }
        "textboxTightWrap" => p.tight_wrap.map(|t| val(s, &TIGHT.iter().find(|(_, x)| *x == t).map_or("none", |(n, _)| n))).is_some(),
        "divId" => p.division.map(|v| val(s, &v)).is_some(),
        "tabs" => !p.tabs.is_empty(),
        "numPr" => p.list.is_some() || p.list_level.is_some(),
        "pBdr" => !p.borders.is_empty(),
        "shd" => p
            .shading
            .map(|sh| {
                val(s, &super::line_style::shading_val(sh.pattern));
                if let Some(c) = sh.color {
                    colour_attrs(s, c, LINE_COLOUR);
                }
                if let Some(c) = sh.fill {
                    colour_attrs(s, c, FILL);
                }
            })
            .is_some(),
        "framePr" => p.frame.as_ref().map(|f| frame_attrs(f, s)).is_some(),
        "textDirection" => p.text_flow.map(|c| val(s, &FLOWS.iter().find(|(_, x)| *x == c).map_or("lrTb", |(n, _)| n))).is_some(),
        "textAlignment" => p
            .text_alignment
            .map(|t| {
                let v = match t {
                    TextAlignment::Auto => "auto",
                    TextAlignment::Baseline => "baseline",
                    TextAlignment::Top => "top",
                    TextAlignment::Center => "center",
                    TextAlignment::Bottom => "bottom",
                };
                val(s, &v)
            })
            .is_some(),
        "outlineLvl" => p.outline_level.map(|v| val(s, &v)).is_some(),
        COLLAPSED => p
            .collapsed
            .map(|on| {
                if !on {
                    s.attr(W15, "val", &"0");
                }
            })
            .is_some(),
        _ => false,
    }
}

/// A frame's attributes, in the order the frame commands write them.
fn frame_attrs(f: &FramePr, s: &mut dyn Sink) {
    let text = |s: &mut dyn Sink, local: &'static str, v: &Option<String>| {
        if let Some(v) = v {
            s.attr(W, local, &v.as_str());
        }
    };
    let length = |s: &mut dyn Sink, local: &'static str, v: Option<f64>| {
        if let Some(v) = v {
            s.attr(W, local, &Twips(v));
        }
    };
    text(s, "dropCap", &f.drop_cap);
    if let Some(n) = f.lines {
        s.attr(W, "lines", &n);
    }
    length(s, "w", f.w);
    length(s, "h", f.h);
    text(s, "hRule", &f.h_rule);
    for (local, v) in [("hSpace", f.h_space), ("vSpace", f.v_space)] {
        if v == 0.0 {
            s.implied(local, "0");
        } else {
            s.attr(W, local, &Twips(v));
        }
    }
    text(s, "wrap", &f.wrap);
    text(s, "vAnchor", &f.v_anchor);
    text(s, "hAnchor", &f.h_anchor);
    text(s, "xAlign", &f.x_align);
    length(s, "x", f.x);
    text(s, "yAlign", &f.y_align);
    length(s, "y", f.y);
}

// Serialize

/// `p` as a `w:pPr` in the part `cx` describes, with the children other nodes own (`owned`) in their places; a theme
/// colour's value is written as `colours` resolve it.
pub fn serialize(p: &ParaProps, owned: Owned, colours: &ThemeColours, cx: &Cx) -> String {
    let mut out = Out::new(cx, colours);
    let mut decls = String::new();
    out.declarations(p.foreign.as_deref(), &mut decls);
    let w = &cx.w;
    let _ = write!(out.s, "<{w}pPr{decls}");
    out.free_attrs(p.foreign.as_deref().map_or(&[][..], |f| &f.attributes), None, &[]);
    out.s.push('>');
    write_children(&mut out, p, owned);
    let _ = write!(out.s, "</{w}pPr>");
    out.s
}

/// CT_PPr's children of `p`, in schema order, with the children other nodes own where they go.
fn write_children(out: &mut Out, p: &ParaProps, owned: Owned) {
    let foreign = p.foreign.as_deref();
    let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(|c| rank_of(&c.name.ns, &c.name.local)).collect());
    for (i, &key) in ORDER.iter().enumerate() {
        if key == "pPrChange" {
            foreign_children(out, foreign, &ranks, None);
        }
        if OWNED.contains(&key) {
            if let Some(x) = owned.get(key) {
                out.s.push_str(x);
            }
            foreign_children(out, foreign, &ranks, Some(i));
        } else {
            foreign_children(out, foreign, &ranks, Some(i));
            modelled_child(out, p, key);
        }
    }
}

fn foreign_children(out: &mut Out, foreign: Option<&ForeignProps>, ranks: &[Option<usize>], at: Option<usize>) {
    let Some(f) = foreign else { return };
    for (c, _) in f.children.iter().zip(ranks).filter(|(_, r)| **r == at) {
        out.payload(c);
    }
}

/// The name of kept child `c` among [`ORDER`]'s.
fn key_of_child(c: &ForeignChild) -> Option<&'static str> {
    rank_of(&c.name.ns, &c.name.local).map(|k| ORDER[k])
}

/// Modelled child `key` of `p`: as written when it was kept so and its value is still the one read from it.
fn modelled_child(out: &mut Out, p: &ParaProps, key: &'static str) {
    let kept = p.foreign.as_deref().and_then(|f| f.kept.iter().rev().find(|c| key_of_child(c) == Some(key)));
    if let Some(c) = kept {
        let at = out.s.len();
        typed_child(out, p, key);
        let now = out.s.split_off(at);
        let same = read_as(c).is_some_and(|q| {
            let at = out.s.len();
            typed_child(out, &q, key);
            out.s.split_off(at) == now
        });
        if same {
            out.payload(c);
        } else {
            out.s.push_str(&now);
        }
        return;
    }
    typed_child(out, p, key);
}

/// The typed value read from kept child `c` alone.
fn read_as(c: &ForeignChild) -> Option<ParaProps> {
    let mut wrapper = String::from("<x");
    for (p, u) in c.bindings.iter() {
        wrapper.push_str(if p.is_empty() { " xmlns" } else { " xmlns:" });
        wrapper.push_str(p);
        wrapper.push_str("=\"");
        super::prop_xml::push_esc(&mut wrapper, u);
        wrapper.push('"');
    }
    let _ = write!(wrapper, ">{}</x>", c.payload);
    let doc = roxmltree::Document::parse(&wrapper).ok()?;
    Some(parse(doc.root_element()))
}

/// Modelled child `key` of `p` from its typed value.
fn typed_child(out: &mut Out, p: &ParaProps, key: &'static str) {
    let foreign = p.foreign.as_deref().map_or(&[][..], |f| &f.attributes);
    let (ns, local) = match key {
        COLLAPSED => (W15, "collapsed"),
        _ => (W, key),
    };
    let at = out.s.len();
    let prefix = out.prefix(ns);
    let decl = if ns == W15 { out.w15_decl } else { "" };
    let _ = write!(out.s, "<{prefix}{local}{decl}");
    let mut present = false;
    let written = out.attrs(ns, foreign, Some((ns, local)), |s| present = emit(p, key, s));
    if !present {
        out.s.truncate(at);
        return;
    }
    out.free_attrs(foreign, Some((ns, local)), &written);
    let w = out.cx.w.clone();
    match key {
        "tabs" => {
            out.s.push('>');
            for t in &p.tabs {
                let kind = match t.kind {
                    TabKind::Left => "left",
                    TabKind::Center => "center",
                    TabKind::Right => "right",
                    TabKind::Decimal => "decimal",
                    TabKind::Bar => "bar",
                    TabKind::Num => "num",
                    TabKind::Clear => "clear",
                };
                let leader = match t.leader {
                    Leader::None => "",
                    Leader::Dot => "dot",
                    Leader::Hyphen => "hyphen",
                    Leader::Underscore => "underscore",
                    Leader::Heavy => "heavy",
                    Leader::MiddleDot => "middleDot",
                };
                let _ = write!(out.s, "<{w}tab {w}val=\"{kind}\"");
                if !leader.is_empty() {
                    let _ = write!(out.s, " {w}leader=\"{leader}\"");
                }
                let _ = write!(out.s, " {w}pos=\"{}\"/>", (t.pos * 20.0).round() as i64);
            }
            let _ = write!(out.s, "</{w}tabs>");
        }
        "numPr" => {
            out.s.push('>');
            if let Some(l) = p.list_level {
                let _ = write!(out.s, "<{w}ilvl {w}val=\"{l}\"/>");
            }
            if let Some(c) = p.list {
                let id = match c {
                    ListChoice::None => 0,
                    ListChoice::In(id) => id.0,
                };
                let _ = write!(out.s, "<{w}numId {w}val=\"{id}\"/>");
            }
            let _ = write!(out.s, "</{w}numPr>");
        }
        "pBdr" => {
            out.s.push('>');
            let b = &p.borders;
            for (side, v) in [("top", &b.top), ("left", &b.left), ("bottom", &b.bottom), ("right", &b.right), ("between", &b.between)] {
                let Some(v) = v else { continue };
                let _ = write!(out.s, "<{w}{side}");
                let _ = out.attrs(W, &[], None, |s| border_attrs(s, v));
                out.s.push_str("/>");
            }
            let _ = write!(out.s, "</{w}pBdr>");
        }
        _ => out.s.push_str("/>"),
    }
}

/// `p` as a `w:pPr` in the part `cx` describes, written as an edit of `origin` (the `w:pPr` the properties it holds
/// were read from, and those properties) would: each child the change leaves alone as written there, each it changes
/// in its place, a new one in schema order after any of its name, one it drops removed, the children other nodes own
/// as they are there; with no origin, as an edit of no `w:pPr`. A changed child is written whole in its place, or,
/// when `set` sets it in part, as its attributes set on it. `None` when nothing is left.
pub fn written_over(p: &ParaProps, colours: &ThemeColours, cx: &Cx, origin: Option<(&ParaProps, &str)>, set: &[ParaSet]) -> Option<String> {
    use super::write::props::Props;
    let foreign = |q: &ParaProps| q.foreign.as_ref().is_some_and(|f| !f.children.is_empty() || !f.attributes.is_empty() || !f.kept.is_empty());
    if origin.is_none() && foreign(p) {
        return Some(serialize(p, Owned::default(), colours, cx));
    }
    let empty = ParaProps::default();
    let (read, xml) = origin.unwrap_or((&empty, ""));
    let mut props = Props::parse(origin.map(|_| xml), "pPr", cx);
    let mut out = Out::new(cx, colours);
    for &key in ORDER.iter().filter(|k| !OWNED.contains(k)) {
        let mut render = |q: &ParaProps| {
            modelled_child(&mut out, q, key);
            std::mem::take(&mut out.s)
        };
        let (was, now) = (render(read), render(p));
        let whole = set.iter().any(|s| s.whole && key_of(s.prop) == key);
        let parts: Vec<ParaProp> = set.iter().filter(|s| !s.whole && key_of(s.prop) == key).map(|s| s.prop).collect();
        if was == now && !whole && parts.is_empty() {
            continue;
        }
        if now.is_empty() {
            props.remove(key);
            continue;
        }
        if !whole && !parts.is_empty() {
            let attrs: Vec<(&str, Option<String>)> = parts.iter().flat_map(|q| part_attrs(*q, p)).collect();
            props.attrs(key, &attrs, cx);
            continue;
        }
        props.set(key, now);
    }
    let children = |q: &ParaProps| {
        q.foreign.as_ref().map_or_else(Vec::new, |f| f.children.iter().map(|c| (props_key(&c.name), c.payload.clone())).collect::<Vec<_>>())
    };
    let (had, mut want) = (children(read), children(p));
    if had != want {
        for (key, text) in had {
            match want.iter().position(|w| w.0 == key && w.1 == text) {
                Some(k) => {
                    want.remove(k);
                }
                None => {
                    if let Some(k) = props.kids.iter().position(|(n, x)| *n == key && **x == *text) {
                        props.kids.remove(k);
                    }
                }
            }
        }
        for (key, text) in want {
            props.add(&key, text.to_string());
        }
    }
    props.xml()
}

/// The attributes property `prop`, set in part, sets on its child, each with its value in `p` (written as the
/// serializer writes it) or removed, the other names of the value removed.
fn part_attrs(prop: ParaProp, p: &ParaProps) -> Vec<(&'static str, Option<String>)> {
    let length = |v: Option<f64>| v.map(|v| Twips(v).to_string());
    let on = |b: Option<bool>| b.map(|b| if b { "1" } else { "0" }.to_string());
    match prop {
        ParaProp::IndentLeft => vec![("left", length(p.indent_left)), ("start", None)],
        ParaProp::IndentRight => vec![("right", length(p.indent_right)), ("end", None)],
        ParaProp::IndentFirst => match p.indent_first {
            Some(v) if v.is_sign_negative() => vec![("hanging", length(Some(-v))), ("firstLine", None)],
            v => vec![("firstLine", length(v)), ("hanging", None)],
        },
        ParaProp::SpaceBefore => vec![("before", length(p.space_before))],
        ParaProp::SpaceBeforeAuto => vec![("beforeAutospacing", on(p.space_before_auto))],
        ParaProp::SpaceAfter => vec![("after", length(p.space_after))],
        ParaProp::SpaceAfterAuto => vec![("afterAutospacing", on(p.space_after_auto))],
        ParaProp::Line => vec![("line", p.line.map(|v| v.to_string()))],
        ParaProp::LineRule => vec![(
            "lineRule",
            p.line_rule.map(|r| {
                match r {
                    LineRule::Auto => "auto",
                    LineRule::Exact => "exact",
                    LineRule::AtLeast => "atLeast",
                }
                .to_string()
            }),
        )],
        _ => Vec::new(),
    }
}

/// The child of CT_PPr a property is written in.
fn key_of(p: ParaProp) -> &'static str {
    match p {
        ParaProp::Style => "pStyle",
        ParaProp::Align => "jc",
        ParaProp::IndentLeft | ParaProp::IndentRight | ParaProp::IndentFirst => "ind",
        ParaProp::SpaceBefore | ParaProp::SpaceBeforeAuto | ParaProp::SpaceAfter | ParaProp::SpaceAfterAuto | ParaProp::Line | ParaProp::LineRule => {
            "spacing"
        }
        ParaProp::Flag(f) => FLAGS.iter().find(|(_, x)| *x == f).map_or("keepNext", |(n, _)| n),
        ParaProp::TightWrap => "textboxTightWrap",
        ParaProp::Division => "divId",
        ParaProp::Tabs => "tabs",
        ParaProp::List => "numPr",
        ParaProp::Borders => "pBdr",
        ParaProp::Shading => "shd",
        ParaProp::Frame => "framePr",
        ParaProp::TextFlow => "textDirection",
        ParaProp::TextAlignment => "textAlignment",
        ParaProp::OutlineLevel => "outlineLvl",
        ParaProp::Collapsed => COLLAPSED,
    }
}

/// What an edit of `w:pPr` XML set, as typed properties' record of it: each child that differs between `was` and
/// `now` (both in the part `cx` describes), set whole when `now` writes it as `p` does, else, for the indents and
/// spacing, the attributes it changed in part, in the order the edit set them.
#[cfg(test)]
pub(crate) fn inferred_set(p: &ParaProps, was: &str, now: &str, colours: &ThemeColours, cx: &Cx) -> Vec<ParaSet> {
    use super::write::props::Props;
    let (old, new) = (Props::parse(Some(was), "pPr", cx), Props::parse(Some(now), "pPr", cx));
    let mut out = Out::new(cx, colours);
    let mut set = Vec::new();
    for &key in ORDER.iter().filter(|k| !OWNED.contains(k)) {
        let (a, b) = (old.get(key), new.get(key));
        if a == b {
            continue;
        }
        modelled_child(&mut out, p, key);
        let typed = std::mem::take(&mut out.s);
        let whole = || {
            let prop = match key {
                "ind" => ParaProp::IndentLeft,
                "spacing" => ParaProp::SpaceBefore,
                _ => FLAGS.iter().find(|(n, _)| *n == key).map_or_else(|| prop_of(key), |(_, f)| ParaProp::Flag(*f)),
            };
            ParaSet { prop, whole: true }
        };
        let (Some(a), Some(b)) = (a, b) else {
            set.push(whole());
            continue;
        };
        if b == typed || !matches!(key, "ind" | "spacing") {
            set.push(whole());
            continue;
        }
        let attrs = |x: &str| super::prop_xml::attr_list(x).map(|(_, l)| l).unwrap_or_default();
        let (was_attrs, now_attrs) = (attrs(a), attrs(b));
        let local = |q: &str| q.rsplit(':').next().unwrap_or(q).to_string();
        let mut changed: Vec<String> = now_attrs.iter().filter(|n| !was_attrs.contains(n)).map(|(q, _)| local(q)).collect();
        changed.extend(was_attrs.iter().filter(|w| !now_attrs.iter().any(|n| local(&n.0) == local(&w.0))).map(|(q, _)| local(q)));
        for name in changed {
            let prop = match name.as_str() {
                "left" | "start" => ParaProp::IndentLeft,
                "right" | "end" => ParaProp::IndentRight,
                "firstLine" | "hanging" => ParaProp::IndentFirst,
                "before" => ParaProp::SpaceBefore,
                "beforeAutospacing" => ParaProp::SpaceBeforeAuto,
                "after" => ParaProp::SpaceAfter,
                "afterAutospacing" => ParaProp::SpaceAfterAuto,
                "line" => ParaProp::Line,
                "lineRule" => ParaProp::LineRule,
                _ => continue,
            };
            if !set.iter().any(|s: &ParaSet| s.prop == prop) {
                set.push(ParaSet { prop, whole: false });
            }
        }
    }
    set
}

/// The property written in child `key`, one that isn't an on/off flag.
#[cfg(test)]
fn prop_of(key: &str) -> ParaProp {
    use ParaProp::*;
    [Style, Align, TightWrap, Division, Tabs, List, Borders, Shading, Frame, TextFlow, TextAlignment, OutlineLevel, Collapsed]
        .into_iter()
        .find(|p| key_of(*p) == key)
        .unwrap_or(Style)
}

/// What `p` keeps beside each modelled child `set` sets whole, or that it changes and doesn't set in part: nothing, as
/// writing the child anew keeps nothing of it as written; beside one set in part, no value written otherwise.
fn drop_replaced(was: &ParaProps, p: &mut ParaProps, set: &[ParaSet]) {
    let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\" xmlns:w15=\"{W15}\">"));
    let colours = ThemeColours::default();
    let mut out = Out::new(&cx, &colours);
    let mut render = |q: &ParaProps, key: &'static str| {
        modelled_child(&mut out, q, key);
        std::mem::take(&mut out.s)
    };
    let replaced: Vec<(&str, &str)> = ORDER
        .iter()
        .filter(|k| {
            let by = |whole: bool| set.iter().any(|s| s.whole == whole && key_of(s.prop) == **k);
            !OWNED.contains(k) && (by(true) || !by(false) && render(was, k) != render(p, k))
        })
        .map(|k| if *k == COLLAPSED { (W15, "collapsed") } else { (W, *k) })
        .collect();
    if replaced.is_empty() {
        return;
    }
    let Some(f) = p.foreign.as_mut() else { return };
    let f = Rc::make_mut(f);
    let of = |q: &QName| replaced.contains(&(&*q.ns, &*q.local));
    let part: Vec<&str> = set.iter().filter(|s| !s.whole).map(|s| key_of(s.prop)).collect();
    let in_part = |q: &QName| part.iter().any(|k| &*q.ns == W && &*q.local == *k);
    // What it held as read goes; what the edit gave it stays
    let had = was.foreign.as_deref();
    f.attributes
        .retain(|a| !a.on.as_ref().is_some_and(|q| of(q) || in_part(q) && a.stands_for.is_some()) || !had.is_some_and(|h| h.attributes.contains(a)));
    f.kept.retain(|c| !of(&c.name) || !had.is_some_and(|h| h.kept.contains(c)));
    if *f == ForeignProps::default() {
        p.foreign = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::format::ShadingPattern;
    use crate::document::line::BorderStyle;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" xmlns:x="urn:x""#;

    fn cx() -> Cx {
        Cx::of_xml(&format!("<w:document {NS}>"))
    }

    fn read(x: &str) -> ParaProps {
        cx().parse(x, |_, el| parse(el)).expect("parses")
    }

    /// `x` written again, with what the writing lost of it.
    fn rewritten(x: &str, edit: impl FnOnce(&mut ParaProps)) -> (String, Vec<String>) {
        let cx = cx();
        let mut p = read(x);
        edit(&mut p);
        let xml = serialize(&p, Owned::default(), &ThemeColours::default(), &cx);
        let lost = cx.parse(x, |_, had| cx.parse(&xml, |_, has| super::super::char_props::tests::lost(had, has))).flatten().expect("parses");
        (xml, lost)
    }

    #[test]
    fn unset_off_and_explicit_stay_apart() {
        let p = read(r#"<w:pPr><w:keepNext w:val="0"/><w:widowControl/><w:numPr><w:numId w:val="0"/></w:numPr><w:ind w:left="0"/></w:pPr>"#);
        assert_eq!(p.flags.get(ParaFlag::KEEP_NEXT), Some(false));
        assert_eq!(p.flags.get(ParaFlag::WIDOW_CONTROL), Some(true));
        assert_eq!(p.flags.get(ParaFlag::KEEP_LINES), None);
        assert_eq!((p.list, p.list_level), (Some(ListChoice::None), None));
        assert_eq!((p.indent_left, p.indent_right), (Some(0.0), None));
    }

    #[test]
    fn unmodelled_children_and_attributes_are_kept_in_their_place() {
        let x = r#"<w:pPr x:a="1"><w:pStyle w:val="Body"/><w:adjustRightInd w:val="0"/><w:ind w:left="720" w:leftChars="100" x:b="2"/><w:cnfStyle w:val="100000000000"/><x:mine/></w:pPr>"#;
        let (xml, lost) = rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let (xml, lost) = rewritten(x, |p| p.align = Some(Jc::Center));
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let order: Vec<&str> = ["pStyle", "adjustRightInd", "w:ind", "jc", "cnfStyle", "x:mine"].into_iter().collect();
        let at: Vec<usize> =
            order.iter().map(|n| xml.find(&format!("<{}", if n.contains(':') { n.to_string() } else { format!("w:{n}") })).expect(n)).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "schema order: {xml}");
    }

    #[test]
    fn values_written_otherwise_are_written_as_read_until_they_change() {
        let x = r#"<w:pPr><w:keepNext w:val="true"/><w:ind w:start="720" w:end="1in"/><w:jc w:val="start"/></w:pPr>"#;
        let (xml, lost) = rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        assert!(xml.contains(r#"w:start="720""#) && xml.contains(r#"w:end="1in""#) && xml.contains(r#"w:val="start""#), "{xml}");
        let (xml, _) = rewritten(x, |p| (p.indent_left, p.align) = (Some(18.0), Some(Jc::Right)));
        assert!(xml.contains(r#"w:left="360""#) && !xml.contains("w:start") && xml.contains(r#"w:val="right""#), "{xml}");
    }

    #[test]
    fn children_with_content_are_written_as_read_until_they_change() {
        let x = r#"<w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="3"/><w:numberingChange w:id="9" w:original=""/></w:numPr><w:pBdr><w:start w:val="single" w:sz="4" w:space="1" w:color="auto"/><w:bar w:val="single"/></w:pBdr><w:tabs><w:tab w:val="left" w:pos="720" x:q="1"/></w:tabs></w:pPr>"#;
        let p = read(x);
        assert_eq!(p.borders.left.map(|b| b.style), Some(BorderStyle::Single));
        assert_eq!((p.list, p.list_level), (Some(ListChoice::In(ListId(3))), Some(1)));
        let (xml, lost) = rewritten(x, |_| {});
        assert_eq!(lost, Vec::<String>::new(), "{xml}");
        let (xml, _) = rewritten(x, |p| p.list = Some(ListChoice::In(ListId(4))));
        assert!(xml.contains(r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="4"/></w:numPr>"#), "{xml}");
    }

    #[test]
    fn the_children_other_nodes_own_go_where_the_schema_places_them() {
        let p = ParaProps {
            style: Some("Body".into()),
            outline_level: Some(1),
            collapsed: Some(false),
            shading: Some(Shading { pattern: ShadingPattern::Clear, color: None, fill: Some(crate::format::Color::Rgb(0xFF0000)) }),
            ..Default::default()
        };
        let owned = Owned {
            mark: Some("<w:rPr><w:b/></w:rPr>"),
            section: Some("<w:sectPr/>"),
            change: Some("<w:pPrChange w:id=\"1\"><w:pPr/></w:pPrChange>"),
        };
        let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">"));
        let xml = serialize(&p, owned, &ThemeColours::default(), &cx);
        assert_eq!(
            xml,
            format!(
                r#"<w:pPr><w:pStyle w:val="Body"/><w:shd w:val="clear" w:fill="FF0000"/><w:outlineLvl w:val="1"/><w:rPr><w:b/></w:rPr><w:sectPr/><w:collapsed xmlns:w15="{W15}" w15:val="0"/><w:pPrChange w:id="1"><w:pPr/></w:pPrChange></w:pPr>"#
            )
            .replace("<w:collapsed", "<w15:collapsed")
        );
    }

    #[test]
    fn an_edit_writes_over_the_xml_it_was_read_from() {
        let cx = cx();
        let x = r#"<w:pPr>  <w:pStyle w:val="Body"/><w:ind w:leftChars="200" w:left="720" w:hanging="360"/><w:rPr><w:i/></w:rPr></w:pPr>"#;
        let read = read(x);
        let mut p = read.clone();
        p.indent_left = Some(72.0);
        let set = [ParaSet { prop: ParaProp::IndentLeft, whole: false }];
        let xml = written_over(&p, &ThemeColours::default(), &cx, Some((&read, x)), &set).expect("properties");
        let mut props = super::super::write::props::Props::parse(Some(x), "pPr", &cx);
        props.attrs("ind", &[("left", Some("1440".into())), ("start", None)], &cx);
        assert_eq!(Some(xml), props.xml(), "as the XML patch writes it");
    }
}
