//! styles.xml and the theme's fonts: style chains and the cascade of STYLE-1–STYLE-4.

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
    /// The size a cell's text takes when its size comes from a 12pt default paragraph style (STYLE-4): the active
    /// regions' and table style's `w:sz`, else docDefaults', unless that is 10pt. None in the default table style, in
    /// one that sets no paragraph, run or region property, and in compat 15.
    pub size_over_normal: Option<f64>,
    /// The region's `w:pPr` or `w:rPr` holds a property (STYLE-4).
    pub sets_text: bool,
}

impl Region {
    fn parse(n: Node) -> Region {
        Region {
            ppr: ParaProps::parse(child(n, "pPr")),
            rpr: RunProps::parse(child(n, "rPr")),
            tbl: TblProps::parse(child(n, "tblPr")),
            tc: TcProps::parse(child(n, "tcPr")),
            tr: TrProps::parse(child(n, "trPr")),
            size_over_normal: None,
            sets_text: ["pPr", "rPr"].iter().any(|k| child(n, k).is_some_and(|e| e.children().any(|c| c.is_element()))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Style {
    pub id: String,
    /// `w:name`, and whether it is a quick style (`w:qFormat`): the Styles gallery.
    pub name: String,
    pub quick: bool,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    /// `w:next`: the style of the paragraph Enter starts at the end of one in this style.
    pub next: Option<String>,
    pub default: bool,
    /// `w:uiPriority` (lists sort by it), `w:semiHidden` or `w:hidden`, and `w:customStyle` (not built in).
    pub priority: Option<u32>,
    pub hidden: bool,
    pub custom: bool,
    pub base: Region,
    /// Conditional regions by `w:type` (`firstRow`, `band1Horz`, ...).
    pub regions: HashMap<String, Region>,
}

/// Table-style conditional regions, lowest precedence first (TABLE-3).
pub const REGION_ORDER: [&str; 12] =
    ["band1Horz", "band2Horz", "band1Vert", "band2Vert", "firstCol", "lastCol", "firstRow", "lastRow", "nwCell", "neCell", "swCell", "seCell"];

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
    pub by_id: HashMap<String, Style>,
    /// Paragraph style ids in styles.xml order (STYLE-6 linkage).
    pub paragraph_order: Vec<String>,
    pub default_paragraph: Option<String>,
    pub default_table: Option<String>,
    pub doc_rpr: RunProps,
    pub doc_ppr: ParaProps,
    /// compat 15: no table-style size exception (STYLE-4).
    pub compat15: bool,
}

impl Styles {
    pub fn parse(xml: Option<&str>) -> Styles {
        let mut s = Styles::default();
        let doc = xml.and_then(|x| roxmltree::Document::parse(x).ok());
        let Some(doc) = doc else {
            s.app_defaults();
            return s;
        };
        let root = doc.root_element();
        match child(root, "docDefaults") {
            Some(dd) => {
                s.doc_rpr = RunProps::parse(child(dd, "rPrDefault").and_then(|n| child(n, "rPr")));
                s.doc_ppr = ParaProps::parse(child(dd, "pPrDefault").and_then(|n| child(n, "pPr")));
            }
            None => s.app_defaults(),
        }
        for n in children(root, "style") {
            let Some(id) = attr(n, "styleId") else { continue };
            let kind = match attr(n, "type") {
                Some("character") => StyleKind::Character,
                Some("table") => StyleKind::Table,
                Some("numbering") => StyleKind::Numbering,
                _ => StyleKind::Paragraph,
            };
            let default = attr(n, "default").map(|v| on_off_str(Some(v))).unwrap_or(false);
            let mut regions = HashMap::new();
            for r in children(n, "tblStylePr") {
                if let Some(t) = attr(r, "type") {
                    regions.insert(t.to_string(), Region::parse(r));
                }
            }
            let style = Style {
                id: id.to_string(),
                name: child(n, "name").and_then(val).unwrap_or("").to_string(),
                quick: child(n, "qFormat").is_some(),
                kind,
                based_on: child(n, "basedOn").and_then(val).map(str::to_string),
                next: child(n, "next").and_then(val).map(str::to_string),
                default,
                priority: child(n, "uiPriority").and_then(val).and_then(|v| v.parse().ok()),
                hidden: child(n, "semiHidden").is_some_and(on_off) || child(n, "hidden").is_some_and(on_off),
                custom: attr(n, "customStyle").is_some_and(|v| on_off_str(Some(v))),
                base: Region::parse(n),
                regions,
            };
            if kind == StyleKind::Paragraph {
                s.paragraph_order.push(style.id.clone());
                if default {
                    // STYLE-2: with several marked defaults the last one wins.
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
        s
    }

    /// STYLE-2: Word's own defaults when a document has no docDefaults (this build: Aptos 12pt, after 8pt, line 1.16).
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

    /// The paragraph style a paragraph uses (STYLE-2): its `pStyle` if that exists, else the default paragraph style.
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
            cur = s.based_on.as_deref().and_then(|b| self.by_id.get(b));
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

    /// A paragraph or character style chain as one level for toggles (STYLE-3).
    pub fn chain_rpr(&self, id: Option<&str>) -> RunProps {
        let chain = self.chain(id);
        RunProps::fold_chain(chain.iter().map(|s| &s.base.rpr))
    }

    /// The table style a table uses: its `tblStyle`, else the default table style ("Normal Table", TABLE-2).
    pub fn table_style<'a>(&'a self, id: Option<&'a str>) -> Option<&'a str> {
        match id {
            Some(id) if self.by_id.contains_key(id) => Some(id),
            _ => self.default_table.as_deref(),
        }
    }

    /// A table style's properties for one cell: the base region, then the active conditional regions in precedence order
    /// (TABLE-3), each through the `basedOn` chain. Toggles don't XOR between regions.
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
            add(&s.base, &mut out);
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
        // STYLE-4's exception (compat ≤ 14): a table style that sets some paragraph, run or region property.
        let sets = chain.iter().any(|s| s.base.sets_text || !s.regions.is_empty());
        if !self.compat15 && id.is_some() && id != self.default_table.as_deref() && sets {
            out.size_over_normal = out.rpr.size.or(self.doc_rpr.size).filter(|s| *s != 10.0);
        }
        out
    }

    /// Which regions the style defines anywhere in its chain.
    pub fn table_defines(&self, id: Option<&str>, region: &str) -> bool {
        self.chain(id).iter().any(|s| s.regions.contains_key(region))
    }
}
