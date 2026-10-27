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
    "rPrChange",
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

    /// Parses an element as written in the main part (its prefixes declared at the root): `f` gets the parsed text and
    /// the element.
    pub fn parse<T>(&self, raw: &str, f: impl FnOnce(&str, roxmltree::Node) -> T) -> Option<T> {
        let text = format!("<x {}>{raw}</x>", self.ns);
        let doc = roxmltree::Document::parse(&text).ok()?;
        let el = doc.root_element().first_element_child()?;
        Some(f(&text, el))
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
            "numPr" => &["ilvl", "numId", "numberingChange", "ins"],
            _ => RPR,
        };
        let mut p = Props { open: format!("<{w}{local}>"), close: format!("</{w}{local}>"), order, kids: Vec::new() };
        let parsed = raw.and_then(|r| {
            cx.parse(r, |text, el| {
                let open = el.first_child().map(|c| text[el.range().start..c.range().start].to_string());
                (open, el.children().filter(|c| c.is_element()).map(|c| (c.tag_name().name().to_string(), text[c.range()].to_string())).collect())
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
