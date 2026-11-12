//! Styles

use super::model::Compat;
use std::collections::HashMap;

use roxmltree::Node;

use super::props::*;
use super::xml::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
}

/// One `w:tblStylePr` region (or a table style's base properties).
#[derive(Clone, Debug, Default)]
pub struct Region {
    pub ppr: ParaProps,
    pub rpr: RunProps,
    pub tbl: TblProps,
    pub tc: TcProps,
    pub tr: TrProps,
    pub size_over_normal: Option<f64>,
    pub sets_text: bool,
    pub mark_at_style: bool,
}

impl Region {
    fn parse(n: Node, colours: &ThemeColours) -> Region {
        Region {
            ppr: ParaProps::parse(child(n, "pPr"), colours),
            rpr: RunProps::parse(child(n, "rPr"), colours),
            tbl: TblProps::parse(child(n, "tblPr"), colours),
            tc: TcProps::parse(child(n, "tcPr"), colours),
            tr: TrProps::parse(child(n, "trPr")),
            size_over_normal: None,
            sets_text: ["pPr", "rPr"].iter().any(|k| child(n, k).is_some_and(|e| e.children().any(|c| c.is_element()))),
            mark_at_style: false,
        }
    }
}

/// Which styles part a style was read from: the document's, or the glossary's (a style only a building block body
/// uses, kept in the document's table under an id that doesn't collide).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StyleOrigin {
    #[default]
    Document,
    Glossary,
}

/// A style's flags for style lists and galleries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleMeta {
    pub semi_hidden: bool,
    pub hidden: bool,
    pub unhide_when_used: bool,
    pub locked: bool,
    pub auto_redefine: bool,
}

#[derive(Clone, Debug)]
pub struct Style {
    pub id: String,
    /// `w:name`'s first name; the others are `aliases`. And whether it is a quick style (`w:qFormat`): the Styles
    /// gallery.
    pub name: String,
    pub aliases: Vec<String>,
    pub quick: bool,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    /// `w:next`: the style of the paragraph Enter starts at the end of one in this style.
    pub next: Option<String>,
    pub default: bool,
    /// `w:link`: the linked character or paragraph style.
    pub link: Option<String>,
    /// `w:uiPriority` (lists sort by it), the list flags, and `w:customStyle` (not built in).
    pub priority: Option<u32>,
    pub meta: StyleMeta,
    pub custom: bool,
    pub base: Region,
    /// Conditional regions by `w:type` (`firstRow`, `band1Horz`, ...).
    pub regions: HashMap<String, Region>,
    pub origin: StyleOrigin,
    /// The `w:style` element as written: what saving writes for it, and what edits change.
    pub raw: String,
    /// A table style whose properties start from Word's defaults, not its base: it was read before its base and kept
    /// what it had then ([`Styles::parse`]).
    pub frozen: bool,
}

impl Style {
    /// Hidden from style lists: `w:semiHidden` or `w:hidden`.
    pub fn hidden(&self) -> bool {
        self.meta.semi_hidden || self.meta.hidden
    }

    fn parse(n: Node, text: &str, colours: &ThemeColours) -> Option<Style> {
        let id = attr(n, "styleId")?;
        let kind = match attr(n, "type") {
            Some("character") => StyleKind::Character,
            Some("table") => StyleKind::Table,
            Some("numbering") => StyleKind::Numbering,
            _ => StyleKind::Paragraph,
        };
        let mut regions = HashMap::new();
        for r in children(n, "tblStylePr") {
            if let Some(t) = attr(r, "type") {
                regions.insert(t.to_string(), Region::parse(r, colours));
            }
        }
        let mut names = child(n, "name").and_then(val).unwrap_or("").split(',').map(|a| a.trim().to_string());
        let flag = |name: &str| child(n, name).is_some_and(on_off);
        Some(Style {
            id: id.to_string(),
            name: names.next().unwrap_or_default(),
            aliases: names.filter(|a| !a.is_empty()).collect(),
            quick: flag("qFormat"),
            kind,
            based_on: child(n, "basedOn").and_then(val).map(str::to_string),
            next: child(n, "next").and_then(val).map(str::to_string),
            default: attr(n, "default").map(|v| on_off_str(Some(v))).unwrap_or(false),
            link: child(n, "link").and_then(val).map(str::to_string),
            priority: child(n, "uiPriority").and_then(val).and_then(|v| v.parse().ok()),
            meta: StyleMeta {
                semi_hidden: flag("semiHidden"),
                hidden: flag("hidden"),
                unhide_when_used: flag("unhideWhenUsed"),
                locked: flag("locked"),
                auto_redefine: flag("autoRedefine"),
            },
            custom: attr(n, "customStyle").is_some_and(|v| on_off_str(Some(v))),
            base: Region::parse(n, colours),
            regions,
            origin: StyleOrigin::Document,
            raw: text[n.range()].to_string(),
            frozen: false,
        })
    }
}

/// Reading styles.xml in order, Word keeps each style's full properties once worked out, and works them out again
/// after the last style only for those whose own properties were stored since. A table style worked out while its base
/// wasn't read yet keeps what it had then: Word's defaults and its own properties.
#[derive(Default)]
struct Expansions {
    /// Per style read: its base if read before it, and whether its properties are worked out.
    read: HashMap<String, (Option<String>, bool)>,
}

impl Expansions {
    /// Works the style's properties out, and its bases' that aren't on the way.
    fn expand(&mut self, id: &str) {
        let mut cur = Some(id.to_string());
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

    /// Replays the reader's steps for a table style: own run, paragraph and table properties are stored at its first
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

/// A built-in style the document doesn't define: `w:latentStyles`' exceptions, with its defaults for the rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatentStyle {
    pub name: String,
    pub priority: Option<u32>,
    pub semi_hidden: Option<bool>,
    pub unhide_when_used: Option<bool>,
    pub quick: Option<bool>,
    pub locked: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatentStyles {
    /// The defaults: priority, semi-hidden, unhide when used, quick, locked.
    pub priority: Option<u32>,
    pub semi_hidden: bool,
    pub unhide_when_used: bool,
    pub quick: bool,
    pub locked: bool,
    pub exceptions: Vec<LatentStyle>,
    /// The `w:latentStyles` element as written.
    pub raw: Option<String>,
}

impl LatentStyles {
    fn parse(n: Node, text: &str) -> LatentStyles {
        let on = |n: Node, a: &str| attr(n, a).map(|v| on_off_str(Some(v)));
        LatentStyles {
            priority: attr(n, "defUIPriority").and_then(|v| v.parse().ok()),
            semi_hidden: on(n, "defSemiHidden").unwrap_or(false),
            unhide_when_used: on(n, "defUnhideWhenUsed").unwrap_or(false),
            quick: on(n, "defQFormat").unwrap_or(false),
            locked: on(n, "defLockedState").unwrap_or(false),
            exceptions: children(n, "lsdException")
                .filter_map(|e| {
                    Some(LatentStyle {
                        name: attr(e, "name")?.to_string(),
                        priority: attr(e, "uiPriority").and_then(|v| v.parse().ok()),
                        semi_hidden: on(e, "semiHidden"),
                        unhide_when_used: on(e, "unhideWhenUsed"),
                        quick: on(e, "qFormat"),
                        locked: on(e, "locked"),
                    })
                })
                .collect(),
            raw: Some(text[n.range()].to_string()),
        }
    }
}

/// Why a style edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleEditError {
    /// The XML isn't one `w:style` element with a `w:styleId`.
    Malformed,
    /// It would be based on itself, through its chain.
    Loop,
    /// No such style.
    Missing,
}

pub const REGION_ORDER: [&str; 12] =
    ["band1Horz", "band2Horz", "band1Vert", "band2Vert", "lastCol", "firstCol", "lastRow", "firstRow", "seCell", "swCell", "neCell", "nwCell"];

#[derive(Clone, Debug, Default)]
pub struct ThemeFonts {
    pub major: String,
    pub minor: String,
    pub major_ea: Option<String>,
    pub minor_ea: Option<String>,
    pub major_cs: Option<String>,
    pub minor_cs: Option<String>,
}

impl ThemeFonts {
    pub fn parse(theme: Option<&str>) -> ThemeFonts {
        let mut t = ThemeFonts { major: "Calibri Light".into(), minor: "Calibri".into(), ..Default::default() };
        let Some(xml) = theme else { return t };
        let Ok(doc) = roxmltree::Document::parse(xml) else { return t };
        let face = |scheme: Node, kind: &str| {
            scheme.children().find(|c| is(*c, A, kind)).and_then(|n| n.attribute("typeface")).filter(|s| !s.is_empty()).map(str::to_string)
        };
        for n in doc.descendants().filter(|n| is(*n, A, "majorFont") || is(*n, A, "minorFont")) {
            let major = n.tag_name().name() == "majorFont";
            let (latin, ea, cs) = (face(n, "latin"), face(n, "ea"), face(n, "cs"));
            if major {
                if let Some(l) = latin {
                    t.major = l;
                }
                t.major_ea = ea;
                t.major_cs = cs;
            } else {
                if let Some(l) = latin {
                    t.minor = l;
                }
                t.minor_ea = ea;
                t.minor_cs = cs;
            }
        }
        t
    }

    /// A `w:*Theme` value as a family name.
    /// An East Asian theme slot without a typeface ([`Self::resolve`] gives the Latin font).
    pub fn ea_unset(&self, theme: &str) -> bool {
        theme.ends_with("EastAsia") && if theme.starts_with("major") { self.major_ea.is_none() } else { self.minor_ea.is_none() }
    }

    pub fn resolve(&self, theme: &str) -> String {
        let major = theme.starts_with("major");
        let (latin, ea, cs) = if major { (&self.major, &self.major_ea, &self.major_cs) } else { (&self.minor, &self.minor_ea, &self.minor_cs) };
        if theme.ends_with("EastAsia") {
            ea.clone().unwrap_or_else(|| latin.clone())
        } else if theme.ends_with("Bidi") {
            cs.clone().unwrap_or_else(|| latin.clone())
        } else {
            latin.clone()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Styles {
    pub colours: ThemeColours,
    pub by_id: HashMap<String, Style>,
    pub paragraph_order: Vec<String>,
    pub default_paragraph: Option<String>,
    pub default_table: Option<String>,
    pub doc_rpr: RunProps,
    pub doc_ppr: ParaProps,
    pub compat: Compat,
    pub word2002_rules: bool,
    /// A 12pt Normal gives way to a table style's size in cells, and an 11pt cell mark takes its paragraph's:
    /// compat ≤ 14 without `overrideTableStyleFontSizeAndJustification`.
    pub cell_size_rules: bool,
    /// OpenType features (ligatures, number forms and spacing, stylistic sets) apply: compat 15, or
    /// `enableOpenTypeFeatures`.
    pub open_type: bool,
    pub latent: LatentStyles,
    /// The part's root start tag, whose namespace declarations a style's XML is read and written under.
    pub root: String,
    /// The `w:docDefaults` element as written.
    pub defaults_raw: Option<String>,
}

impl Styles {
    pub fn parse(xml: Option<&str>, colours: ThemeColours) -> Styles {
        let mut s = Styles { colours, ..Styles::default() };
        let doc = xml.and_then(|x| roxmltree::Document::parse(x).ok());
        let Some(doc) = doc else {
            s.app_defaults();
            return s;
        };
        let text = xml.unwrap_or_default();
        let root = doc.root_element();
        s.root = text[root.range().start..root.range().start + text[root.range().start..].find('>').map_or(0, |e| e + 1)].to_string();
        s.latent = child(root, "latentStyles").map(|n| LatentStyles::parse(n, text)).unwrap_or_default();
        s.defaults_raw = child(root, "docDefaults").map(|n| text[n.range()].to_string());
        match child(root, "docDefaults") {
            Some(dd) => {
                s.app_defaults();
                if let Some(r) = child(dd, "rPrDefault") {
                    s.doc_rpr = RunProps::parse(child(r, "rPr"), &colours);
                }
                if let Some(p) = child(dd, "pPrDefault") {
                    s.doc_ppr = ParaProps::parse(child(p, "pPr"), &colours);
                }
            }
            None => s.app_defaults(),
        }
        let mut expansions = Expansions::default();
        for n in children(root, "style") {
            let Some(mut style) = Style::parse(n, text, &colours) else { continue };
            let (kind, default) = (style.kind, style.default);
            let base = style.based_on.clone().filter(|b| *b != style.id && expansions.read.contains_key(b));
            expansions.read.entry(style.id.clone()).or_insert((base, false));
            if kind == StyleKind::Table && expansions.table_style(n, &style.id) {
                // Word's built-in run properties, the document's fonts kept.
                let r = &mut style.base.rpr;
                r.size = r.size.or(Some(10.0));
                r.color = r.color.or(Some(None));
                r.spacing = r.spacing.or(Some(0.0));
                r.scale = r.scale.or(Some(100.0));
            }
            if kind == StyleKind::Paragraph {
                s.paragraph_order.push(style.id.clone());
                if default {
                    s.default_paragraph = Some(style.id.clone());
                }
            }
            if kind == StyleKind::Table && default && s.default_table.is_none() {
                s.default_table = Some(style.id.clone());
            }
            s.by_id.insert(style.id.clone(), style);
        }
        if s.default_paragraph.is_none() && s.by_id.get("Normal").is_some_and(|st| st.kind == StyleKind::Paragraph) {
            s.default_paragraph = Some("Normal".into());
        }
        // Worked out with its base missing: frozen. The styles on one take what it has.
        for (id, (base, done)) in expansions.read {
            if let Some(st) = s.by_id.get_mut(&id).filter(|st| st.kind == StyleKind::Table && st.based_on.is_some()) {
                st.frozen = done && base.is_none();
            }
        }
        s
    }

    /// Replaces the document defaults from their `w:docDefaults` XML. Every style's properties change.
    pub fn set_defaults_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError> {
        let cx = self.cx();
        let colours = self.colours;
        let (rpr, ppr) = cx
            .parse(&raw, |_, n| {
                let rpr = child(n, "rPrDefault").map(|r| RunProps::parse(child(r, "rPr"), &colours));
                let ppr = child(n, "pPrDefault").map(|p| ParaProps::parse(child(p, "pPr"), &colours));
                (rpr, ppr)
            })
            .ok_or(StyleEditError::Malformed)?;
        if let Some(r) = rpr {
            self.doc_rpr = r;
        }
        if let Some(p) = ppr {
            self.doc_ppr = p;
        }
        self.defaults_raw = Some(raw);
        let mut all: Vec<String> = self.by_id.keys().cloned().collect();
        all.sort();
        Ok(all)
    }

    /// The prefix and declarations a style's XML is written with.
    pub fn cx(&self) -> super::write::props::Cx {
        if self.root.is_empty() {
            return super::write::props::Cx::of_xml(&format!("<w:styles xmlns:w=\"{}\">", super::write::props::W_NS));
        }
        super::write::props::Cx::of_xml(&self.root)
    }

    /// The styles whose properties come from `id`'s: it, and every style based on it, directly or not.
    pub fn dependants(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = self.by_id.values().filter(|s| self.chain(Some(&s.id)).iter().any(|c| c.id == id)).map(|s| s.id.clone()).collect();
        out.sort();
        out
    }

    /// Adds a style, or replaces one, from its `w:style` XML. Returns the styles whose properties changed: it and its
    /// dependants (with the ones of the style it replaced).
    pub fn set_xml(&mut self, raw: String) -> Result<Vec<String>, StyleEditError> {
        let cx = self.cx();
        let style = cx.parse(&raw, |text, n| Style::parse(n, text, &self.colours)).flatten().ok_or(StyleEditError::Malformed)?;
        let style = Style { raw, ..style };
        let mut base = style.based_on.as_deref();
        let mut hops = 0;
        while let Some(b) = base {
            if b == style.id || hops > 32 {
                return Err(StyleEditError::Loop);
            }
            base = self.by_id.get(b).and_then(|s| s.based_on.as_deref());
            hops += 1;
        }
        let id = style.id.clone();
        let mut changed = if self.by_id.contains_key(&id) { self.dependants(&id) } else { Vec::new() };
        if style.kind == StyleKind::Paragraph && !self.paragraph_order.contains(&id) {
            self.paragraph_order.push(id.clone());
        }
        self.by_id.insert(id.clone(), style);
        changed.extend(self.dependants(&id));
        changed.sort();
        changed.dedup();
        Ok(changed)
    }

    /// Removes a style. The styles based on it take its base, and references to it from `w:next` and `w:link` go.
    /// Returns the styles whose properties changed: it (its users fall back) and its dependants.
    pub fn delete(&mut self, id: &str) -> Result<Vec<String>, StyleEditError> {
        let gone = self.by_id.get(id).ok_or(StyleEditError::Missing)?.clone();
        let changed = self.dependants(id);
        let cx = self.cx();
        let refs: Vec<String> = self
            .by_id
            .values()
            .filter(|s| s.id != id && (s.based_on.as_deref() == Some(id) || s.next.as_deref() == Some(id) || s.link.as_deref() == Some(id)))
            .map(|s| s.id.clone())
            .collect();
        for r in refs {
            let Some(s) = self.by_id.get(&r) else { continue };
            let mut el = super::write::props::Props::parse(Some(&s.raw), "style", &cx);
            let w = &cx.w;
            if s.based_on.as_deref() == Some(id) {
                match &gone.based_on {
                    Some(b) => el.set("basedOn", format!("<{w}basedOn {w}val=\"{}\"/>", super::write::props::esc(b))),
                    None => el.remove("basedOn"),
                }
            }
            if s.next.as_deref() == Some(id) {
                el.remove("next");
            }
            if s.link.as_deref() == Some(id) {
                el.remove("link");
            }
            el.reorder(super::write::props::STYLE);
            if let Some(raw) = el.xml() {
                self.set_xml(raw)?;
            }
        }
        self.by_id.remove(id);
        self.paragraph_order.retain(|p| p != id);
        if self.default_paragraph.as_deref() == Some(id) {
            self.default_paragraph = None;
        }
        Ok(changed)
    }

    fn app_defaults(&mut self) {
        let mut r = RunProps::default();
        let aptos = Some(FontRef::Name("Aptos".into()));
        r.ascii = aptos.clone();
        r.h_ansi = aptos.clone();
        r.east_asia = aptos.clone();
        r.cs = aptos;
        r.size = Some(12.0);
        r.kern = Some(1.0);
        self.doc_rpr = r;
        self.doc_ppr = ParaProps { space_after: Some(8.0), line: Some(278.0), line_rule: Some(LineRule::Auto), ..Default::default() };
    }

    pub fn get(&self, id: &str) -> Option<&Style> {
        self.by_id.get(id)
    }

    pub fn paragraph_style<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        match id {
            Some(id) if self.by_id.get(id).is_some_and(|s| s.kind == StyleKind::Paragraph) => Some(id),
            _ => self.default_paragraph.as_deref(),
        }
    }

    /// A style and its `basedOn` ancestors, root first.
    pub fn chain(&self, id: Option<&str>) -> Vec<&Style> {
        let mut out = Vec::new();
        let mut cur = id.and_then(|i| self.by_id.get(i));
        while let Some(s) = cur {
            if out.iter().any(|o: &&Style| o.id == s.id) || out.len() > 32 {
                break;
            }
            out.push(s);
            cur = s.based_on.as_deref().filter(|_| !s.frozen).and_then(|b| self.by_id.get(b));
        }
        out.reverse();
        out
    }

    pub fn chain_ppr(&self, id: Option<&str>) -> ParaProps {
        let mut p = ParaProps::default();
        for s in self.chain(id) {
            p.merge(&s.base.ppr);
        }
        p
    }

    pub fn chain_rpr(&self, id: Option<&str>) -> RunProps {
        let chain = self.chain(id);
        RunProps::fold_chain(chain.iter().map(|s| &s.base.rpr))
    }

    pub fn table_style<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        match id {
            Some(id) if self.by_id.contains_key(id) => Some(id),
            _ => self.default_table.as_deref(),
        }
    }

    pub fn table_region(&self, id: Option<&str>, active: &[&str]) -> Region {
        let chain = self.chain(id);
        let mut out = Region::default();
        let mut rprs: Vec<&RunProps> = Vec::new();
        let add = |r: &'_ Region, out: &mut Region| {
            out.ppr.merge(&r.ppr);
            out.tbl.merge(&r.tbl);
            out.tc.merge(&r.tc);
            out.tr.merge(&r.tr);
        };
        for s in &chain {
            // A style's own cell borders and `noWrap` don't apply; its own table borders replace its base's.
            let tc = TcProps { borders: Borders::default(), no_wrap: None, ..s.base.tc.clone() };
            if s.base.tbl.borders != Borders::default() {
                out.tbl.borders = Borders::default();
            }
            add(&Region { tc, ..s.base.clone() }, &mut out);
            rprs.push(&s.base.rpr);
        }
        for name in REGION_ORDER.iter().filter(|n| active.contains(n)) {
            for s in &chain {
                if let Some(r) = s.regions.get(*name) {
                    add(r, &mut out);
                    rprs.push(&r.rpr);
                }
            }
        }
        out.rpr = RunProps::fold_chain(rprs.into_iter());
        let sets = chain.iter().any(|s| s.base.sets_text || !s.regions.is_empty());
        if self.cell_size_rules && id.is_some() && id != self.default_table.as_deref() && sets {
            out.size_over_normal = out.rpr.size.or(self.doc_rpr.size).filter(|s| *s != 10.0);
        }
        out.mark_at_style = self.cell_size_rules
            && (id.is_none() || id == self.default_table.as_deref())
            && self.doc_rpr.size == Some(11.0)
            && self.chain_rpr(self.default_paragraph.as_deref()).size == Some(12.0);
        out
    }

    /// Which regions the style defines anywhere in its chain.
    pub fn table_defines(&self, id: Option<&str>, region: &str) -> bool {
        self.chain(id).iter().any(|s| s.regions.contains_key(region))
    }
}
