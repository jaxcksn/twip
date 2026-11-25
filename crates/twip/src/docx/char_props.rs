//! Typed run properties from and to `w:rPr`
//!
//! The parse keeps theme references as references and keeps in [`ForeignProps`] what the typed value does not hold,
//! so that writing the value again loses nothing of what was read:
//!
//! - unmodelled children whole, with the namespace bindings their names use, and earlier duplicates of a modelled
//!   child (the last one counts; for a revision, the first);
//! - a modelled child its value doesn't keep (a size that isn't a number) whole;
//! - unmodelled attributes of modelled children and of the element itself, and modelled attributes that didn't decide
//!   the value (a font's name beside its theme font, a colour's value beside its theme colour), the latter written
//!   while the attribute that decided it is;
//! - modelled attributes written otherwise than the writer writes their value (`w:val="true"`, `ff0000`, an invalid
//!   literal, read as the value layout falls back to), written as read while the value is still the one read;
//! - children with content the parse doesn't read all of (text effects, stylistic sets, a child with children of its
//!   own) whole beside their typed value, written as read while the typed value writes as the one read from them did.
//!
//! Revision children are not formatting: they come back beside the properties ([`RprRevisions`]).
//!
//! The serializer writes the mark's revisions, CT_RPr's children in schema order with Word 2010's after them, then
//! `w:rPrChange`, in the destination part's prefixes. An unmodelled child goes at its schema place when CT_RPr names
//! it (before the modelled one of its name; after it for a revision), else after the modelled children (before
//! `w:rPrChange`), in source order. A binding an unmodelled name needs and the part's root lacks is declared on the
//! `w:rPr`, or on the child itself when its prefix is one the writer uses for another namespace.

use std::fmt::{Display, Write as _};
use std::rc::Rc;

use roxmltree::Node;

use super::line_style::{border_style, border_val, emphasis, emphasis_val, shading_pattern, shading_val, underline, underline_val};
use super::model::ThemeColourRef;
use super::props::{self as xml_props, RunProps, ThemeColours, theme_name, theme_ref};
use super::write::props::{Cx, RPR};
use super::xml::{W, W14, attr, color, half_points, int, num, on_off, twips, val};
use crate::document::format::VertAlign;
use crate::format::{
    CharEdit, CharProps, Color, Effect, FontHint, FontRef, ForeignAttr, ForeignChild, ForeignProps, Highlight, LangProps, NumberForm, NumberSpacing,
    Prop, QName, ScriptClass, ScriptFonts, Set, Shading, TextBorder, ThemeFace, Toggle, Toggles, TypographyProps, UnderlineProps,
};

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// Word 2010's namespace declared on an element of it.
const W14_DECL: &str = " xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"";

/// A revision element's identity as written, and its other attributes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RevisionAttrs {
    /// `w:id`.
    pub id: Option<Rc<str>>,
    /// `w:author`.
    pub author: Option<Rc<str>>,
    /// `w:date`.
    pub date: Option<Rc<str>>,
    /// Its unmodelled attributes, and the modelled ones written otherwise.
    pub foreign: Vec<ForeignAttr>,
}

/// `w:rPrChange`: the change's identity and the properties before it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormatChangeProps {
    /// Who made the change, and when.
    pub attrs: RevisionAttrs,
    /// The properties before the change.
    pub previous: CharProps,
}

/// The revision children of a `w:rPr`: a paragraph mark's insertion, deletion and moves, and a formatting change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RprRevisions {
    /// `w:ins`.
    pub ins: Option<RevisionAttrs>,
    /// `w:del`.
    pub del: Option<RevisionAttrs>,
    /// `w:moveFrom`.
    pub move_from: Option<RevisionAttrs>,
    /// `w:moveTo`.
    pub move_to: Option<RevisionAttrs>,
    /// `w:rPrChange`.
    pub change: Option<Box<FormatChangeProps>>,
}

impl RprRevisions {
    /// Whether it holds no revision.
    pub fn is_empty(&self) -> bool {
        self.ins.is_none() && self.del.is_none() && self.move_from.is_none() && self.move_to.is_none() && self.change.is_none()
    }
}

/// A run's authored properties: typed, with the `w:rPr` they were read from (or last written as) beside them. The
/// written form stands for the properties only while they are the ones it was read as; never part of equality.
#[derive(Clone, Debug, Default)]
pub struct RunDirect {
    props: Option<Rc<CharProps>>,
    origin: Option<(Rc<CharProps>, Rc<str>)>,
    /// What edits since reading set, in order: written again over the `w:rPr` as read.
    set: Vec<Set>,
}

impl RunDirect {
    /// Properties read from `xml`, the `w:rPr` they were parsed from.
    pub fn read(props: CharProps, xml: Rc<str>) -> RunDirect {
        let props = Rc::new(props);
        RunDirect { origin: Some((props.clone(), xml)), props: Some(props), set: Vec::new() }
    }

    /// The properties of `xml`, a `w:rPr` with the main namespace bound to `w`.
    #[cfg(test)]
    pub(crate) fn in_w(xml: &str) -> RunDirect {
        RunDirect::from_xml(Some(xml), &Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\">")))
    }

    /// Properties read from `xml`, shared with other runs read alike.
    pub fn shared(props: Rc<CharProps>, xml: Rc<str>) -> RunDirect {
        RunDirect { origin: Some((props.clone(), xml)), props: Some(props), set: Vec::new() }
    }

    /// Typed properties with no written form.
    pub fn typed(props: Option<Rc<CharProps>>) -> RunDirect {
        RunDirect { props, origin: None, set: Vec::new() }
    }

    /// The properties `xml` (a `w:rPr` in the part `cx` describes) holds; none for no XML, and nothing typed (the
    /// XML kept as written) for XML that doesn't parse.
    pub fn from_xml(xml: Option<&str>, cx: &Cx) -> RunDirect {
        let Some(x) = xml else { return RunDirect::default() };
        RunDirect::read(cx.parse(x, |_, el| parse(el).0).unwrap_or_default(), x.into())
    }

    /// The typed properties; `None`: none of its own.
    pub fn props(&self) -> Option<&Rc<CharProps>> {
        self.props.as_ref()
    }

    /// The `w:rPr` as read, while the properties are the ones read from it.
    pub fn as_read(&self) -> Option<&Rc<str>> {
        let (read, xml) = self.origin.as_ref()?;
        let props = self.props.as_ref()?;
        (Rc::ptr_eq(read, props) || read == props).then_some(xml)
    }

    /// The `w:rPr` to write in the part `cx` describes: as read while the properties are those, else written from
    /// them (a theme colour's value as `colours` resolve it).
    pub fn xml(&self, colours: &ThemeColours, cx: &Cx) -> Option<std::borrow::Cow<'_, str>> {
        if let Some(x) = self.as_read() {
            return Some(std::borrow::Cow::Borrowed(x));
        }
        let p = self.props.as_ref()?;
        let origin = self.origin.as_ref().map(|(read, xml)| (&**read, &**xml));
        written_over(p, &RprRevisions::default(), colours, cx, origin, &self.set).map(std::borrow::Cow::Owned)
    }

    /// `props` in place of these properties, set by an edit that set `set`, in order: the `w:rPr` they were read from
    /// kept to write them over, what was set written again.
    pub fn edited(&self, props: Option<CharProps>, set: &[Set]) -> RunDirect {
        let mut all = self.set.clone();
        all.extend_from_slice(set);
        let props = props.map(|mut p| {
            if let Some(was) = self.props.as_deref().filter(|_| p.foreign.is_some()) {
                drop_replaced(was, &mut p, &all);
            }
            Rc::new(p)
        });
        RunDirect { props, origin: self.origin.clone(), set: all }
    }

    /// An edit's result as these properties' successor.
    pub fn edit(&self, e: CharEdit) -> RunDirect {
        let props = (e.props != CharProps::default()).then_some(e.props);
        self.edited(props, &e.set)
    }

    /// These properties where `cx` describes the part they are written in: `None` while the `w:rPr` they were read
    /// from reads the same there, else without it, to be written from the properties.
    pub fn moved_to(&self, cx: &Cx) -> Option<RunDirect> {
        let (read, xml) = self.origin.as_ref()?;
        if cx.parse(xml, |_, el| parse(el).0).is_some_and(|p| p == **read) {
            return None;
        }
        Some(RunDirect { props: self.props.clone(), origin: None, set: self.set.clone() })
    }

    /// The properties and the `w:rPr` they were read from, while they are still those.
    pub fn read_from(&self) -> Option<(&Rc<CharProps>, &Rc<str>)> {
        let xml = self.as_read()?;
        Some((self.props.as_ref()?, xml))
    }

    /// Whether a paragraph mark with these properties is a style separator (its `w:specVanish` on).
    pub fn spec_vanish(&self) -> bool {
        self.props.as_ref().is_some_and(|p| p.spec_vanish == Some(true))
    }

    /// Authored equality: the typed properties.
    pub fn authored_eq(&self, o: &RunDirect) -> bool {
        match (&self.props, &o.props) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a == b,
            (None, None) => true,
            _ => false,
        }
    }
}

/// `p` with WordprocessingML on/off child `local`, which the typed properties don't model, on, as the part `cx`
/// describes writes it.
pub fn put_unmodelled_flag(p: &mut CharProps, local: &str, cx: &Cx) {
    drop_unmodelled(p, local);
    let child = ForeignChild {
        name: QName { ns: W.into(), local: local.into() },
        payload: format!("<{}{local}/>", cx.w).into(),
        bindings: Rc::from([(Rc::from(cx.w.trim_end_matches(':')), Rc::from(W))]),
    };
    Rc::make_mut(p.foreign.get_or_insert_with(Default::default)).children.push(child);
}

/// `p` without WordprocessingML child `local`, which the typed properties don't model.
pub fn drop_unmodelled(p: &mut CharProps, local: &str) {
    let Some(f) = p.foreign.as_mut() else { return };
    if !f.children.iter().any(|c| &*c.name.ns == W && &*c.name.local == local) {
        return;
    }
    let f = Rc::make_mut(f);
    f.children.retain(|c| !(&*c.name.ns == W && &*c.name.local == local));
    if f.children.is_empty() && f.attributes.is_empty() && f.kept.is_empty() {
        p.foreign = None;
    }
}

/// The on/off children and their toggles.
const TOGGLES: [(&str, Toggle); 12] = [
    ("b", Toggle::BOLD),
    ("i", Toggle::ITALIC),
    ("caps", Toggle::CAPS),
    ("smallCaps", Toggle::SMALL_CAPS),
    ("strike", Toggle::STRIKE),
    ("vanish", Toggle::HIDDEN),
    ("bCs", Toggle::BOLD_COMPLEX),
    ("iCs", Toggle::ITALIC_COMPLEX),
    ("emboss", Toggle::EMBOSS),
    ("imprint", Toggle::IMPRINT),
    ("outline", Toggle::OUTLINE),
    ("shadow", Toggle::SHADOW),
];

const HIGHLIGHTS: [(&str, Highlight); 16] = {
    use Highlight::*;
    [
        ("yellow", Yellow),
        ("green", Green),
        ("cyan", Cyan),
        ("magenta", Magenta),
        ("blue", Blue),
        ("red", Red),
        ("darkBlue", DarkBlue),
        ("darkCyan", DarkCyan),
        ("darkGreen", DarkGreen),
        ("darkMagenta", DarkMagenta),
        ("darkRed", DarkRed),
        ("darkYellow", DarkYellow),
        ("darkGray", DarkGray),
        ("lightGray", LightGray),
        ("black", Black),
        ("white", White),
    ]
};

/// `ST_Ligatures` by its bits.
const LIGATURES: [&str; 16] = [
    "none",
    "standard",
    "contextual",
    "standardContextual",
    "historical",
    "standardHistorical",
    "contextualHistorical",
    "standardContextualHistorical",
    "discretional",
    "standardDiscretional",
    "contextualDiscretional",
    "standardContextualDiscretional",
    "historicalDiscretional",
    "standardHistoricalDiscretional",
    "contextualHistoricalDiscretional",
    "all",
];

/// The colour attributes of a colour: value, theme colour, tint, shade.
type ColourAttrs = [&'static str; 4];
const COLOUR: ColourAttrs = ["val", "themeColor", "themeTint", "themeShade"];
const LINE_COLOUR: ColourAttrs = ["color", "themeColor", "themeTint", "themeShade"];
const FILL: ColourAttrs = ["fill", "themeFill", "themeFillTint", "themeFillShade"];

const FONT_SLOTS: [(&str, &str); 4] = [("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme"), ("eastAsia", "eastAsiaTheme"), ("cs", "cstheme")];

/// Children whose content the parse reads; they are kept as written too.
const WHOLE: [&str; 6] = ["w14:textFill", "w14:textOutline", "w14:shadow", "w14:glow", "w14:reflection", "w14:stylisticSets"];

/// CT_RPr's children, counted.
const KEYS: usize = RPR.len();

fn is_revision(key: &str) -> bool {
    matches!(key, "ins" | "del" | "moveFrom" | "moveTo" | "rPrChange")
}

/// The place in CT_RPr's order of an element's name, if it has one.
fn rank_of(ns: &str, local: &str) -> Option<usize> {
    RPR.iter().position(|k| match k.strip_prefix("w14:") {
        Some(l) => ns == W14 && local == l,
        None => ns == W && local == *k,
    })
}

// Parse

/// A `w:rPr` as typed properties, and its revision children.
pub fn parse(rpr: Node) -> (CharProps, RprRevisions) {
    let mut rev = RprRevisions::default();
    let p = read(rpr, Some(&mut rev));
    (p, rev)
}

/// `rpr`'s properties; its revision children into `rev`, or (in a change's earlier properties, where none belong)
/// kept as unmodelled children.
fn read(rpr: Node, mut rev: Option<&mut RprRevisions>) -> CharProps {
    let mut p = CharProps::default();
    let mut f = ForeignProps::default();
    let mut scratch = String::new();
    Compare::new(rpr, &mut scratch).finish(None, &mut f.attributes);
    let elements = || rpr.children().filter(|c| c.is_element());
    // The child each modelled name is read from: a revision's first, any other's last.
    let mut pick = [None::<usize>; KEYS];
    for (i, e) in elements().enumerate() {
        if let Some(k) = rank_of(e.tag_name().namespace().unwrap_or_default(), e.tag_name().name())
            && (!is_revision(RPR[k]) || pick[k].is_none())
        {
            pick[k] = Some(i);
        }
    }
    for (i, e) in elements().enumerate() {
        let (ns, name) = (e.tag_name().namespace().unwrap_or_default(), e.tag_name().name());
        let Some(key) = rank_of(ns, name).filter(|k| pick[*k] == Some(i)).map(|k| RPR[k]) else {
            f.children.push(foreign_child(e));
            continue;
        };
        if is_revision(key) {
            match rev.as_deref_mut() {
                Some(r) => revision(r, e, key, &mut scratch),
                None => f.children.push(foreign_child(e)),
            }
            continue;
        }
        let modelled = if ns == W14 { w14(&mut p, e, name) } else { wml(&mut p, e, name) };
        if !modelled {
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

/// A WordprocessingML child into `p`; false when it isn't modelled.
fn wml(p: &mut CharProps, e: Node, name: &str) -> bool {
    if let Some(&(_, t)) = TOGGLES.iter().find(|(n, _)| *n == name) {
        p.toggles.put(t, on_off(e));
        return true;
    }
    match name {
        "rFonts" => {
            let font = |(name, theme): (&str, &str)| {
                let theme = if name == "cs" { attr(e, "csTheme").or_else(|| attr(e, theme)) } else { attr(e, theme) };
                match theme {
                    Some(t) => Some(theme_font(t)),
                    None => attr(e, name).filter(|s| !s.is_empty()).map(|s| FontRef::Family(s.into())),
                }
            };
            let [latin, latin_extended, east_asian, complex] = FONT_SLOTS.map(font);
            p.fonts = ScriptFonts { latin, latin_extended, east_asian, complex };
            p.hint = attr(e, "hint").map(hint);
        }
        "sz" => p.size = half_points(val(e)),
        "szCs" => p.complex_size = half_points(val(e)),
        "cs" => p.complex = Some(on_off(e)),
        "dstrike" => p.double_strike = Some(on_off(e)),
        "noProof" => p.no_proof = Some(on_off(e)),
        "rtl" => p.rtl = Some(on_off(e)),
        "specVanish" => p.spec_vanish = Some(on_off(e)),
        "u" => p.underline = Some(UnderlineProps { style: val(e).map_or(super::model::Underline::Single, underline), color: colour(e, LINE_COLOUR) }),
        "color" => p.color = Some(colour(e, COLOUR).unwrap_or(Color::Auto)),
        "highlight" => p.highlight = Some(val(e).and_then(|v| HIGHLIGHTS.iter().find(|(n, _)| *n == v)).map(|(_, h)| *h)),
        "shd" => {
            let pattern = val(e).map_or(crate::document::format::ShadingPattern::Clear, shading_pattern);
            p.shading = Some(Shading { pattern, color: colour(e, LINE_COLOUR), fill: colour(e, FILL) });
        }
        "bdr" => p.border = Some(border(e)),
        "em" => p.emphasis = Some(val(e).map_or(crate::document::format::Emphasis::None, emphasis)),
        "vertAlign" => {
            p.vertical = Some(match val(e) {
                Some("superscript") => VertAlign::Superscript,
                Some("subscript") => VertAlign::Subscript,
                _ => VertAlign::Baseline,
            })
        }
        "position" => p.position = half_points(val(e)),
        "spacing" => p.spacing = twips(val(e)),
        "w" => p.scale = num(val(e).map(|v| v.trim_end_matches('%'))),
        "kern" => p.kern = half_points(val(e)),
        "lang" => {
            let tag = |a: &str| attr(e, a).map(Rc::from);
            p.lang = LangProps { latin: tag("val"), east_asian: tag("eastAsia"), complex: tag("bidi") };
        }
        "rStyle" => p.style = val(e).map(Rc::from),
        "fitText" => p.fit_text = twips(val(e)).map(|w| (w, int(attr(e, "id")).unwrap_or(0))),
        _ => return false,
    }
    true
}

/// `w14:val`, else `w:val`.
fn val14<'a>(e: Node<'a, '_>) -> Option<&'a str> {
    e.attribute((W14, "val")).or_else(|| val(e))
}

/// A Word 2010 child into `p`; false when it isn't modelled.
fn w14(p: &mut CharProps, e: Node, name: &str) -> bool {
    let v = val14(e);
    let t = &mut p.typography;
    match name {
        "ligatures" => t.ligatures = Some(v.and_then(|v| LIGATURES.iter().position(|n| *n == v)).map_or(0, |k| k as u8)),
        "cntxtAlts" => t.contextual_alternates = Some(!matches!(v, Some("0" | "false" | "off"))),
        "numForm" => {
            t.number_form = Some(match v {
                Some("lining") => NumberForm::Lining,
                Some("oldStyle") => NumberForm::OldStyle,
                _ => NumberForm::Default,
            })
        }
        "numSpacing" => {
            t.number_spacing = Some(match v {
                Some("proportional") => NumberSpacing::Proportional,
                Some("tabular") => NumberSpacing::Tabular,
                _ => NumberSpacing::Default,
            })
        }
        "stylisticSets" => {
            let on = |s: &Node| !matches!(s.attribute((W14, "val")), Some("0" | "false" | "off"));
            let ids =
                e.children().filter(|s| super::xml::is(*s, W14, "styleSet") && on(s)).filter_map(|s| s.attribute((W14, "id"))?.parse::<u32>().ok());
            t.stylistic_sets = Some(ids.filter(|n| (1..=20).contains(n)).fold(0, |m, n| m | 1 << (n - 1)));
        }
        "textFill" | "textOutline" | "shadow" | "glow" | "reflection" => {
            use crate::drawing::colour::Ns;
            use crate::drawing::paint::{Fill, Glow, Line, OuterShadow, Reflection};
            let fx = Rc::make_mut(p.effects.get_or_insert_with(Default::default));
            match name {
                "textFill" => fx.fill = Fill::child_of_in(e, Ns::W14, &mut |_| None).or(fx.fill.take()),
                "textOutline" => fx.outline = Some(Line::parse_in(e, Ns::W14, &mut |_| None)),
                "shadow" => fx.shadow = Some(OuterShadow::parse(e, Ns::W14)),
                "glow" => fx.glow = Some(Glow::parse(e, Ns::W14)),
                _ => fx.reflection = Some(Reflection::parse(e, Ns::W14)),
            }
        }
        _ => return false,
    }
    true
}

/// A mark's revision child or a formatting change into `rev`.
fn revision(rev: &mut RprRevisions, e: Node, key: &str, scratch: &mut String) {
    let slot = match key {
        "ins" => &mut rev.ins,
        "del" => &mut rev.del,
        "moveFrom" => &mut rev.move_from,
        "moveTo" => &mut rev.move_to,
        _ => {
            let previous = e.children().find(|c| super::xml::is_w(*c, "rPr")).map(|r| read(r, None)).unwrap_or_default();
            rev.change = Some(Box::new(FormatChangeProps { attrs: revision_attrs(e, scratch), previous }));
            return;
        }
    };
    *slot = Some(revision_attrs(e, scratch));
}

fn revision_attrs(e: Node, scratch: &mut String) -> RevisionAttrs {
    let a = |n: &str| attr(e, n).map(Rc::from);
    let mut r = RevisionAttrs { id: a("id"), author: a("author"), date: a("date"), foreign: Vec::new() };
    let mut cmp = Compare::new(e, scratch);
    emit_revision(&r, &mut cmp);
    cmp.finish(None, &mut r.foreign);
    r
}

/// An element as written, with the bindings of the namespaces its names use.
fn foreign_child(e: Node) -> ForeignChild {
    let mut used: Vec<&str> = Vec::new();
    for d in e.descendants().filter(|d| d.is_element()) {
        let names = d.tag_name().namespace().into_iter().chain(d.attributes().filter_map(|a| a.namespace()));
        for ns in names {
            if !used.contains(&ns) {
                used.push(ns);
            }
        }
    }
    let bindings = e
        .namespaces()
        .filter(|b| b.uri() != XML_NS && used.contains(&b.uri()))
        .map(|b| (Rc::from(b.name().unwrap_or_default()), Rc::from(b.uri())))
        .collect();
    ForeignChild {
        name: QName { ns: e.tag_name().namespace().unwrap_or_default().into(), local: e.tag_name().name().into() },
        payload: e.document().input_text()[e.range()].into(),
        bindings,
    }
}

/// An optional colour from its attributes: a theme colour wins over the value; a value that isn't RGB is automatic.
fn colour(e: Node, [hex, theme, tint, shade]: ColourAttrs) -> Option<Color> {
    match theme_ref(e, theme, tint, shade) {
        Some(r) => Some(Color::Theme(r)),
        None => attr(e, hex).map(|v| color(Some(v)).map_or(Color::Auto, Color::Rgb)),
    }
}

fn border(e: Node) -> TextBorder {
    // Eighths of a point
    let sz = match num(attr(e, "sz")) {
        Some(v) if v > 0.0 && v < 1.0 => 1.0,
        Some(v) => v.clamp(0.0, 1584.0 * 8.0),
        None => 0.0,
    };
    TextBorder {
        style: val(e).map_or(crate::document::line::BorderStyle::None, border_style),
        width: sz / 8.0,
        space: num(attr(e, "space")).unwrap_or(0.0),
        shadow: matches!(attr(e, "shadow"), Some("1" | "true" | "on")),
        color: colour(e, LINE_COLOUR),
    }
}

/// An `ST_Theme` value: by its set's prefix and its script's suffix, as the theme resolves it.
fn theme_font(t: &str) -> FontRef {
    let face = if t.starts_with("major") {
        ThemeFace::Major
    } else if t.starts_with("minor") {
        ThemeFace::Minor
    } else {
        ThemeFace::Unrecognised
    };
    let script = if t.ends_with("EastAsia") {
        ScriptClass::EastAsian
    } else if t.ends_with("Bidi") {
        ScriptClass::Complex
    } else if t.ends_with("HAnsi") {
        ScriptClass::LatinExtended
    } else {
        ScriptClass::Latin
    };
    FontRef::Theme { face, script }
}

fn theme_font_name(face: ThemeFace, script: ScriptClass) -> &'static str {
    match (face, script) {
        (ThemeFace::Major, ScriptClass::Latin) => "majorAscii",
        (ThemeFace::Major, ScriptClass::LatinExtended) => "majorHAnsi",
        (ThemeFace::Major, ScriptClass::EastAsian) => "majorEastAsia",
        (ThemeFace::Major, ScriptClass::Complex) => "majorBidi",
        (_, ScriptClass::Latin) => "minorAscii",
        (_, ScriptClass::LatinExtended) => "minorHAnsi",
        (_, ScriptClass::EastAsian) => "minorEastAsia",
        (_, ScriptClass::Complex) => "minorBidi",
    }
}

fn hint(v: &str) -> FontHint {
    match v {
        "eastAsia" => FontHint::EastAsia,
        "cs" => FontHint::Complex,
        _ => FontHint::Default,
    }
}

/// `p` without its fonts: no font, no hint, and nothing kept of `w:rFonts`.
pub fn without_fonts(mut p: CharProps) -> CharProps {
    (p.fonts, p.hint) = (ScriptFonts::default(), None);
    if let Some(f) = p.foreign.take() {
        let mut f = Rc::unwrap_or_clone(f);
        let fonts = |q: &QName| &*q.ns == W && &*q.local == "rFonts";
        f.attributes.retain(|a| !a.on.as_ref().is_some_and(fonts));
        f.children.retain(|c| !fonts(&c.name));
        f.kept.retain(|c| !fonts(&c.name));
        if f != ForeignProps::default() {
            p.foreign = Some(Rc::new(f));
        }
    }
    p
}

// Conversion from the XML path's properties

/// The XML path's run properties (one level, or levels folded) as typed properties.
pub fn from_run_props(r: &RunProps) -> CharProps {
    let font = |f: &Option<xml_props::FontRef>| {
        f.as_ref().map(|f| match f {
            xml_props::FontRef::Name(n) => FontRef::Family(n.as_str().into()),
            xml_props::FontRef::Theme(t) => theme_font(t),
        })
    };
    let rgb = |c: Option<u32>, theme: Option<ThemeColourRef>| theme.map(Color::Theme).or(c.map(Color::Rgb));
    let toggles = |bits: u16| {
        let mut on = 0;
        for (name, bit) in xml_props::toggle::NAMES {
            if bits & bit != 0 {
                on |= TOGGLES.iter().find(|(n, _)| *n == name).map_or(0, |(_, t)| t.0);
            }
        }
        on
    };
    CharProps {
        style: r.style.as_deref().map(Rc::from),
        fonts: ScriptFonts { latin: font(&r.ascii), latin_extended: font(&r.h_ansi), east_asian: font(&r.east_asia), complex: font(&r.cs) },
        east_asian_below: font(&r.east_asia_below),
        east_asian_theme: font(&r.east_asia_theme),
        hint: r.hint.as_deref().map(hint),
        size: r.size,
        complex_size: r.size_cs,
        complex: r.complex,
        toggles: Toggles { set: toggles(r.toggles_set), on: toggles(r.toggles) },
        double_strike: r.dstrike,
        no_proof: r.no_proof,
        underline: r.underline.map(|style| UnderlineProps { style, color: rgb(r.underline_color.flatten(), r.underline_color_theme) }),
        color: r.color.map(|c| rgb(c, r.color_theme).unwrap_or(Color::Auto)),
        highlight: r.highlight.map(|h| h.and_then(|v| Highlight::ALL.into_iter().find(|h| h.rgb() == v))),
        shading: r.shd.as_ref().map(|s| Shading { pattern: s.pattern, color: rgb(s.color, s.color_theme), fill: rgb(s.fill, s.fill_theme) }),
        border: r.border.as_ref().map(|b| TextBorder {
            style: b.style,
            width: b.width,
            space: b.space,
            shadow: b.shadow,
            color: rgb(b.color, b.color_theme),
        }),
        vertical: r.vert_align,
        position: r.position,
        spacing: r.spacing,
        scale: r.scale,
        kern: r.kern,
        typography: TypographyProps {
            ligatures: r.ligatures,
            contextual_alternates: r.cntxt_alts,
            number_form: r.num_form.map(|n| match n {
                1 => NumberForm::Lining,
                2 => NumberForm::OldStyle,
                _ => NumberForm::Default,
            }),
            number_spacing: r.num_spacing.map(|n| match n {
                4 => NumberSpacing::Proportional,
                8 => NumberSpacing::Tabular,
                _ => NumberSpacing::Default,
            }),
            stylistic_sets: r.style_sets,
        },
        effects: r.effects.clone(),
        emphasis: r.em,
        lang: LangProps { latin: r.lang.as_deref().map(Rc::from), east_asian: r.lang_east_asia.as_deref().map(Rc::from), complex: None },
        rtl: r.rtl,
        spec_vanish: None,
        fit_text: r.fit_text,
        foreign: None,
    }
}

// The attributes a modelled child is written with

/// Where a modelled child's attributes go: written, or compared with an element as read.
trait Sink {
    /// Attribute `local` (in Word 2010's namespace when `w14`, else WordprocessingML's) with value `v`.
    fn attr(&mut self, w14: bool, local: &'static str, v: &dyn Display);
    /// Attribute `local` left out, which reads as `means`.
    fn implied(&mut self, local: &'static str, means: &'static str);
    /// Attribute `local`, which `by` decides: a colour's value beside its theme colour (written as `theme` resolves),
    /// a font's name beside its theme font.
    fn decided(&mut self, local: &'static str, by: &'static str, theme: Option<ThemeColourRef>);
}

/// Writes modelled child `key` of `p` to `s`; false when `p` doesn't set it. Its content, if any, is written apart.
fn emit(p: &CharProps, key: &str, s: &mut dyn Sink) -> bool {
    let toggle = |s: &mut dyn Sink, v: Option<bool>| match v {
        Some(true) => {
            s.implied("val", "1");
            true
        }
        Some(false) => {
            s.attr(false, "val", &"0");
            true
        }
        None => false,
    };
    let val = |s: &mut dyn Sink, v: &dyn Display| s.attr(false, "val", v);
    if let Some(&(_, t)) = TOGGLES.iter().find(|(n, _)| *n == key) {
        return toggle(s, p.toggles.get(t));
    }
    match key {
        "rStyle" => p.style.as_deref().map(|v| val(s, &v)).is_some(),
        "rFonts" => {
            let fonts = [&p.fonts.latin, &p.fonts.latin_extended, &p.fonts.east_asian, &p.fonts.complex];
            for (font, (name, theme)) in fonts.into_iter().zip(FONT_SLOTS) {
                match font {
                    Some(FontRef::Family(n)) => s.attr(false, name, &&**n),
                    Some(FontRef::Theme { face, script }) => {
                        s.attr(false, theme, &theme_font_name(*face, *script));
                        s.decided(name, theme, None);
                    }
                    None => {}
                }
            }
            if let Some(h) = p.hint {
                let v = match h {
                    FontHint::Default => "default",
                    FontHint::EastAsia => "eastAsia",
                    FontHint::Complex => "cs",
                };
                s.attr(false, "hint", &v);
            }
            fonts.iter().any(|f| f.is_some()) || p.hint.is_some()
        }
        "dstrike" => toggle(s, p.double_strike),
        "noProof" => toggle(s, p.no_proof),
        "rtl" => toggle(s, p.rtl),
        "specVanish" => toggle(s, p.spec_vanish),
        "cs" => toggle(s, p.complex),
        "color" => p.color.map(|c| colour_attrs(s, c, COLOUR)).is_some(),
        "spacing" => p.spacing.map(|v| val(s, &Twips(v))).is_some(),
        "fitText" => p
            .fit_text
            .map(|(w, id)| {
                val(s, &Twips(w));
                s.attr(false, "id", &id);
            })
            .is_some(),
        "w" => p.scale.map(|v| val(s, &v)).is_some(),
        "kern" => p.kern.map(|v| val(s, &HalfPoints(v))).is_some(),
        "position" => p.position.map(|v| val(s, &HalfPoints(v))).is_some(),
        "sz" => p.size.map(|v| val(s, &HalfPoints(v))).is_some(),
        "szCs" => p.complex_size.map(|v| val(s, &HalfPoints(v))).is_some(),
        "highlight" => p.highlight.map(|h| val(s, &h.and_then(|h| HIGHLIGHTS.iter().find(|(_, c)| *c == h)).map_or("none", |(n, _)| n))).is_some(),
        "u" => p
            .underline
            .map(|u| {
                val(s, &underline_val(u.style));
                if let Some(c) = u.color {
                    colour_attrs(s, c, LINE_COLOUR);
                }
            })
            .is_some(),
        "bdr" => p
            .border
            .map(|b| {
                val(s, &border_val(b.style));
                s.attr(false, "sz", &(b.width * 8.0));
                s.attr(false, "space", &b.space);
                if let Some(c) = b.color {
                    colour_attrs(s, c, LINE_COLOUR);
                }
                if b.shadow {
                    s.attr(false, "shadow", &1);
                } else {
                    s.implied("shadow", "0");
                }
            })
            .is_some(),
        "shd" => p
            .shading
            .map(|sh| {
                val(s, &shading_val(sh.pattern));
                if let Some(c) = sh.color {
                    colour_attrs(s, c, LINE_COLOUR);
                }
                if let Some(c) = sh.fill {
                    colour_attrs(s, c, FILL);
                }
            })
            .is_some(),
        "vertAlign" => p
            .vertical
            .map(|v| {
                let v = match v {
                    VertAlign::Baseline => "baseline",
                    VertAlign::Superscript => "superscript",
                    VertAlign::Subscript => "subscript",
                };
                val(s, &v)
            })
            .is_some(),
        "em" => p.emphasis.map(|e| val(s, &emphasis_val(e))).is_some(),
        "lang" => {
            let l = &p.lang;
            for (name, v) in [("val", &l.latin), ("eastAsia", &l.east_asian), ("bidi", &l.complex)] {
                if let Some(v) = v {
                    s.attr(false, name, &&**v);
                }
            }
            l.latin.is_some() || l.east_asian.is_some() || l.complex.is_some()
        }
        "w14:ligatures" => p.typography.ligatures.map(|l| s.attr(true, "val", &LIGATURES.get(usize::from(l)).copied().unwrap_or("none"))).is_some(),
        "w14:cntxtAlts" => p.typography.contextual_alternates.map(|on| s.attr(true, "val", &if on { "1" } else { "0" })).is_some(),
        "w14:numForm" => p
            .typography
            .number_form
            .map(|n| {
                let v = match n {
                    NumberForm::Default => "default",
                    NumberForm::Lining => "lining",
                    NumberForm::OldStyle => "oldStyle",
                };
                s.attr(true, "val", &v)
            })
            .is_some(),
        "w14:numSpacing" => p
            .typography
            .number_spacing
            .map(|n| {
                let v = match n {
                    NumberSpacing::Default => "default",
                    NumberSpacing::Proportional => "proportional",
                    NumberSpacing::Tabular => "tabular",
                };
                s.attr(true, "val", &v)
            })
            .is_some(),
        _ => false,
    }
}

fn colour_attrs(s: &mut dyn Sink, c: Color, [hex, theme, tint, shade]: ColourAttrs) {
    match c {
        Color::Auto => s.attr(false, hex, &"auto"),
        Color::Rgb(v) => s.attr(false, hex, &format_args!("{v:06X}")),
        Color::Theme(r) => {
            s.decided(hex, theme, Some(r));
            if let Some(name) = theme_name(r.slot) {
                s.attr(false, theme, &name);
            }
            if let Some(t) = r.tint {
                s.attr(false, tint, &format_args!("{t:02X}"));
            }
            if let Some(v) = r.shade {
                s.attr(false, shade, &format_args!("{v:02X}"));
            }
        }
    }
}

fn emit_revision(r: &RevisionAttrs, s: &mut dyn Sink) {
    for (name, v) in [("id", &r.id), ("author", &r.author), ("date", &r.date)] {
        if let Some(v) = v {
            s.attr(false, name, &&**v);
        }
    }
}

/// The attribute of `e` the parse reads for modelled attribute `local`: in Word 2010's namespace for a Word 2010
/// child, else WordprocessingML's, else in none (and `w:cstheme` as `w:csTheme` first).
fn read_attr(e: Node, w14: bool, local: &str) -> Option<usize> {
    let find = |ns: Option<&str>, local: &str| e.attributes().position(|a| a.namespace() == ns && a.name() == local);
    if local == "cstheme" {
        return find(Some(W), "csTheme").or_else(|| find(None, "csTheme")).or_else(|| find(Some(W), local)).or_else(|| find(None, local));
    }
    w14.then(|| find(Some(W14), local)).flatten().or_else(|| find(Some(W), local)).or_else(|| find(None, local))
}

/// Compares the attributes a modelled child is written with against an element as read: the attributes that differ
/// or that aren't written are the element's foreign ones.
struct Compare<'a, 'i, 's> {
    e: Node<'a, 'i>,
    scratch: &'s mut String,
    /// The attributes written as read, by index (past the 64th, none).
    same: u64,
    /// The attributes written otherwise, by index, with the value they stand for, or beside the attribute deciding
    /// them.
    ties: Vec<(usize, Option<Rc<str>>, Option<Rc<str>>)>,
}

impl<'a, 'i, 's> Compare<'a, 'i, 's> {
    fn new(e: Node<'a, 'i>, scratch: &'s mut String) -> Self {
        Compare { e, scratch, same: 0, ties: Vec::new() }
    }

    fn finish(self, on: Option<QName>, out: &mut Vec<ForeignAttr>) {
        let text = self.e.document().input_text();
        for (i, a) in self.e.attributes().enumerate() {
            if i < 64 && self.same & 1 << i != 0 {
                continue;
            }
            let qname = &text[a.range_qname()];
            let (stands_for, beside) = self.ties.iter().find(|t| t.0 == i).map_or((None, None), |t| (t.1.clone(), t.2.clone()));
            out.push(ForeignAttr {
                on: on.clone(),
                name: QName { ns: a.namespace().unwrap_or_default().into(), local: a.name().into() },
                prefix: qname.split_once(':').map_or("", |(p, _)| p).into(),
                value: a.value().into(),
                stands_for,
                beside,
            });
        }
    }
}

impl Sink for Compare<'_, '_, '_> {
    fn attr(&mut self, w14: bool, local: &'static str, v: &dyn Display) {
        let Some((i, a)) = read_attr(self.e, w14, local).and_then(|i| Some((i, self.e.attributes().nth(i)?))) else { return };
        self.scratch.clear();
        let _ = write!(self.scratch, "{v}");
        let own = a.namespace() == Some(if w14 { W14 } else { W }) && a.name() == local;
        if own && a.value() == self.scratch.as_str() {
            if i < 64 {
                self.same |= 1 << i;
            }
        } else {
            self.ties.push((i, Some(self.scratch.as_str().into()), None));
        }
    }

    fn implied(&mut self, local: &'static str, means: &'static str) {
        if let Some(i) = read_attr(self.e, false, local) {
            self.ties.push((i, Some(means.into()), None));
        }
    }

    fn decided(&mut self, local: &'static str, by: &'static str, _: Option<ThemeColourRef>) {
        if let Some(i) = read_attr(self.e, false, local) {
            self.ties.push((i, None, Some(by.into())));
        }
    }
}

// Serialize

/// `p` and its revisions as a `w:rPr` in the part `cx` describes; a theme colour's value is written as `colours`
/// resolve it.
pub fn serialize(p: &CharProps, rev: &RprRevisions, colours: &ThemeColours, cx: &Cx) -> String {
    let w14 = cx.prefix_of(W14);
    let mut out = Out {
        s: String::new(),
        cx,
        colours,
        w14: w14.clone().unwrap_or_else(|| "w14:".into()),
        w14_decl: if w14.is_some() { "" } else { W14_DECL },
        declared: Vec::new(),
        renamed: Vec::new(),
    };
    let mut decls = String::new();
    let previous = rev.change.as_ref().map(|c| &c.previous);
    for q in [Some(p), previous].into_iter().flatten() {
        out.declarations(q.foreign.as_deref(), &mut decls);
    }
    let attrs = [&rev.ins, &rev.del, &rev.move_from, &rev.move_to].into_iter().flatten().chain(rev.change.as_ref().map(|c| &c.attrs));
    for a in attrs.flat_map(|r| &r.foreign) {
        out.declare_attr(a, &mut decls);
    }
    let w = &cx.w;
    let _ = write!(out.s, "<{w}rPr{decls}");
    out.free_attrs(p.foreign.as_deref().map_or(&[][..], |f| &f.attributes), None, &[]);
    out.s.push('>');
    out.children(p, Some(rev));
    let _ = write!(out.s, "</{w}rPr>");
    out.s
}

/// `p` and its revisions as a `w:rPr` in the part `cx` describes, written as an edit of `origin` (the `w:rPr` the
/// properties it holds were read from, and those properties) would: each child the change leaves alone as written
/// there, each it changes in its place, a new one in schema order after any of its name, one it drops removed; with no
/// origin, as an edit of no `w:rPr`. A changed child is written whole in its place, or, when `patch` names it, as its
/// attributes set on it. `None` when nothing is left.
pub fn written_over(
    p: &CharProps,
    rev: &RprRevisions,
    colours: &ThemeColours,
    cx: &Cx,
    origin: Option<(&CharProps, &str)>,
    set: &[Set],
) -> Option<String> {
    use super::write::props::Props;
    let foreign = |q: &CharProps| q.foreign.as_ref().is_some_and(|f| !f.children.is_empty() || !f.attributes.is_empty() || !f.kept.is_empty());
    let w14 = cx.prefix_of(W14);
    if origin.is_none() && foreign(p) {
        return Some(serialize(p, rev, colours, cx));
    }
    let empty = CharProps::default();
    let (read, xml) = origin.unwrap_or((&empty, ""));
    let mut props = Props::parse(origin.map(|_| xml), "rPr", cx);
    let mut out = Out {
        s: String::new(),
        cx,
        colours,
        w14: w14.clone().unwrap_or_else(|| "w14:".into()),
        w14_decl: if w14.is_some() { "" } else { W14_DECL },
        declared: Vec::new(),
        renamed: Vec::new(),
    };
    let none = RprRevisions::default();
    for &key in RPR {
        let mut render = |q: &CharProps, rev: &RprRevisions| {
            if is_revision(key) {
                out.revision(Some(rev), key);
            } else {
                out.modelled(q, key);
            }
            std::mem::take(&mut out.s)
        };
        let (was, now) = (render(read, &none), render(p, rev));
        let whole: Vec<Prop> = set.iter().filter(|s| s.whole && key_of(s.prop) == key).map(|s| s.prop).collect();
        let parts: Vec<Prop> = set.iter().filter(|s| !s.whole && key_of(s.prop) == key).map(|s| s.prop).collect();
        if was == now && whole.is_empty() && parts.is_empty() {
            continue;
        }
        if now.is_empty() {
            props.remove(key);
            continue;
        }
        let written = match props.get(key) {
            Some(old) if whole.is_empty() && !parts.is_empty() => in_part(old, &now, &parts, &cx.w),
            _ => None,
        };
        props.set(key, written.unwrap_or_else(|| in_order(&now, &whole)));
    }
    let children = |q: &CharProps| {
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

/// The child of CT_RPr a property is written in.
fn key_of(p: Prop) -> &'static str {
    match p {
        Prop::Toggle(t) => TOGGLES.iter().find(|(_, x)| *x == t).map_or("b", |(n, _)| n),
        Prop::DoubleStrike => "dstrike",
        Prop::NoProof => "noProof",
        Prop::Complex => "cs",
        Prop::Rtl => "rtl",
        Prop::SpecVanish => "specVanish",
        Prop::Size => "sz",
        Prop::ComplexSize => "szCs",
        Prop::Underline | Prop::UnderlineColor => "u",
        Prop::Color => "color",
        Prop::Highlight => "highlight",
        Prop::Shading => "shd",
        Prop::Border => "bdr",
        Prop::Vertical => "vertAlign",
        Prop::Position => "position",
        Prop::Spacing => "spacing",
        Prop::Scale => "w",
        Prop::Kern => "kern",
        Prop::Font(_) | Prop::Hint => "rFonts",
        Prop::Style => "rStyle",
        Prop::Lang(_) => "lang",
        Prop::FitText => "fitText",
        Prop::Emphasis => "em",
        Prop::Ligatures => "w14:ligatures",
        Prop::ContextualAlternates => "w14:cntxtAlts",
        Prop::NumberForm => "w14:numForm",
        Prop::NumberSpacing => "w14:numSpacing",
        Prop::StylisticSets => "w14:stylisticSets",
        Prop::Effect(e) => match e {
            Effect::Fill => "w14:textFill",
            Effect::Outline => "w14:textOutline",
            Effect::Shadow => "w14:shadow",
            Effect::Glow => "w14:glow",
            Effect::Reflection => "w14:reflection",
        },
    }
}

/// A script class's name and theme font attributes in `w:rFonts`.
fn font_attrs(c: ScriptClass) -> (&'static str, &'static str) {
    match c {
        ScriptClass::Latin => FONT_SLOTS[0],
        ScriptClass::LatinExtended => FONT_SLOTS[1],
        ScriptClass::EastAsian => FONT_SLOTS[2],
        ScriptClass::Complex => FONT_SLOTS[3],
    }
}

/// An element with no content as written: its name and each attribute's name and quoted value.
fn attr_list(x: &str) -> Option<(String, Vec<(String, String)>)> {
    let x = x.strip_suffix("/>")?;
    let name = x.strip_prefix('<')?.split(|c: char| c.is_whitespace()).next()?.to_string();
    let list = super::binding::tag_attrs(x)
        .map(|(name, _, r)| {
            let written = &x[r];
            (name.to_string(), written[written.find('=').map_or(0, |e| e + 1)..].trim_start().to_string())
        })
        .collect();
    Some((name, list))
}

/// `now`, a child written anew, with the attributes of the fonts `whole` names first, in its order.
fn in_order(now: &str, whole: &[Prop]) -> String {
    let slots: Vec<ScriptClass> = whole.iter().filter_map(|p| if let Prop::Font(c) = p { Some(*c) } else { None }).collect();
    let Some((name, mut attrs)) = attr_list(now).filter(|_| slots.len() > 1) else { return now.to_string() };
    let mut out = format!("<{name}");
    for c in slots {
        let (n, t) = font_attrs(c);
        if let Some(k) = attrs.iter().position(|(q, _)| q.rsplit(':').next().is_some_and(|l| l == n || l == t)) {
            let (q, v) = attrs.remove(k);
            let _ = write!(out, " {q}={v}");
        }
    }
    for (q, v) in attrs {
        let _ = write!(out, " {q}={v}");
    }
    out.push_str("/>");
    out
}

/// Child `old` as written with the properties `parts` set on it in part, in order, as editing the XML set them: a
/// font named in place of what it had (its theme font gone), a theme font, an underline's colour or a language as
/// attributes set on it, the others kept as written. Values come from `now`, the child as its properties write it.
/// `None` unless both are elements with no content.
fn in_part(old: &str, now: &str, parts: &[Prop], w: &str) -> Option<String> {
    let (_, new) = attr_list(now)?;
    attr_list(old)?;
    let value = |local: &str| new.iter().find(|(q, _)| q.rsplit(':').next() == Some(local)).map(|(_, v)| v.as_str());
    let mut tag = old.to_string();
    // A family named in part: the slot's attributes set on the element, rewritten
    let named: Vec<ScriptClass> =
        parts.iter().filter_map(|p| if let Prop::Font(c) = p { Some(*c) } else { None }).filter(|c| value(font_attrs(*c).0).is_some()).collect();
    if !named.is_empty() {
        let (_, mut attrs) = attr_list(&tag)?;
        let local = |q: &str| q.rsplit(':').next().map(str::to_string);
        for c in named {
            let (n, t) = font_attrs(c);
            let v = value(n)?.to_string();
            match attrs.iter_mut().find(|(q, _)| local(q).as_deref() == Some(n)) {
                Some(a) => a.1 = v,
                None => attrs.push((format!("{w}{n}"), v)),
            }
            attrs.retain(|(q, _)| local(q).as_deref() != Some(t) && !(c == ScriptClass::Complex && local(q).as_deref() == Some("csTheme")));
        }
        tag = format!("<{w}rFonts{}/>", attrs.iter().map(|(q, v)| format!(" {q}={v}")).collect::<String>());
    }
    for p in parts {
        match *p {
            Prop::Font(c) if value(font_attrs(c).0).is_none() => {
                let (n, t) = font_attrs(c);
                tag = strip(&tag, &format!("{w}{n}"));
                tag = strip(&tag, &format!("{w}{t}"));
                if let Some(v) = value(t) {
                    tag = with_attr_written(&tag, &format!("{w}{t}"), v);
                }
            }
            Prop::UnderlineColor => {
                for a in ["themeColor", "themeTint", "themeShade"] {
                    tag = strip(&tag, &format!("{w}{a}"));
                }
                for a in ["color", "themeColor", "themeTint", "themeShade"] {
                    if let Some(v) = value(a) {
                        tag = with_attr_written(&tag, &format!("{w}{a}"), v);
                    }
                }
            }
            Prop::Lang(c) => {
                let a = match c {
                    ScriptClass::EastAsian => "eastAsia",
                    ScriptClass::Complex => "bidi",
                    _ => "val",
                };
                tag = match value(a) {
                    Some(v) => with_attr_written(&tag, &format!("{w}{a}"), v),
                    None => strip(&tag, &format!("{w}{a}")),
                };
            }
            Prop::Hint => {
                tag = match value("hint") {
                    Some(v) => with_attr_written(&tag, &format!("{w}hint"), v),
                    None => strip(&tag, &format!("{w}hint")),
                };
            }
            _ => {}
        }
    }
    Some(tag)
}

/// The span of attribute `name` in `tag` as `(start of its leading space, start of its value, end of its value)`,
/// written with either quote; other attributes' values are skipped.
fn attr_span(tag: &str, name: &str) -> Option<(usize, usize, usize)> {
    let b = tag.as_bytes();
    let mut k = 0;
    while k < b.len() {
        match b[k] {
            q @ (b'"' | b'\'') => k = tag[k + 1..].find(q as char).map_or(b.len(), |e| k + e + 2),
            _ if k > 0 && b[k - 1].is_ascii_whitespace() && b[k..].starts_with(name.as_bytes()) => {
                let after = k + name.len();
                if let Some(v) = tag[after..].trim_start().strip_prefix('=').map(str::trim_start)
                    && let Some(q) = v.chars().next().filter(|c| matches!(c, '"' | '\''))
                {
                    let start = tag.len() - v.len() + 1;
                    let end = tag[start..].find(q).map_or(tag.len(), |e| start + e);
                    return Some((tag[..k].trim_end().len(), start, end));
                }
                k = after;
            }
            _ => k += 1,
        }
    }
    None
}

/// `tag` without the attribute `name`.
pub(crate) fn strip(tag: &str, name: &str) -> String {
    let Some((lead, _, end)) = attr_span(tag, name) else { return tag.to_string() };
    format!("{}{}", &tag[..lead], &tag[(end + 1).min(tag.len())..])
}

/// `tag` (one start tag or empty element) with the attribute `name` set to `value`.
pub(crate) fn with_attr(tag: &str, name: &str, value: &str) -> String {
    let v = super::write::props::esc(value);
    with_attr_text(tag, name, &v)
}

/// [`with_attr`] with `quoted`, a value written in double quotes (escaped).
fn with_attr_written(tag: &str, name: &str, quoted: &str) -> String {
    with_attr_text(tag, name, quoted.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(quoted))
}

fn with_attr_text(tag: &str, name: &str, v: &str) -> String {
    if let Some((_, from, to)) = attr_span(tag, name) {
        // Its own quotes kept: a single-quoted value's apostrophes escaped
        let v = if tag[..from].ends_with('\'') { v.replace('\'', "&apos;") } else { v.to_string() };
        return format!("{}{v}{}", &tag[..from], &tag[to..]);
    }
    let at = tag.rfind("/>").or_else(|| tag.rfind('>')).unwrap_or(tag.len());
    format!("{} {name}=\"{v}\"{}", &tag[..at], &tag[at..])
}

/// What `p` keeps beside each modelled child `set` sets whole, or that it changes and doesn't set in part: nothing, as
/// writing the child anew keeps nothing of it as written; beside one set in part, no value written otherwise.
fn drop_replaced(was: &CharProps, p: &mut CharProps, set: &[Set]) {
    let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\" xmlns:w14=\"{W14}\">"));
    let colours = ThemeColours::default();
    let mut out = Out { s: String::new(), cx: &cx, colours: &colours, w14: "w14:".into(), w14_decl: "", declared: Vec::new(), renamed: Vec::new() };
    let mut render = |q: &CharProps, key: &'static str| {
        out.modelled(q, key);
        std::mem::take(&mut out.s)
    };
    let replaced: Vec<(&str, &str)> = RPR
        .iter()
        .filter(|k| {
            let by = |whole: bool| set.iter().any(|s| s.whole == whole && key_of(s.prop) == **k);
            !is_revision(k) && (by(true) || !by(false) && render(was, k) != render(p, k))
        })
        .map(|k| match k.strip_prefix("w14:") {
            Some(local) => (W14, local),
            None => (W, *k),
        })
        .collect();
    if replaced.is_empty() {
        return;
    }
    let Some(f) = p.foreign.as_mut() else { return };
    let f = Rc::make_mut(f);
    let of = |q: &QName| replaced.contains(&(&*q.ns, &*q.local));
    let part: Vec<&str> = set.iter().filter(|s| !s.whole).map(|s| key_of(s.prop)).collect();
    let in_part = |q: &QName| {
        part.iter().any(|k| match k.strip_prefix("w14:") {
            Some(l) => &*q.ns == W14 && &*q.local == l,
            None => &*q.ns == W && &*q.local == *k,
        })
    };
    // What it held as read goes; what the edit gave it stays
    let had = was.foreign.as_deref();
    f.attributes
        .retain(|a| !a.on.as_ref().is_some_and(|q| of(q) || in_part(q) && a.stands_for.is_some()) || !had.is_some_and(|h| h.attributes.contains(a)));
    f.kept.retain(|c| !of(&c.name) || !had.is_some_and(|h| h.kept.contains(c)));
    if *f == ForeignProps::default() {
        p.foreign = None;
    }
}

/// `p` over `q` a child at a time, as a source format writes them: each child `q` writes and `p` doesn't, with what `q`
/// keeps beside it; `q`'s character style aside.
pub fn over(p: &CharProps, q: &CharProps) -> CharProps {
    let cx = Cx::of_xml(&format!("<w:document xmlns:w=\"{W}\" xmlns:w14=\"{W14}\">"));
    let colours = ThemeColours::default();
    let mut out = Out { s: String::new(), cx: &cx, colours: &colours, w14: "w14:".into(), w14_decl: "", declared: Vec::new(), renamed: Vec::new() };
    let mut writes = |x: &CharProps, key: &'static str| {
        out.modelled(x, key);
        !std::mem::take(&mut out.s).is_empty()
    };
    let mut r = CharProps { style: None, foreign: None, ..q.clone() };
    r.merge(p);
    // Fonts and languages: one child each
    if writes(p, "rFonts") {
        (r.fonts, r.hint) = (p.fonts.clone(), p.hint);
    }
    if writes(p, "lang") {
        r.lang = p.lang.clone();
    }
    (r.east_asian_below, r.east_asian_theme) = (None, None);
    let theirs: Vec<&'static str> = RPR.iter().copied().filter(|k| !is_revision(k) && *k != "rStyle" && !writes(p, k) && writes(q, k)).collect();
    let of_theirs = |n: &QName| rank_of(&n.ns, &n.local).is_some_and(|k| theirs.contains(&RPR[k]));
    let (mine, other) = (p.foreign.as_deref(), q.foreign.as_deref());
    if let Some(o) = other {
        let mut f = mine.cloned().unwrap_or_default();
        let named = |c: &ForeignChild| f.children.iter().any(|m| m.name == c.name);
        let children: Vec<ForeignChild> = o.children.iter().filter(|c| !named(c)).cloned().collect();
        f.children.extend(children);
        f.attributes.extend(o.attributes.iter().filter(|a| a.on.as_ref().is_some_and(of_theirs)).cloned());
        f.kept.extend(o.kept.iter().filter(|c| of_theirs(&c.name)).cloned());
        r.foreign = (f != ForeignProps::default()).then(|| Rc::new(f));
    } else {
        r.foreign = p.foreign.clone();
    }
    r
}

/// A child's key among a properties element's ([`super::write::props::Props`]): its local name in WordprocessingML,
/// Word 2010's and 2012's with their prefix, any other with its namespace.
fn props_key(name: &QName) -> String {
    match &*name.ns {
        W | "" => name.local.to_string(),
        W14 => format!("w14:{}", name.local),
        super::xml::W15 => format!("w15:{}", name.local),
        ns => format!("{{{ns}}}{}", name.local),
    }
}

struct Out<'a> {
    s: String,
    cx: &'a Cx,
    colours: &'a ThemeColours,
    /// Word 2010's prefix, with its colon.
    w14: String,
    /// Its declaration, on each element of it, when the part's root lacks one.
    w14_decl: &'static str,
    /// The bindings declared on the `w:rPr`.
    declared: Vec<(Rc<str>, Rc<str>)>,
    /// Unmodelled attributes' namespaces written under a prefix of their own, to keep clear of the writer's.
    renamed: Vec<(Rc<str>, String)>,
}

impl Out<'_> {
    /// Whether the part's root binds `prefix` to `uri`.
    fn rooted(&self, prefix: &str, uri: &str) -> bool {
        if prefix.is_empty() {
            return self.cx.w.is_empty() && uri == W;
        }
        self.cx.prefix_of(uri).is_some_and(|p| p.strip_suffix(':') == Some(prefix))
    }

    /// Whether `prefix` is one the writer itself uses, for a namespace other than `uri`.
    fn clashes(&self, prefix: &str, uri: &str) -> bool {
        let w = self.cx.w.strip_suffix(':').unwrap_or_default();
        let w14 = self.w14.strip_suffix(':').unwrap_or_default();
        (prefix == w && uri != W) || (prefix == w14 && uri != W14) || self.declared.iter().any(|(p, u)| &**p == prefix && &**u != uri)
    }

    fn declare(&mut self, prefix: &str, uri: &str, decls: &mut String) {
        if self.declared.iter().any(|(p, u)| &**p == prefix && &**u == uri) {
            return;
        }
        decls.push_str(if prefix.is_empty() { " xmlns" } else { " xmlns:" });
        decls.push_str(prefix);
        decls.push_str("=\"");
        push_esc(decls, uri);
        decls.push('"');
        self.declared.push((prefix.into(), uri.into()));
    }

    /// Declares on the `w:rPr` what the unmodelled children and attributes need and the root lacks.
    fn declarations(&mut self, foreign: Option<&ForeignProps>, decls: &mut String) {
        let Some(f) = foreign else { return };
        for c in f.children.iter().chain(&f.kept) {
            for (p, u) in c.bindings.iter() {
                if !self.rooted(p, u) && !declares(&c.payload, p) && !self.clashes(p, u) {
                    self.declare(p, u, decls);
                }
            }
        }
        for a in &f.attributes {
            self.declare_attr(a, decls);
        }
    }

    fn declare_attr(&mut self, a: &ForeignAttr, decls: &mut String) {
        let (p, u) = (&*a.prefix, &*a.name.ns);
        if u.is_empty() || u == XML_NS || u == W || u == W14 || self.rooted(p, u) || self.renamed.iter().any(|(r, _)| &**r == u) {
            return;
        }
        if p.is_empty() || self.clashes(p, u) {
            let fresh = format!("ns{}", self.renamed.len());
            self.declare(&fresh, u, decls);
            self.renamed.push((a.name.ns.clone(), fresh));
        } else {
            self.declare(p, u, decls);
        }
    }

    /// The prefix, with its colon, a foreign attribute is written with.
    fn prefix_for(&self, a: &ForeignAttr) -> String {
        match &*a.name.ns {
            "" => String::new(),
            W => self.cx.w.clone(),
            W14 => self.w14.clone(),
            ns => match self.renamed.iter().find(|(u, _)| &**u == ns) {
                Some((_, fresh)) => format!("{fresh}:"),
                None => format!("{}:", a.prefix),
            },
        }
    }

    /// The foreign attributes of `on` written as they are: not standing for a modelled value, not among `written`,
    /// and beside an attribute that is written if they are beside one.
    fn free_attrs(&mut self, attrs: &[ForeignAttr], on: Option<(&str, &str)>, written: &[(&str, &str)]) {
        for a in attrs.iter().filter(|a| a.on.as_ref().map(|q| (&*q.ns, &*q.local)) == on && a.stands_for.is_none()) {
            if written.contains(&(&*a.name.ns, &*a.name.local)) || a.beside.as_deref().is_some_and(|b| !written.iter().any(|w| w.1 == b)) {
                continue;
            }
            let prefix = self.prefix_for(a);
            let _ = write!(self.s, " {prefix}{}=\"", a.name.local);
            push_esc(&mut self.s, &a.value);
            self.s.push('"');
        }
    }

    /// CT_RPr's children of `p`, in schema order, with the mark's revisions first and the formatting change last.
    fn children(&mut self, p: &CharProps, rev: Option<&RprRevisions>) {
        let foreign = p.foreign.as_deref();
        let ranks: Vec<Option<usize>> = foreign.map_or_else(Vec::new, |f| f.children.iter().map(|c| rank_of(&c.name.ns, &c.name.local)).collect());
        for (i, &key) in RPR.iter().enumerate() {
            if key == "rPrChange" {
                self.foreign_children(foreign, &ranks, None);
            }
            if is_revision(key) {
                self.revision(rev, key);
                self.foreign_children(foreign, &ranks, Some(i));
            } else {
                self.foreign_children(foreign, &ranks, Some(i));
                self.modelled(p, key);
            }
        }
    }

    fn foreign_children(&mut self, foreign: Option<&ForeignProps>, ranks: &[Option<usize>], at: Option<usize>) {
        let Some(f) = foreign else { return };
        for (c, _) in f.children.iter().zip(ranks).filter(|(_, r)| **r == at) {
            self.payload(c);
        }
    }

    /// An element as written, with any binding it needs that neither the part nor the `w:rPr` declares.
    fn payload(&mut self, c: &ForeignChild) {
        let missing =
            |(p, u): &&(Rc<str>, Rc<str>)| !self.rooted(p, u) && !declares(&c.payload, p) && !self.declared.iter().any(|(dp, du)| dp == p && du == u);
        let missing: Vec<&(Rc<str>, Rc<str>)> = c.bindings.iter().filter(missing).collect();
        if missing.is_empty() {
            self.s.push_str(&c.payload);
            return;
        }
        // Bindings on the child itself
        let at = c.payload.find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/').unwrap_or(c.payload.len());
        self.s.push_str(&c.payload[..at]);
        for (p, u) in missing {
            self.s.push_str(if p.is_empty() { " xmlns" } else { " xmlns:" });
            self.s.push_str(p);
            self.s.push_str("=\"");
            push_esc(&mut self.s, u);
            self.s.push('"');
        }
        self.s.push_str(&c.payload[at..]);
    }

    fn revision(&mut self, rev: Option<&RprRevisions>, key: &str) {
        let Some(rev) = rev else { return };
        let (r, change) = match key {
            "ins" => (rev.ins.as_ref(), None),
            "del" => (rev.del.as_ref(), None),
            "moveFrom" => (rev.move_from.as_ref(), None),
            "moveTo" => (rev.move_to.as_ref(), None),
            _ => (rev.change.as_ref().map(|c| &c.attrs), rev.change.as_ref().map(|c| &c.previous)),
        };
        let Some(r) = r else { return };
        let _ = write!(self.s, "<{}{key}", self.cx.w);
        let written = self.attrs(false, &r.foreign, None, |s| emit_revision(r, s));
        self.free_attrs(&r.foreign, None, &written);
        let Some(previous) = change else {
            self.s.push_str("/>");
            return;
        };
        let w = self.cx.w.clone();
        let _ = write!(self.s, "><{w}rPr");
        self.free_attrs(previous.foreign.as_deref().map_or(&[][..], |f| &f.attributes), None, &[]);
        self.s.push('>');
        self.children(previous, None);
        let _ = write!(self.s, "</{w}rPr></{w}{key}>");
    }

    /// Writes the attributes `emit` gives into the open start tag: each modelled one as read while it still stands
    /// for the value read. Returns the names written.
    fn attrs(
        &mut self,
        w14: bool,
        foreign: &[ForeignAttr],
        on: Option<(&str, &str)>,
        emit: impl FnOnce(&mut dyn Sink),
    ) -> Vec<(&'static str, &'static str)> {
        let mut sink = Attrs { out: self, w14, foreign, on, written: Vec::new(), scratch: String::new() };
        emit(&mut sink);
        sink.written
    }

    /// Modelled child `key` of `p`: as written when it was kept so and its value is still the one read from it.
    fn modelled(&mut self, p: &CharProps, key: &'static str) {
        let kept = p.foreign.as_deref().and_then(|f| f.kept.iter().rev().find(|c| rank_of(&c.name.ns, &c.name.local).is_some_and(|k| RPR[k] == key)));
        if let Some(c) = kept {
            let at = self.s.len();
            self.typed(p, key);
            let now = self.s.split_off(at);
            if self.read_as(c).is_some_and(|q| match same_value(p, &q, key) {
                Some(same) => same,
                None => {
                    let at = self.s.len();
                    self.typed(&q, key);
                    self.s.split_off(at) == now
                }
            }) {
                self.payload(c);
            } else {
                self.s.push_str(&now);
            }
            return;
        }
        self.typed(p, key);
    }

    /// The typed value read from kept child `c` alone.
    fn read_as(&self, c: &ForeignChild) -> Option<CharProps> {
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
        Some(read(doc.root_element(), None))
    }

    /// Modelled child `key` of `p` from its typed value.
    fn typed(&mut self, p: &CharProps, key: &'static str) {
        use super::write::drawing as fx;
        let cx = self.cx;
        let foreign = p.foreign.as_deref().map_or(&[][..], |f| &f.attributes);
        if let Some(local) = key.strip_prefix("w14:") {
            let effect = |e: &crate::docx::model::TextEffects| match local {
                "glow" => e.glow.as_ref().map(|g| fx::text_glow_xml(cx, g)),
                "shadow" => e.shadow.as_ref().map(|s| fx::text_shadow_xml(cx, s)),
                "reflection" => e.reflection.as_ref().map(|r| fx::text_reflection_xml(cx, r)),
                "textOutline" => e.outline.as_ref().map(|l| text_outline(cx, l)),
                // An empty text fill
                _ if e.is_empty() => Some(cx.w14("textFill", &[], "")),
                _ => e.fill.as_ref().map(|l| fx::text_fill_xml(cx, l)),
            };
            match local {
                "glow" | "shadow" | "reflection" | "textOutline" | "textFill" => {
                    if let Some(x) = p.effects.as_deref().and_then(effect) {
                        self.s.push_str(&x);
                    }
                    return;
                }
                "stylisticSets" => {
                    let Some(sets) = p.typography.stylistic_sets else { return };
                    let _ = write!(self.s, "<{}{local}{}", self.w14, self.w14_decl);
                    if sets == 0 {
                        self.s.push_str("/>");
                        return;
                    }
                    self.s.push('>');
                    for n in (1..=20).filter(|n| sets & 1 << (n - 1) != 0) {
                        let _ = write!(self.s, "<{p}styleSet {p}id=\"{n}\"/>", p = self.w14);
                    }
                    let _ = write!(self.s, "</{}{local}>", self.w14);
                    return;
                }
                _ => {}
            }
        }
        let (w14, local, ns) = match key.strip_prefix("w14:") {
            Some(local) => (true, local, W14),
            None => (false, key, W),
        };
        let at = self.s.len();
        match w14 {
            true => {
                let _ = write!(self.s, "<{}{local}{}", self.w14, self.w14_decl);
            }
            false => {
                let _ = write!(self.s, "<{}{local}", self.cx.w);
            }
        }
        let mut present = false;
        let written = self.attrs(w14, foreign, Some((ns, local)), |s| present = emit(p, key, s));
        if !present {
            self.s.truncate(at);
            return;
        }
        self.free_attrs(foreign, Some((ns, local)), &written);
        self.s.push_str("/>");
    }
}

/// Writes a modelled child's attributes into an open start tag.
struct Attrs<'o, 'a, 'f> {
    out: &'o mut Out<'a>,
    w14: bool,
    foreign: &'f [ForeignAttr],
    on: Option<(&'f str, &'f str)>,
    written: Vec<(&'static str, &'static str)>,
    scratch: String,
}

impl Attrs<'_, '_, '_> {
    /// The attribute read for modelled attribute `local` that is written otherwise, if any.
    fn as_read(&self, local: &str) -> Option<&ForeignAttr> {
        let names = |a: &&ForeignAttr| match &*a.name.ns {
            "" | W => a.name.local.as_ref() == local || local == "cstheme" && &*a.name.local == "csTheme",
            W14 => self.w14 && &*a.name.local == local,
            _ => false,
        };
        self.foreign.iter().filter(|a| a.on.as_ref().map(|q| (&*q.ns, &*q.local)) == self.on).find(names)
    }

    fn write(&mut self, a: Option<&ForeignAttr>, local: &'static str, v: &str) {
        let (prefix, local) = match a {
            Some(a) => (self.out.prefix_for(a), &*a.name.local),
            None => ((if self.w14 { &self.out.w14 } else { &self.out.cx.w }).clone(), local),
        };
        let s = &mut self.out.s;
        let _ = write!(s, " {prefix}{local}=\"");
        push_esc(s, v);
        s.push('"');
    }
}

impl Sink for Attrs<'_, '_, '_> {
    fn attr(&mut self, w14: bool, local: &'static str, v: &dyn Display) {
        let ns = if w14 { W14 } else { W };
        self.scratch.clear();
        let _ = write!(self.scratch, "{v}");
        let read = self.as_read(local).filter(|a| a.stands_for.as_deref() == Some(self.scratch.as_str())).cloned();
        let value = std::mem::take(&mut self.scratch);
        match &read {
            Some(a) => self.write(Some(a), local, &a.value),
            None => self.write(None, local, &value),
        }
        self.scratch = value;
        self.written.push((ns, local));
    }

    fn implied(&mut self, local: &'static str, means: &'static str) {
        if let Some(a) = self.as_read(local).filter(|a| a.stands_for.as_deref() == Some(means)).cloned() {
            self.write(Some(&a), local, &a.value);
            self.written.push((W, local));
        }
    }

    fn decided(&mut self, local: &'static str, _: &'static str, theme: Option<ThemeColourRef>) {
        let Some(r) = theme else { return };
        // The value as read goes with the theme colour that decided it.
        if let Some(a) = self.as_read(local).filter(|a| a.beside.is_some()).cloned() {
            self.write(Some(&a), local, &a.value);
            self.written.push((W, local));
            return;
        }
        let rgb = format!("{:06X}", self.out.colours.resolve(r));
        self.write(None, local, &rgb);
        self.written.push((W, local));
    }
}

/// Turns text effect `which` off at this level, over a style's, in Word's own form of no effect (a glow of no
/// radius, a shadow or reflection of no size, an outline with no fill), written in the part `cx` describes.
pub fn effect_off(e: &mut CharEdit, which: Effect, cx: &Cx) {
    let zeros = |names: &[&'static str]| names.iter().map(|&n| (n, "0")).chain([("algn", "none")]).collect::<Vec<_>>();
    let black = "<{w14}srgbClr {w14}val=\"000000\"/>";
    let payload = match which {
        Effect::Glow => cx.w14("glow", &[("rad", "0")], black),
        Effect::Shadow => cx.w14("shadow", &zeros(&["blurRad", "dist", "dir", "sx", "sy", "kx", "ky"]), black),
        Effect::Reflection => {
            cx.w14("reflection", &zeros(&["blurRad", "stA", "stPos", "endA", "endPos", "dist", "dir", "fadeDir", "sx", "sy", "kx", "ky"]), "")
        }
        Effect::Outline | Effect::Fill => cx.w14("textOutline", &[], "<{w14}noFill/>"),
    };
    let prefix = cx.prefix_of(W14).map_or_else(|| "w14".to_string(), |p| p.trim_end_matches(':').to_string());
    let wrapper = format!("<x xmlns:{prefix}=\"{W14}\">{payload}</x>");
    let Ok(doc) = roxmltree::Document::parse(&wrapper) else { return };
    let read = read(doc.root_element(), None);
    let (Some(now), Some(kept)) = (read.effects.as_deref(), read.foreign.as_ref().and_then(|f| f.kept.first())) else { return };
    let t = Rc::make_mut(e.effects.get_or_insert_with(Default::default));
    match which {
        Effect::Glow => t.glow = now.glow.clone(),
        Effect::Shadow => t.shadow = now.shadow.clone(),
        Effect::Reflection => t.reflection = now.reflection.clone(),
        Effect::Outline | Effect::Fill => t.outline = now.outline.clone(),
    }
    let f = Rc::make_mut(e.foreign.get_or_insert_with(Default::default));
    f.kept.retain(|c| c.name != kept.name);
    f.kept.push(kept.clone());
    e.whole(Prop::Effect(which));
}

/// A text outline as the outline command writes it: each of its settings given, a dash only when it is a preset.
pub fn outline_as_set(l: &crate::drawing::paint::Line) -> crate::drawing::paint::Line {
    use crate::drawing::paint::{Cap, Dash, Fill, Join, Line};
    Line {
        width: Some(l.width.unwrap_or(crate::drawing::units::Emu(9525))),
        fill: Some(l.fill.clone().unwrap_or(Fill::None)),
        dash: l.dash.clone().filter(|d| matches!(d, Dash::Preset(_))),
        compound: Some(l.compound.unwrap_or_default()),
        cap: Some(l.cap.unwrap_or(Cap::Flat)),
        join: Some(match l.join {
            Some(j @ (Join::Bevel | Join::Miter(_))) => j,
            _ => Join::Round,
        }),
        inset: Some(l.inset == Some(true)),
        head: None,
        tail: None,
    }
}

/// Whether the value child `key` holds in `p` is the one it holds in `q`, for a child whose writing depends on more
/// than its value (`None` for any other: their writing is compared).
fn same_value(p: &CharProps, q: &CharProps, key: &str) -> Option<bool> {
    use crate::docx::model::TextEffects;
    let effect = |same: fn(&TextEffects, &TextEffects) -> bool| Some(p.effects.as_deref().zip(q.effects.as_deref()).is_some_and(|(a, b)| same(a, b)));
    match key {
        "w14:textFill" => effect(|a, b| a.fill == b.fill),
        "w14:textOutline" => effect(|a, b| a.outline == b.outline),
        "w14:shadow" => effect(|a, b| a.shadow == b.shadow),
        "w14:glow" => effect(|a, b| a.glow == b.glow),
        "w14:reflection" => effect(|a, b| a.reflection == b.reflection),
        "w14:stylisticSets" => Some(p.typography.stylistic_sets.is_some() && p.typography.stylistic_sets == q.typography.stylistic_sets),
        _ => None,
    }
}

/// `w14:textOutline` with what `l` sets and nothing else.
fn text_outline(cx: &Cx, l: &crate::drawing::paint::Line) -> String {
    use crate::drawing::paint::{Cap, Compound, Dash, Join};
    let mut attrs: Vec<(&str, String)> = Vec::new();
    if let Some(w) = l.width {
        attrs.push(("w", w.0.to_string()));
    }
    if let Some(c) = l.cap {
        let v = match c {
            Cap::Round => "rnd",
            Cap::Square => "sq",
            Cap::Flat => "flat",
        };
        attrs.push(("cap", v.into()));
    }
    if let Some(c) = l.compound {
        let v = match c {
            Compound::Single => "sng",
            Compound::Double => "dbl",
            Compound::ThickThin => "thickThin",
            Compound::ThinThick => "thinThick",
            Compound::Triple => "tri",
        };
        attrs.push(("cmpd", v.into()));
    }
    if let Some(inset) = l.inset {
        attrs.push(("algn", if inset { "in" } else { "ctr" }.into()));
    }
    let mut inner = l.fill.as_ref().map(super::write::drawing::w14_fill_xml).unwrap_or_default();
    if let Some(Dash::Preset(d)) = &l.dash {
        let _ = write!(inner, r#"<{{w14}}prstDash {{w14}}val="{}"/>"#, d.name());
    }
    match l.join {
        Some(Join::Round) => inner.push_str("<{w14}round/>"),
        Some(Join::Bevel) => inner.push_str("<{w14}bevel/>"),
        Some(Join::Miter(lim)) => {
            let _ = write!(inner, r#"<{{w14}}miter {{w14}}lim="{}"/>"#, lim.0);
        }
        None => {}
    }
    let attrs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    cx.w14("textOutline", &attrs, &inner)
}

/// `v` as attribute text: `& < > "` escaped, characters XML can't hold left out.
fn push_esc(s: &mut String, v: &str) {
    for c in v.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            c if super::xml::is_char(c) => s.push(c),
            _ => {}
        }
    }
}

/// Whether an element as written declares `prefix` itself.
fn declares(payload: &str, prefix: &str) -> bool {
    let tag = payload.split('>').next().unwrap_or_default();
    if prefix.is_empty() { tag.contains("xmlns=") } else { tag.contains(&format!("xmlns:{prefix}=")) }
}

/// Points as twips, in a universal measure when twips can't hold them exactly.
struct Twips(f64);

impl Display for Twips {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = (self.0 * 20.0).round();
        if t / 20.0 == self.0 { write!(f, "{t}") } else { write!(f, "{}pt", self.0) }
    }
}

/// Points as half-points, in a universal measure when half-points can't hold them exactly.
struct HalfPoints(f64);

impl Display for HalfPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let h = self.0 * 2.0;
        if h.fract() == 0.0 { write!(f, "{h}") } else { write!(f, "{}pt", self.0) }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::document::format::{Emphasis, ShadingPattern};
    use crate::document::line::BorderStyle;
    use crate::docx::model::Underline;

    const NS: &str =
        r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;

    fn model(name: &str) -> crate::docx::Document {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model").join(name);
        crate::docx::read_editable(&std::fs::read(&path).expect("reads")).expect("opens")
    }

    /// Typed properties and revisions written in `to` parse to themselves there; the XML path reads the same from the
    /// written element as from `rpr`.
    fn round_trip(rpr: Node, colours: &ThemeColours, to: &Cx) -> (CharProps, RprRevisions, String) {
        let (p, rev) = parse(rpr);
        let xml = serialize(&p, &rev, colours, to);
        let again = to.parse(&xml, |_, el| parse(el)).unwrap_or_else(|| panic!("{xml} parses"));
        assert_eq!(again, (p.clone(), rev.clone()), "{xml}");
        let base = to.parse(&xml, |_, el| RunProps::parse(Some(el), colours)).expect("parses");
        assert_eq!(base, RunProps::parse(Some(rpr), colours), "{xml}");
        (p, rev, xml)
    }

    /// Every `w:rPr` of a model fixture's main part as written (a formatting change's earlier one with its change),
    /// round tripped in the part itself and in `to` (else the part).
    fn fixture(name: &str, to: Option<&Cx>) -> Vec<(CharProps, RprRevisions, String)> {
        let d = model(name);
        let src = &d.source.as_ref().expect("a source").xml;
        let colours = d.resolver.as_ref().expect("a resolver").styles.colours;
        let own = Cx::of_xml(src);
        let doc = roxmltree::Document::parse(src).expect("parses");
        let rprs =
            doc.descendants().filter(|n| super::super::xml::is_w(*n, "rPr") && !super::super::xml::is_w(n.parent().expect("a parent"), "rPrChange"));
        rprs.map(|n| {
            round_trip(n, &colours, &own);
            round_trip(n, &colours, to.unwrap_or(&own))
        })
        .collect()
    }

    fn parsed(x: &str) -> (CharProps, RprRevisions) {
        let doc = roxmltree::Document::parse(x).expect("parses");
        parse(doc.root_element())
    }

    #[test]
    fn theme_references_survive_as_references() {
        let read = fixture("theme-references.docx", None);
        let themed = |c: Option<Color>, tint: bool| matches!(c, Some(Color::Theme(r)) if if tint { r.tint.is_some() } else { r.shade.is_some() });
        for tint in [true, false] {
            assert!(read.iter().any(|(p, ..)| themed(p.color, tint)), "a text colour");
            assert!(read.iter().any(|(p, ..)| themed(p.underline.and_then(|u| u.color), tint)), "an underline colour");
        }
        assert!(read.iter().any(|(p, ..)| p.shading.is_some_and(|s| themed(s.fill, true) || themed(s.color, false))), "a shading");
        assert!(read.iter().any(|(p, ..)| themed(p.border.and_then(|b| b.color), false)), "a border");
        let theme = |f: &Option<FontRef>| matches!(f, Some(FontRef::Theme { .. }));
        assert!(
            read.iter().any(|(p, ..)| [&p.fonts.latin, &p.fonts.latin_extended, &p.fonts.east_asian, &p.fonts.complex].into_iter().all(theme)),
            "all four slots"
        );
        let (_, _, xml) = read.iter().find(|(p, ..)| themed(p.color, false)).expect("a shaded text colour");
        assert!(xml.contains(r#"w:themeColor="accent1" w:themeShade="BF""#), "{xml}");
    }

    #[test]
    fn unset_off_and_automatic_stay_apart() {
        let read = fixture("explicit-off.docx", None);
        let any = |f: &dyn Fn(&CharProps) -> bool| read.iter().any(|(p, ..)| f(p));
        assert!(any(&|p| p.toggles.get(Toggle::BOLD) == Some(false)), "bold off");
        assert!(any(&|p| p.toggles.get(Toggle::HIDDEN) == Some(false)), "hidden off");
        assert!(any(&|p| p.underline.is_some_and(|u| u.style == Underline::None)), "underline none");
        assert!(any(&|p| p.color == Some(Color::Auto)), "colour automatic");
        assert!(any(&|p| p.highlight == Some(None)), "highlight none");
        assert!(any(&|p| p.shading.is_some_and(|s| s.pattern == ShadingPattern::Nil)), "shading nil");
        assert!(any(&|p| p.shading.is_some_and(|s| s.pattern == ShadingPattern::Clear && s.fill == Some(Color::Auto))), "shading clear");
        let (p, rev) = parsed(&format!(r#"<w:rPr {NS}><w:b w:val="0"/><w:color w:val="auto"/><w:highlight w:val="none"/></w:rPr>"#));
        let cx = Cx::of_xml(&format!("<w:document {NS}>"));
        let xml = serialize(&p, &rev, &ThemeColours::default(), &cx);
        assert_eq!(xml, r#"<w:rPr><w:b w:val="0"/><w:color w:val="auto"/><w:highlight w:val="none"/></w:rPr>"#);
        assert_eq!(serialize(&CharProps::default(), &RprRevisions::default(), &ThemeColours::default(), &cx), "<w:rPr></w:rPr>");
    }

    #[test]
    fn unmodelled_children_and_attributes_are_kept_in_their_place() {
        let read = fixture("unknown-attributes.docx", None);
        let foreign = |p: &CharProps| p.foreign.as_deref().map_or((0, 0), |f| (f.children.len(), f.attributes.len()));
        assert!(read.iter().any(|(p, ..)| foreign(p) == (4, 1)), "webHidden, effect, props3d and glint, and the colour's tone");
        assert!(read.iter().any(|(p, ..)| p.lang.complex.as_deref() == Some("he-IL")), "the bidi language");
        let ns = format!(r#"{NS} xmlns:tx="http://example.com/twip/ext""#);
        let x = format!(
            r#"<w:rPr {ns}><w:b/><w:webHidden/><w:color w:val="2E74B5" tx:tone="cool"/><tx:glint tx:val="soft"/><w:sz w:val="24"/><w:effect w:val="sparkle"/><w:lang w:val="en-US" w:bidi="he-IL"/><w14:props3d w14:extrusionH="0"/></w:rPr>"#
        );
        let (p, rev) = parsed(&x);
        let xml = serialize(&p, &rev, &ThemeColours::default(), &Cx::of_xml(&format!("<w:document {ns}>")));
        // In schema order; the extension element, which CT_RPr doesn't name, after the modelled children.
        let want = r#"<w:rPr><w:b/><w:webHidden/><w:color w:val="2E74B5" tx:tone="cool"/><w:sz w:val="24"/><w:effect w:val="sparkle"/><w:lang w:val="en-US" w:bidi="he-IL"/><w14:props3d w14:extrusionH="0"/><tx:glint tx:val="soft"/></w:rPr>"#;
        assert_eq!(xml, want);
        // An invalid literal reads as layout's fallback.
        let (p, _) = parsed(&format!(r#"<w:rPr {NS}><w:em w:val="sparkle"/><w:shd w:val="pct200"/><w:u w:val="squiggle"/></w:rPr>"#));
        assert_eq!(
            (p.emphasis, p.shading.map(|s| s.pattern), p.underline.map(|u| u.style)),
            (Some(Emphasis::Dot), Some(ShadingPattern::Nil), Some(Underline::Single))
        );
    }

    #[test]
    fn a_part_writes_its_own_prefixes() {
        let q = Cx::of_xml(r#"<q:document xmlns:q="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
        let read = fixture("namespace-dependencies.docx", Some(&q));
        assert!(read.iter().any(|(p, ..)| p.typography.ligatures == Some(3) && p.effects.is_some()), "the w14 children bound on the paragraph");
        for (_, _, xml) in &read {
            assert!(xml.starts_with("<q:rPr") && !xml.contains("<w:") && !xml.contains("<x:"), "{xml}");
        }
        let (_, _, xml) = read.iter().find(|(p, ..)| p.typography.ligatures.is_some()).expect("ligatures");
        assert!(xml.starts_with(r#"<q:rPr xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">"#), "{xml}");
    }

    #[test]
    fn an_unmodelled_childs_namespace_is_declared_where_the_part_lacks_it() {
        let ext = r#"xmlns:tx="urn:tx""#;
        let x = format!(r#"<w:rPr {NS} {ext}><tx:glint tx:val="soft"/><w:b tx:weight="9"/></w:rPr>"#);
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let to = Cx::of_xml(&format!("<w:document {NS}>"));
        let (_, _, xml) = round_trip(doc.root_element(), &ThemeColours::default(), &to);
        assert_eq!(xml, r#"<w:rPr xmlns:tx="urn:tx"><w:b tx:weight="9"/><tx:glint tx:val="soft"/></w:rPr>"#);
        // A prefix the part gives its main namespace is the child's own; a namespace the part has under another
        // prefix keeps the one written.
        let x = format!(r#"<w:rPr {NS} xmlns:q="urn:tx"><q:glint q:val="soft"/><w:b q:weight="9"/></w:rPr>"#);
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let to = Cx::of_xml(r#"<q:document xmlns:q="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
        let (p, rev) = parse(doc.root_element());
        let xml = serialize(&p, &rev, &ThemeColours::default(), &to);
        assert_eq!(xml, r#"<q:rPr xmlns:ns0="urn:tx"><q:b ns0:weight="9"/><q:glint xmlns:q="urn:tx" q:val="soft"/></q:rPr>"#);
        let names = |p: &CharProps| p.foreign.as_ref().map(|f| (f.children[0].name.clone(), f.attributes[0].name.clone()));
        assert_eq!(to.parse(&xml, |_, el| names(&parse(el).0)).flatten(), names(&p));
    }

    #[test]
    fn revisions_come_beside_the_properties_and_write_first_and_last() {
        let x = format!(
            r#"<w:rPr {NS}><w:ins w:id="1" w:author="A &amp; B" w:date="2024-01-02T03:04:05Z"/><w:b/><w:rPrChange w:id="2" w:author="C"><w:rPr><w:i/><w14:ligatures w14:val="standard"/></w:rPr></w:rPrChange></w:rPr>"#
        );
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let to = Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
        let (p, rev, xml) = round_trip(doc.root_element(), &ThemeColours::default(), &to);
        assert_eq!(p.toggles.get(Toggle::BOLD), Some(true));
        assert_eq!(rev.ins.as_ref().and_then(|i| i.author.as_deref()), Some("A & B"));
        let change = rev.change.as_ref().expect("a formatting change");
        assert_eq!((change.previous.toggles.get(Toggle::ITALIC), change.previous.typography.ligatures), (Some(true), Some(1)));
        assert_eq!(
            xml,
            r#"<w:rPr><w:ins w:id="1" w:author="A &amp; B" w:date="2024-01-02T03:04:05Z"/><w:b/><w:rPrChange w:id="2" w:author="C"><w:rPr><w:i/><w14:ligatures xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" w14:val="standard"/></w:rPr></w:rPrChange></w:rPr>"#
        );
    }

    /// What of `input` (every element, its text and each attribute with its value, by its path of expanded names)
    /// `output` lacks.
    pub(crate) fn lost(input: Node, output: Node) -> Vec<String> {
        fn items(root: Node) -> std::collections::HashMap<String, usize> {
            let mut out = std::collections::HashMap::new();
            for d in root.descendants().filter(|d| d.is_element()) {
                let path: Vec<String> = d
                    .ancestors()
                    .take_while(|a| *a != root)
                    .map(|a| format!("{{{}}}{}", a.tag_name().namespace().unwrap_or_default(), a.tag_name().name()))
                    .collect();
                let path = path.into_iter().rev().collect::<Vec<_>>().join("/");
                let text: String = d.children().filter_map(|c| c.text().filter(|_| c.is_text())).collect();
                *out.entry(format!("{path} {:?}", text.trim())).or_default() += 1;
                for a in d.attributes() {
                    *out.entry(format!("{path} @{{{}}}{}={:?}", a.namespace().unwrap_or_default(), a.name(), a.value())).or_default() += 1;
                }
            }
            out
        }
        let (had, has) = (items(input), items(output));
        let mut lost: Vec<String> = had.into_iter().filter(|(k, n)| has.get(k).is_none_or(|m| m < n)).map(|(k, _)| k).collect();
        lost.sort();
        lost
    }

    /// `x` (an `rPr` in `NS`) written again; what the writing lost of it.
    fn rewritten(x: &str, edit: impl FnOnce(&mut CharProps)) -> (String, Vec<String>) {
        let doc = roxmltree::Document::parse(x).expect("parses");
        let (mut p, rev) = parse(doc.root_element());
        edit(&mut p);
        let cx = Cx::of_xml(&format!("<w:document {NS}>"));
        let xml = serialize(&p, &rev, &ThemeColours::default(), &cx);
        let lost = cx.parse(&xml, |_, el| lost(doc.root_element(), el)).expect("the writing parses");
        (xml, lost)
    }

    #[test]
    fn values_written_otherwise_are_written_as_read_until_they_change() {
        let x = format!(
            r#"<w:rPr {NS}><w:b w:val="true"/><w:spacing w:val="7.5"/><w:sz w:val="21.5"/><w:bdr w:val="dotted" w:sz="0.5" w:space="2"/><w:color w:val="ff0000"/></w:rPr>"#
        );
        assert_eq!(rewritten(&x, |_| {}).1, Vec::<String>::new());
        let (xml, _) = rewritten(&x, |p| {
            p.toggles.put(Toggle::BOLD, false);
            p.spacing = Some(0.33);
            p.size = Some(12.0);
            p.color = Some(Color::Rgb(0x00FF00));
        });
        assert_eq!(
            xml,
            r#"<w:rPr><w:b w:val="0"/><w:color w:val="00FF00"/><w:spacing w:val="0.33pt"/><w:sz w:val="24"/><w:bdr w:val="dotted" w:sz="0.5" w:space="2"/></w:rPr>"#
        );
        let (xml, _) = rewritten(&x, |p| p.toggles = Toggles::default());
        assert!(!xml.contains("<w:b ") && !xml.contains("<w:b/"), "an unset toggle leaves its value as read behind: {xml}");
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let (p, _) = parse(doc.root_element());
        assert_eq!(p.border.map(|b| (b.style, b.width)), Some((BorderStyle::Dotted, 0.125)));
    }

    #[test]
    fn values_a_theme_decides_are_kept_beside_it() {
        let x = format!(
            r#"<w:rPr {NS}><w:rFonts w:ascii="Calibri" w:asciiTheme="minorHAnsi" w:eastAsia="MS Mincho" w:eastAsiaTheme="majorEastAsia"/><w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/><w:u w:val="single" w:color="C45911" w:themeColor="accent2" w:themeShade="BF"/><w:shd w:val="clear" w:color="auto" w:fill="FFD966" w:themeFill="accent4" w:themeFillTint="66"/></w:rPr>"#
        );
        assert_eq!(rewritten(&x, |_| {}).1, Vec::<String>::new());
        let (xml, _) = rewritten(&x, |p| {
            p.fonts.latin = Some(FontRef::Family("Arial".into()));
            p.fonts.east_asian = None;
            p.color = Some(Color::Rgb(0x112233));
        });
        assert!(xml.contains(r#"<w:rFonts w:ascii="Arial" w:hAnsi"#) || xml.contains(r#"w:ascii="Arial""#), "{xml}");
        assert!(!xml.contains("Calibri") && !xml.contains("Mincho") && !xml.contains("2F5496"), "a value its theme decided goes with it: {xml}");
        assert!(xml.contains(r#"<w:color w:val="112233"/>"#), "{xml}");
    }

    #[test]
    fn invalid_literals_read_as_the_fallback_and_write_as_read() {
        let x =
            format!(r#"<w:rPr {NS}><w:rFonts w:asciiTheme="bogus"/><w:em w:val="sparkle"/><w:shd w:val="pct200"/><w:u w:val="squiggle"/></w:rPr>"#);
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let (p, _) = parse(doc.root_element());
        assert_eq!(
            (p.emphasis, p.shading.map(|s| s.pattern), p.underline.map(|u| u.style), p.fonts.latin),
            (
                Some(Emphasis::Dot),
                Some(ShadingPattern::Nil),
                Some(Underline::Single),
                Some(FontRef::Theme { face: ThemeFace::Unrecognised, script: ScriptClass::Latin })
            )
        );
        assert_eq!(rewritten(&x, |_| {}).1, Vec::<String>::new());
        let (xml, _) = rewritten(&x, |p| p.emphasis = Some(Emphasis::Circle));
        assert!(xml.contains(r#"<w:em w:val="circle"/>"#), "{xml}");
    }

    #[test]
    fn content_the_parse_doesnt_read_is_kept_while_its_value_stays() {
        let outline =
            r#"<w14:textOutline w14:w="9525"><w14:solidFill><w14:srgbClr w14:val="C00000"/></w14:solidFill><w14:future/></w14:textOutline>"#;
        let x = format!(r#"<w:rPr {NS}><w:b><w:note/></w:b>{outline}<w14:textFill><w14:blipFill/></w14:textFill><w14:glow w14:rad="1"/></w:rPr>"#);
        assert_eq!(rewritten(&x, |_| {}).1, Vec::<String>::new());
        let (xml, _) = rewritten(&x, |p| {
            let fx = Rc::make_mut(p.effects.as_mut().expect("effects"));
            fx.outline.as_mut().expect("an outline").width = Some(crate::drawing::units::Emu(12700));
        });
        assert!(xml.contains(r#"<w14:textOutline w14:w="12700">"#) && !xml.contains("future"), "{xml}");
        assert!(xml.contains("<w14:blipFill/>") && xml.contains("<w:note/>"), "the others stay as read: {xml}");
    }

    #[test]
    fn earlier_duplicates_are_kept_and_the_last_still_counts() {
        let x = format!(r#"<w:rPr {NS}><w:b/><w:sz w:val="20"/><w:b w:val="0"/></w:rPr>"#);
        let (xml, lost) = rewritten(&x, |_| {});
        assert_eq!(lost, Vec::<String>::new());
        let doc = roxmltree::Document::parse(&x).expect("parses");
        let cx = Cx::of_xml(&format!("<w:document {NS}>"));
        let again = cx.parse(&xml, |_, el| parse(el).0).expect("parses");
        assert_eq!(again.toggles.get(Toggle::BOLD), Some(false), "{xml}");
        assert_eq!(
            cx.parse(&xml, |_, el| RunProps::parse(Some(el), &ThemeColours::default())),
            Some(RunProps::parse(Some(doc.root_element()), &ThemeColours::default()))
        );
    }

    #[test]
    fn text_effects_read_back_as_written() {
        let effects = [
            r#"<w14:textOutline><w14:noFill/></w14:textOutline>"#,
            r#"<w14:textOutline w14:w="12700" w14:algn="in"><w14:solidFill><w14:schemeClr w14:val="accent1"><w14:lumMod w14:val="75000"/></w14:schemeClr></w14:solidFill><w14:prstDash w14:val="dash"/><w14:bevel/></w14:textOutline>"#,
            r#"<w14:textFill/>"#,
            r#"<w14:textFill><w14:gradFill><w14:gsLst><w14:gs w14:pos="0"><w14:srgbClr w14:val="FF0000"/></w14:gs><w14:gs w14:pos="100000"><w14:srgbClr w14:val="0000FF"/></w14:gs></w14:gsLst><w14:lin w14:ang="5400000" w14:scaled="0"/></w14:gradFill></w14:textFill>"#,
            r#"<w14:glow w14:rad="63500"><w14:schemeClr w14:val="accent2"><w14:alpha w14:val="60000"/></w14:schemeClr></w14:glow>"#,
            r#"<w14:shadow/>"#,
            r#"<w14:reflection w14:blurRad="6350" w14:stA="55000" w14:endA="300" w14:endPos="45500" w14:dir="5400000" w14:sy="-100000" w14:algn="bl"/>"#,
        ];
        let cx = Cx::of_xml(&format!("<w:document {NS}>"));
        for e in effects {
            let x = format!("<w:rPr {NS}>{e}</w:rPr>");
            let doc = roxmltree::Document::parse(&x).expect("parses");
            let (p, ..) = round_trip(doc.root_element(), &ThemeColours::default(), &cx);
            assert!(p.effects.is_some(), "{e}");
        }
    }

    #[test]
    fn an_edit_writes_over_the_xml_it_was_read_from() {
        let ns = format!(r#"{NS} xmlns:tx="urn:tx""#);
        let cx = Cx::of_xml(&format!("<w:document {ns}>"));
        let x = r#"<w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsi="Calibri"/><w:b w:val="true"/><w:u w:val="single" tx:x="1"/><w:sz w:val="24"/></w:rPr>"#;
        let d = RunDirect::from_xml(Some(x), &cx);
        let read = || CharEdit::new((**d.props().expect("properties")).clone());
        let write = |e: CharEdit| d.edit(e).xml(&ThemeColours::default(), &cx).map(std::borrow::Cow::into_owned);
        assert_eq!(write(read()).as_deref(), Some(x), "as read");
        // A font set in part keeps the others in their places; the underline set whole loses what it held beside its
        // value; the size removed.
        let mut e = read();
        e.fonts.latin = Some(FontRef::Family("Arial".into()));
        e.part(Prop::Font(ScriptClass::Latin));
        e.underline = Some(UnderlineProps { style: Underline::Double, color: None });
        e.whole(Prop::Underline);
        e.size = None;
        e.whole(Prop::Size);
        let want = r#"<w:rPr><w:rFonts w:hAnsi="Calibri" w:ascii="Arial"/><w:b w:val="true"/><w:u w:val="double"/></w:rPr>"#;
        assert_eq!(write(e).as_deref(), Some(want));
        // What is set whole is written anew though its value stays; fonts set whole come in the order set.
        let mut e = read();
        e.toggle(Toggle::BOLD, true);
        let f = Some(FontRef::Family("Arial".into()));
        e.fonts = ScriptFonts { latin: f.clone(), latin_extended: f.clone(), east_asian: f.clone(), complex: f };
        for c in [ScriptClass::Latin, ScriptClass::LatinExtended, ScriptClass::Complex, ScriptClass::EastAsian] {
            e.whole(Prop::Font(c));
        }
        let want = r#"<w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:cs="Arial" w:eastAsia="Arial"/><w:b/><w:u w:val="single" tx:x="1"/><w:sz w:val="24"/></w:rPr>"#;
        assert_eq!(write(e).as_deref(), Some(want));
        // A child new to it goes in schema order; nothing left, no element.
        let mut e = read();
        e.toggle(Toggle::ITALIC, true);
        let want = r#"<w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsi="Calibri"/><w:b w:val="true"/><w:i/><w:u w:val="single" tx:x="1"/><w:sz w:val="24"/></w:rPr>"#;
        assert_eq!(write(e).as_deref(), Some(want));
        assert_eq!(write(CharEdit::default()), None);
    }

    /// An unmodelled flag put on reads as written, in the part's prefix, and taken off leaves nothing.
    #[test]
    fn unmodelled_flags_put_on_read_as_written() {
        for root in [format!("<w:document {NS}>"), format!("<document xmlns=\"{W}\">")] {
            let cx = Cx::of_xml(&root);
            let mut p = CharProps::default();
            put_unmodelled_flag(&mut p, "webHidden", &cx);
            put_unmodelled_flag(&mut p, "webHidden", &cx);
            let xml = serialize(&p, &RprRevisions::default(), &ThemeColours::default(), &cx);
            assert_eq!(xml, format!("<{w}rPr><{w}webHidden/></{w}rPr>", w = cx.w));
            assert_eq!(cx.parse(&xml, |_, el| parse(el).0), Some(p.clone()), "{xml}");
            drop_unmodelled(&mut p, "webHidden");
            assert_eq!(p, CharProps::default());
        }
    }
}
