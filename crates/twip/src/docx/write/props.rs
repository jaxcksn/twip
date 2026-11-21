//! Property elements as written (`w:rPr`, `w:pPr`, ...): children kept as their XML, set and removed in schema order, so
//! saving changes only what an edit changed.

pub const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// CT_RPr's children in schema order (change tracking marks first: a paragraph mark's properties).
pub const RPR: &[&str] = &[
    "ins",
    "del",
    "moveFrom",
    "moveTo",
    "rStyle",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "color",
    "spacing",
    "w",
    "kern",
    "position",
    "sz",
    "szCs",
    "highlight",
    "u",
    "effect",
    "bdr",
    "shd",
    "fitText",
    "vertAlign",
    "rtl",
    "cs",
    "em",
    "lang",
    "eastAsianLayout",
    "specVanish",
    "oMath",
    // Word 2010's, after the base properties.
    "w14:glow",
    "w14:shadow",
    "w14:reflection",
    "w14:textOutline",
    "w14:textFill",
    "w14:scene3d",
    "w14:props3d",
    "w14:ligatures",
    "w14:numForm",
    "w14:numSpacing",
    "w14:stylisticSets",
    "w14:cntxtAlts",
    "rPrChange",
];

/// CT_Style's children in schema order.
pub const STYLE: &[&str] = &[
    "name",
    "aliases",
    "basedOn",
    "next",
    "link",
    "autoRedefine",
    "hidden",
    "uiPriority",
    "semiHidden",
    "unhideWhenUsed",
    "qFormat",
    "locked",
    "personal",
    "personalCompose",
    "personalReply",
    "rsid",
    "pPr",
    "rPr",
    "tblPr",
    "trPr",
    "tcPr",
    "tblStylePr",
];

/// CT_TcPr's children in schema order.
pub const TCPR: &[&str] = &[
    "cnfStyle",
    "tcW",
    "gridSpan",
    "hMerge",
    "vMerge",
    "tcBorders",
    "shd",
    "noWrap",
    "tcMar",
    "textDirection",
    "tcFitText",
    "vAlign",
    "hideMark",
    "headers",
    "cellIns",
    "cellDel",
    "cellMerge",
    "tcPrChange",
];

/// CT_TcBorders' children in schema order.
pub const TC_BORDERS: &[&str] = &["top", "start", "left", "bottom", "end", "right", "insideH", "insideV", "tl2br", "tr2bl"];

/// CT_TblPr's children in schema order.
pub const TBLPR: &[&str] = &[
    "tblStyle",
    "tblpPr",
    "tblOverlap",
    "bidiVisual",
    "tblStyleRowBandSize",
    "tblStyleColBandSize",
    "tblW",
    "jc",
    "tblCellSpacing",
    "tblInd",
    "tblBorders",
    "shd",
    "tblLayout",
    "tblCellMar",
    "tblLook",
    "tblCaption",
    "tblDescription",
    "tblPrChange",
];

/// CT_TrPr's children in schema order.
pub const TRPR: &[&str] = &[
    "cnfStyle",
    "divId",
    "gridBefore",
    "gridAfter",
    "wBefore",
    "wAfter",
    "cantSplit",
    "trHeight",
    "tblHeader",
    "tblCellSpacing",
    "jc",
    "hidden",
    "ins",
    "del",
    "trPrChange",
];

/// CT_PPr's children in schema order.
pub const PPR: &[&str] = &[
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
    "pPrChange",
];

/// How XML for the main part is written: its WordprocessingML prefix (`w:`) and its root's namespace declarations.
pub struct Cx {
    pub w: String,
    ns: String,
}

impl Cx {
    pub fn of(src: &super::super::Source) -> Cx {
        Cx::of_xml(&src.xml)
    }

    /// As [`Cx::of`] for any part's XML (a header's).
    pub fn of_xml(x: &str) -> Cx {
        let start = x.char_indices().find(|&(i, c)| c == '<' && x[i + 1..].starts_with(|c: char| c.is_alphabetic())).map_or(0, |(i, _)| i);
        let tag = &x[start..start + x[start..].find('>').unwrap_or(0)];
        let ns: Vec<&str> = tag.split_whitespace().skip(1).filter(|a| a.starts_with("xmlns")).map(|a| a.trim_end_matches('/')).collect();
        let w = ns
            .iter()
            .find(|a| a.ends_with(&format!("\"{W_NS}\"")))
            .and_then(|a| a.strip_prefix("xmlns:").map(|p| format!("{}:", &p[..p.find('=').unwrap_or(0)])).or(Some(String::new())))
            .unwrap_or_else(|| "w:".into());
        Cx { w, ns: ns.join(" ") }
    }

    /// The prefix the part's root gives `uri`, with its colon.
    pub fn prefix_of(&self, uri: &str) -> Option<String> {
        self.ns
            .split_whitespace()
            .find(|a| a.ends_with(&format!("\"{uri}\"")))
            .and_then(|a| a.strip_prefix("xmlns:"))
            .map(|a| format!("{}:", &a[..a.find('=').unwrap_or(0)]))
    }

    /// An element of the Word 2010 namespace: in the part's prefix for it, else declaring its own.
    pub fn w14(&self, local: &str, attrs: &[(&str, &str)], inner: &str) -> String {
        self.ext(("w14", "http://schemas.microsoft.com/office/word/2010/wordml"), local, attrs, inner)
    }

    /// An element of the Word 2012 namespace, as [`Cx::w14`].
    pub fn w15(&self, local: &str, attrs: &[(&str, &str)]) -> String {
        self.ext(("w15", "http://schemas.microsoft.com/office/word/2012/wordml"), local, attrs, "")
    }

    /// An element of an extension namespace (its usual prefix and URI): in the part's prefix for it, else declaring
    /// its own; `{w14}` in `inner` takes the prefix.
    fn ext(&self, (usual, uri): (&str, &str), local: &str, attrs: &[(&str, &str)], inner: &str) -> String {
        let (p, decl) = match self.prefix_of(uri) {
            Some(p) => (p, String::new()),
            None => (format!("{usual}:"), format!(" xmlns:{usual}=\"{uri}\"")),
        };
        let a: String = attrs.iter().map(|(k, v)| format!(" {p}{k}=\"{}\"", esc(v))).collect();
        if inner.is_empty() { format!("<{p}{local}{decl}{a}/>") } else { format!("<{p}{local}{decl}{a}>{}</{p}{local}>", inner.replace("{w14}", &p)) }
    }

    /// Parses an element as written in the main part (its prefixes declared at the root): `f` gets the parsed text and
    /// the element.
    pub fn parse<T>(&self, raw: &str, f: impl FnOnce(&str, roxmltree::Node) -> T) -> Option<T> {
        let text = format!("<x {}>{raw}</x>", self.ns);
        let doc = roxmltree::Document::parse(&text).ok()?;
        let el = doc.root_element().first_element_child()?;
        Some(f(&text, el))
    }
}

/// A child's key in [`Props`]: its local name in WordprocessingML, `w14:` and its name in Word 2010's namespace, else its
/// expanded name (so `w14:shadow` is not `w:shadow`).
fn key(c: roxmltree::Node) -> String {
    match c.tag_name().namespace() {
        Some(W_NS) | None => c.tag_name().name().to_string(),
        Some("http://schemas.microsoft.com/office/word/2010/wordml") => format!("w14:{}", c.tag_name().name()),
        Some("http://schemas.microsoft.com/office/word/2012/wordml") => format!("w15:{}", c.tag_name().name()),
        Some(ns) => format!("{{{ns}}}{}", c.tag_name().name()),
    }
}

/// A properties element (`w:rPr`, `w:pPr`) as its children's XML, to set and remove properties in schema order.
pub struct Props {
    open: String,
    close: String,
    order: &'static [&'static str],
    pub kids: Vec<(String, String)>,
}

impl Props {
    /// `raw` as written (or an empty `local` element).
    pub fn parse(raw: Option<&str>, local: &str, cx: &Cx) -> Props {
        let w = &cx.w;
        let order = match local {
            "pPr" => PPR,
            "style" => STYLE,
            "numPr" => &["ilvl", "numId", "numberingChange", "ins"],
            _ => RPR,
        };
        let mut p = Props { open: format!("<{w}{local}>"), close: format!("</{w}{local}>"), order, kids: Vec::new() };
        let parsed = raw.and_then(|r| {
            cx.parse(r, |text, el| {
                let open = el.first_child().map(|c| text[el.range().start..c.range().start].to_string());
                (open, el.children().filter(|c| c.is_element()).map(|c| (key(c), text[c.range()].to_string())).collect())
            })
        });
        if let Some((open, kids)) = parsed {
            p.open = open.unwrap_or(p.open);
            p.kids = kids;
        }
        p
    }

    /// Uses another schema order for the children (table, row and cell properties).
    pub fn reorder(&mut self, order: &'static [&'static str]) {
        self.order = order;
    }

    fn rank(&self, name: &str) -> usize {
        self.order.iter().position(|n| *n == name).unwrap_or(self.order.len() - 2)
    }

    pub fn get(&self, local: &str) -> Option<&str> {
        self.kids.iter().find(|(n, _)| n == local).map(|(_, x)| x.as_str())
    }

    pub fn set(&mut self, local: &str, xml: String) {
        if let Some(k) = self.kids.iter_mut().find(|(n, _)| n == local) {
            k.1 = xml;
            return;
        }
        let r = self.rank(local);
        let at = self.kids.iter().position(|(n, _)| self.rank(n) > r).unwrap_or(self.kids.len());
        self.kids.insert(at, (local.to_string(), xml));
    }

    /// Adds a child in schema order after any of the same name (a section's several `w:headerReference`).
    pub fn add(&mut self, local: &str, xml: String) {
        let r = self.rank(local);
        let at = self.kids.iter().position(|(n, _)| self.rank(n) > r).unwrap_or(self.kids.len());
        self.kids.insert(at, (local.to_string(), xml));
    }

    pub fn remove(&mut self, local: &str) {
        self.kids.retain(|(n, _)| n != local);
    }

    /// Sets attributes of the child `local` (creating it), keeping its others; `None` removes one.
    pub fn attrs(&mut self, local: &str, set: &[(&str, Option<String>)], cx: &Cx) {
        let w = &cx.w;
        let mut attrs: Vec<(String, String)> = self
            .get(local)
            .and_then(|x| cx.parse(x, |text, el| attr_list(&text[el.range().start..el.first_child().map_or(el.range().end, |c| c.range().start)])))
            .unwrap_or_default();
        for (name, value) in set {
            let at = attrs.iter().position(|(q, _)| q.rsplit(':').next() == Some(name));
            match (at, value) {
                (Some(i), Some(v)) => attrs[i].1 = esc(v),
                (Some(i), None) => {
                    attrs.remove(i);
                }
                (None, Some(v)) => attrs.push((format!("{w}{name}"), esc(v))),
                (None, None) => {}
            }
        }
        let a: String = attrs.iter().map(|(q, v)| format!(" {q}=\"{v}\"")).collect();
        self.set(local, format!("<{w}{local}{a}/>"));
    }

    /// The element, or nothing when it has no properties left.
    pub fn xml(&self) -> Option<String> {
        (!self.kids.is_empty()).then(|| format!("{}{}{}", self.open, self.kids.iter().map(|(_, x)| x.as_str()).collect::<String>(), self.close))
    }
}

/// WordprocessingML attribute `name`'s value on the element `xml` (a properties child as written, its own prefix
/// WordprocessingML's), unescaped. `None` too for an element with other prefixes its part's root declares: a
/// `w:shd`'s attributes are WordprocessingML's.
pub fn attr_of(xml: &str, name: &str) -> Option<String> {
    let tag = xml.trim_start().strip_prefix('<')?;
    let qname = tag.split(|c: char| c.is_whitespace() || c == '/' || c == '>').next()?;
    let decl = match qname.split_once(':') {
        Some((p, _)) => format!(r#"xmlns:{p}="{W_NS}""#),
        None => format!(r#"xmlns="{W_NS}""#),
    };
    let text = format!("<x {decl}>{xml}</x>");
    let doc = roxmltree::Document::parse(&text).ok()?;
    crate::docx::xml::attr(doc.root_element().first_element_child()?, name).map(str::to_string)
}

/// `name="value"` pairs of a start tag as written (values still escaped).
fn attr_list(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag.trim_start_matches('<');
    rest = rest.trim_start_matches(|c: char| !c.is_whitespace());
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next() else { break };
        let Some(end) = after[1..].find(q) else { break };
        out.push((name, after[1..end + 1].to_string()));
        rest = &after[end + 2..];
    }
    out
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_attribute_is_read_with_its_namespace_and_quotes() {
        assert_eq!(super::attr_of(r#"<w:shd w:color="a>b" w:val="pct20" w:fill="FF0000"/>"#, "val").as_deref(), Some("pct20"));
        assert_eq!(super::attr_of(r#"<w:shd w:val='say "x"' w:fill="FF0000"/>"#, "val").as_deref(), Some(r#"say "x""#));
        assert_eq!(super::attr_of(r#"<q:shd xmlns:x="urn:x" x:val="no" q:val="diagStripe"/>"#, "val").as_deref(), Some("diagStripe"));
        assert_eq!(super::attr_of(r#"<w:shd w:val="a&amp;b"/>"#, "val").as_deref(), Some("a&b"));
        assert_eq!(super::attr_of(r#"<w:shd w:fill="FF0000"/>"#, "val"), None);
    }
}
