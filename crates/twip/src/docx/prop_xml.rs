//! Property elements written from typed properties and compared with the XML they were read from: what the typed
//! property adapters (`w:rPr`, `w:pPr`) share. A modelled child's attributes are given to a [`Sink`], which either
//! compares them with an element as read ([`Compare`]: what differs is kept beside the typed value) or writes them
//! ([`Out`]: each as read while it still stands for the value read).

use std::fmt::{Display, Write as _};
use std::rc::Rc;

use roxmltree::Node;

use super::model::ThemeColourRef;
use super::props::{ThemeColours, theme_name, theme_ref};
use super::write::props::Cx;
use super::xml::{W, W14, W15, attr, color, num, val};
use crate::format::{Color, ForeignAttr, ForeignChild, ForeignProps, QName, TextBorder};

pub(super) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// Word 2010's namespace declared on an element of it.
pub(super) const W14_DECL: &str = " xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"";

/// Word 2012's namespace declared on an element of it.
pub(super) const W15_DECL: &str = " xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"";

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

/// A border around text or a paragraph (`w:bdr`, a side of `w:pBdr`).
pub(super) fn border(e: Node) -> TextBorder {
    // Eighths of a point
    let sz = match num(attr(e, "sz")) {
        Some(v) if v > 0.0 && v < 1.0 => 1.0,
        Some(v) => v.clamp(0.0, 1584.0 * 8.0),
        None => 0.0,
    };
    TextBorder {
        style: val(e).map_or(crate::document::line::BorderStyle::None, super::line_style::border_style),
        width: sz / 8.0,
        space: num(attr(e, "space")).unwrap_or(0.0),
        shadow: matches!(attr(e, "shadow"), Some("1" | "true" | "on")),
        color: colour(e, LINE_COLOUR),
    }
}

/// A border's attributes.
pub(super) fn border_attrs(s: &mut dyn Sink, b: &TextBorder) {
    s.attr(W, "val", &super::line_style::border_val(b.style));
    s.attr(W, "sz", &(b.width * 8.0));
    s.attr(W, "space", &b.space);
    if let Some(c) = b.color {
        colour_attrs(s, c, LINE_COLOUR);
    }
    if b.shadow {
        s.attr(W, "shadow", &1);
    } else {
        s.implied("shadow", "0");
    }
}

/// The colour attributes of a colour: value, theme colour, tint, shade.
pub(super) type ColourAttrs = [&'static str; 4];

pub(super) const COLOUR: ColourAttrs = ["val", "themeColor", "themeTint", "themeShade"];

pub(super) const LINE_COLOUR: ColourAttrs = ["color", "themeColor", "themeTint", "themeShade"];

pub(super) const FILL: ColourAttrs = ["fill", "themeFill", "themeFillTint", "themeFillShade"];

pub(super) fn revision_attrs(e: Node, scratch: &mut String) -> RevisionAttrs {
    let a = |n: &str| attr(e, n).map(Rc::from);
    let mut r = RevisionAttrs { id: a("id"), author: a("author"), date: a("date"), foreign: Vec::new() };
    let mut cmp = Compare::new(e, scratch);
    emit_revision(&r, &mut cmp);
    cmp.finish(None, &mut r.foreign);
    r
}

/// An element as written, with the bindings of the namespaces its names use.
pub(super) fn foreign_child(e: Node) -> ForeignChild {
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
pub(super) fn colour(e: Node, [hex, theme, tint, shade]: ColourAttrs) -> Option<Color> {
    match theme_ref(e, theme, tint, shade) {
        Some(r) => Some(Color::Theme(r)),
        None => attr(e, hex).map(|v| color(Some(v)).map_or(Color::Auto, Color::Rgb)),
    }
}

/// Where a modelled child's attributes go: written, or compared with an element as read.
pub(super) trait Sink {
    /// Attribute `local` in namespace `ns` (WordprocessingML's or an extension's) with value `v`.
    fn attr(&mut self, ns: &'static str, local: &'static str, v: &dyn Display);
    /// Attribute `local` left out, which reads as `means`.
    fn implied(&mut self, local: &'static str, means: &'static str);
    /// Attribute `local`, which `by` decides: a colour's value beside its theme colour (written as `theme` resolves),
    /// a font's name beside its theme font.
    fn decided(&mut self, local: &'static str, by: &'static str, theme: Option<ThemeColourRef>);
}

pub(super) fn colour_attrs(s: &mut dyn Sink, c: Color, [hex, theme, tint, shade]: ColourAttrs) {
    match c {
        Color::Auto => s.attr(W, hex, &"auto"),
        Color::Rgb(v) => s.attr(W, hex, &format_args!("{v:06X}")),
        Color::Theme(r) => {
            s.decided(hex, theme, Some(r));
            if let Some(name) = theme_name(r.slot) {
                s.attr(W, theme, &name);
            }
            if let Some(t) = r.tint {
                s.attr(W, tint, &format_args!("{t:02X}"));
            }
            if let Some(v) = r.shade {
                s.attr(W, shade, &format_args!("{v:02X}"));
            }
        }
    }
}

pub(super) fn emit_revision(r: &RevisionAttrs, s: &mut dyn Sink) {
    for (name, v) in [("id", &r.id), ("author", &r.author), ("date", &r.date)] {
        if let Some(v) = v {
            s.attr(W, name, &&**v);
        }
    }
}

/// The other names a modelled attribute may be written under, the parse preferring the first: `w:csTheme` before
/// `w:cstheme`, `w:left` before `w:start`, `w:right` before `w:end`.
fn names(local: &str) -> &'static [&'static str] {
    match local {
        "cstheme" => &["csTheme", "cstheme"],
        "left" => &["left", "start"],
        "right" => &["right", "end"],
        _ => &[],
    }
}

/// The attribute of `e` the parse reads for modelled attribute `local`, of a child in namespace `ns`: in `ns`, else
/// WordprocessingML's, else in none, under each of its [`names`] in turn.
pub(super) fn read_attr(e: Node, ns: &str, local: &str) -> Option<usize> {
    let find = |ns: Option<&str>, local: &str| e.attributes().position(|a| a.namespace() == ns && a.name() == local);
    let one = |name: &str| (ns != W).then(|| find(Some(ns), name)).flatten().or_else(|| find(Some(W), name)).or_else(|| find(None, name));
    match names(local) {
        [] => one(local),
        all => all.iter().find_map(|n| one(n)),
    }
}

/// Compares the attributes a modelled child is written with against an element as read: the attributes that differ
/// or that aren't written are the element's foreign ones.
pub(super) struct Compare<'a, 'i, 's> {
    e: Node<'a, 'i>,
    scratch: &'s mut String,
    /// The attributes written as read, by index (past the 64th, none).
    same: u64,
    /// The attributes written otherwise, by index, with the value they stand for, or beside the attribute deciding
    /// them.
    ties: Vec<(usize, Option<Rc<str>>, Option<Rc<str>>)>,
}

impl<'a, 'i, 's> Compare<'a, 'i, 's> {
    pub(super) fn new(e: Node<'a, 'i>, scratch: &'s mut String) -> Self {
        Compare { e, scratch, same: 0, ties: Vec::new() }
    }

    pub(super) fn finish(self, on: Option<QName>, out: &mut Vec<ForeignAttr>) {
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
    fn attr(&mut self, ns: &'static str, local: &'static str, v: &dyn Display) {
        let Some((i, a)) = read_attr(self.e, ns, local).and_then(|i| Some((i, self.e.attributes().nth(i)?))) else { return };
        self.scratch.clear();
        let _ = write!(self.scratch, "{v}");
        let own = a.namespace() == Some(ns) && a.name() == local;
        if own && a.value() == self.scratch.as_str() {
            if i < 64 {
                self.same |= 1 << i;
            }
        } else {
            self.ties.push((i, Some(self.scratch.as_str().into()), None));
        }
    }

    fn implied(&mut self, local: &'static str, means: &'static str) {
        if let Some(i) = read_attr(self.e, W, local) {
            self.ties.push((i, Some(means.into()), None));
        }
    }

    fn decided(&mut self, local: &'static str, by: &'static str, _: Option<ThemeColourRef>) {
        if let Some(i) = read_attr(self.e, W, local) {
            self.ties.push((i, None, Some(by.into())));
        }
    }
}

/// An element with no content as written: its name and each attribute's name and quoted value.
pub(super) fn attr_list(x: &str) -> Option<(String, Vec<(String, String)>)> {
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

/// The span of attribute `name` in `tag` as `(start of its leading space, start of its value, end of its value)`,
/// written with either quote; other attributes' values are skipped.
pub(super) fn attr_span(tag: &str, name: &str) -> Option<(usize, usize, usize)> {
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
pub(super) fn with_attr_written(tag: &str, name: &str, quoted: &str) -> String {
    with_attr_text(tag, name, quoted.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(quoted))
}

pub(super) fn with_attr_text(tag: &str, name: &str, v: &str) -> String {
    if let Some((_, from, to)) = attr_span(tag, name) {
        // Its own quotes kept: a single-quoted value's apostrophes escaped
        let v = if tag[..from].ends_with('\'') { v.replace('\'', "&apos;") } else { v.to_string() };
        return format!("{}{v}{}", &tag[..from], &tag[to..]);
    }
    let at = tag.rfind("/>").or_else(|| tag.rfind('>')).unwrap_or(tag.len());
    format!("{} {name}=\"{v}\"{}", &tag[..at], &tag[at..])
}

/// A child's key among a properties element's ([`super::write::props::Props`]): its local name in WordprocessingML,
/// Word 2010's and 2012's with their prefix, any other with its namespace.
pub(super) fn props_key(name: &QName) -> String {
    match &*name.ns {
        W | "" => name.local.to_string(),
        W14 => format!("w14:{}", name.local),
        super::xml::W15 => format!("w15:{}", name.local),
        ns => format!("{{{ns}}}{}", name.local),
    }
}

pub(super) struct Out<'a> {
    pub(super) s: String,
    pub(super) cx: &'a Cx,
    pub(super) colours: &'a ThemeColours,
    /// Word 2010's prefix, with its colon.
    pub(super) w14: String,
    /// Its declaration, on each element of it, when the part's root lacks one.
    pub(super) w14_decl: &'static str,
    /// Word 2012's prefix, with its colon.
    pub(super) w15: String,
    /// Its declaration, as for Word 2010's.
    pub(super) w15_decl: &'static str,
    /// The bindings declared on the properties element.
    pub(super) declared: Vec<(Rc<str>, Rc<str>)>,
    /// Unmodelled attributes' namespaces written under a prefix of their own, to keep clear of the writer's.
    pub(super) renamed: Vec<(Rc<str>, String)>,
}

impl<'a> Out<'a> {
    /// A writer for the part `cx` describes; a theme colour's value is written as `colours` resolve it.
    pub(super) fn new(cx: &'a Cx, colours: &'a ThemeColours) -> Out<'a> {
        let (w14, w15) = (cx.prefix_of(W14), cx.prefix_of(W15));
        Out {
            s: String::new(),
            cx,
            colours,
            w14_decl: if w14.is_some() { "" } else { W14_DECL },
            w14: w14.unwrap_or_else(|| "w14:".into()),
            w15_decl: if w15.is_some() { "" } else { W15_DECL },
            w15: w15.unwrap_or_else(|| "w15:".into()),
            declared: Vec::new(),
            renamed: Vec::new(),
        }
    }

    /// The prefix, with its colon, a modelled name in `ns` is written with.
    pub(super) fn prefix(&self, ns: &str) -> String {
        match ns {
            W14 => self.w14.clone(),
            W15 => self.w15.clone(),
            _ => self.cx.w.clone(),
        }
    }

    /// Whether the part's root binds `prefix` to `uri`.
    pub(super) fn rooted(&self, prefix: &str, uri: &str) -> bool {
        if prefix.is_empty() {
            return self.cx.w.is_empty() && uri == W;
        }
        self.cx.prefix_of(uri).is_some_and(|p| p.strip_suffix(':') == Some(prefix))
    }

    /// Whether `prefix` is one the writer itself uses, for a namespace other than `uri`.
    pub(super) fn clashes(&self, prefix: &str, uri: &str) -> bool {
        let w = self.cx.w.strip_suffix(':').unwrap_or_default();
        let w14 = self.w14.strip_suffix(':').unwrap_or_default();
        (prefix == w && uri != W) || (prefix == w14 && uri != W14) || self.declared.iter().any(|(p, u)| &**p == prefix && &**u != uri)
    }

    pub(super) fn declare(&mut self, prefix: &str, uri: &str, decls: &mut String) {
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
    pub(super) fn declarations(&mut self, foreign: Option<&ForeignProps>, decls: &mut String) {
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

    pub(super) fn declare_attr(&mut self, a: &ForeignAttr, decls: &mut String) {
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
    pub(super) fn prefix_for(&self, a: &ForeignAttr) -> String {
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
    pub(super) fn free_attrs(&mut self, attrs: &[ForeignAttr], on: Option<(&str, &str)>, written: &[(&str, &str)]) {
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

    /// An element as written, with any binding it needs that neither the part nor the `w:rPr` declares.
    pub(super) fn payload(&mut self, c: &ForeignChild) {
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

    /// Writes the attributes `emit` gives into the open start tag: each modelled one as read while it still stands
    /// for the value read. Returns the names written.
    pub(super) fn attrs(
        &mut self,
        ns: &'static str,
        foreign: &[ForeignAttr],
        on: Option<(&str, &str)>,
        emit: impl FnOnce(&mut dyn Sink),
    ) -> Vec<(&'static str, &'static str)> {
        let mut sink = Attrs { out: self, ns, foreign, on, written: Vec::new(), scratch: String::new() };
        emit(&mut sink);
        sink.written
    }
}

/// Writes a modelled child's attributes into an open start tag.
pub(super) struct Attrs<'o, 'a, 'f> {
    out: &'o mut Out<'a>,
    ns: &'static str,
    foreign: &'f [ForeignAttr],
    on: Option<(&'f str, &'f str)>,
    written: Vec<(&'static str, &'static str)>,
    scratch: String,
}

impl Attrs<'_, '_, '_> {
    /// The attribute read for modelled attribute `local` that is written otherwise, if any.
    fn as_read(&self, local: &str) -> Option<&ForeignAttr> {
        let names = |a: &&ForeignAttr| match &*a.name.ns {
            "" | W => &*a.name.local == local || names(local).contains(&&*a.name.local),
            ns => ns == self.ns && &*a.name.local == local,
        };
        self.foreign.iter().filter(|a| a.on.as_ref().map(|q| (&*q.ns, &*q.local)) == self.on).find(names)
    }

    fn write(&mut self, a: Option<&ForeignAttr>, local: &'static str, v: &str) {
        let (prefix, local) = match a {
            Some(a) => (self.out.prefix_for(a), &*a.name.local),
            None => (self.out.prefix(self.ns), local),
        };
        let s = &mut self.out.s;
        let _ = write!(s, " {prefix}{local}=\"");
        push_esc(s, v);
        s.push('"');
    }
}

impl Sink for Attrs<'_, '_, '_> {
    fn attr(&mut self, ns: &'static str, local: &'static str, v: &dyn Display) {
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

/// `v` as attribute text: `& < > "` escaped, characters XML can't hold left out.
pub(super) fn push_esc(s: &mut String, v: &str) {
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
pub(super) fn declares(payload: &str, prefix: &str) -> bool {
    let tag = payload.split('>').next().unwrap_or_default();
    if prefix.is_empty() { tag.contains("xmlns=") } else { tag.contains(&format!("xmlns:{prefix}=")) }
}

/// Points as twips, in a universal measure when twips can't hold them exactly.
pub(super) struct Twips(pub(super) f64);

impl Display for Twips {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let t = (self.0 * 20.0).round();
        if t / 20.0 == self.0 { write!(f, "{t}") } else { write!(f, "{}pt", self.0) }
    }
}

/// Points as half-points, in a universal measure when half-points can't hold them exactly.
pub(super) struct HalfPoints(pub(super) f64);

impl Display for HalfPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let h = self.0 * 2.0;
        if h.fract() == 0.0 { write!(f, "{h}") } else { write!(f, "{}pt", self.0) }
    }
}
