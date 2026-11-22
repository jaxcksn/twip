//! Content control data binding (`w:dataBinding`): a bounded XPath subset over custom XML data parts

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ops::Range;

use roxmltree::Node;

use super::model::{BreakKind, Control, ControlId, ControlKind, ControlProps, DataPart, Inline, Paragraph, Run};
use super::write::props::esc;

/// Steps an XPath may have; longer ones aren't evaluated.
const MAX_STEPS: usize = 64;

/// One location step: an element by namespace and name (`*`: any), the `n`th such child (from 1).
#[derive(Clone, Debug, PartialEq)]
struct Step {
    ns: Option<String>,
    name: String,
    index: usize,
}

/// A binding's XPath, resolved against its prefix mappings: absolute element steps, then an attribute or `text()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    steps: Vec<Step>,
    attr: Option<(Option<String>, String)>,
}

/// A control's binding: its XPath and the data part it names (`w:storeItemID`, or none: the first part it finds a
/// node in).
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub path: Path,
    pub store: Option<String>,
}

/// Whether two data part item ids are the same GUID: compared without case or braces.
pub fn same_item(a: &str, b: &str) -> bool {
    a.trim().trim_matches(['{', '}']).eq_ignore_ascii_case(b.trim().trim_matches(['{', '}']))
}

/// An item id as a GUID is written (`{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, braces optional): braced, upper case.
pub fn guid(id: &str) -> Option<String> {
    let bare = id.trim();
    let bare = bare.strip_prefix('{').and_then(|b| b.strip_suffix('}')).unwrap_or(bare);
    let groups: Vec<&str> = bare.split('-').collect();
    let shape = groups.len() == 5 && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.bytes().all(|c| c.is_ascii_hexdigit()));
    shape.then(|| format!("{{{}}}", bare.to_ascii_uppercase()))
}

/// An attribute value's text: the predefined and numeric character references resolved.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        rest = &rest[k..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n => n
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| n.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr_of<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!(":{name}=")).or_else(|| tag.find(&format!(" {name}=")))? + name.len() + 2;
    let quote = tag[at..].chars().next()?;
    let rest = &tag[at + quote.len_utf8()..];
    Some(&rest[..rest.find(quote)?])
}

/// `xmlns:p='uri'` pairs.
fn prefixes(mappings: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = mappings;
    while let Some(k) = rest.find("xmlns:") {
        rest = &rest[k + 6..];
        let Some(eq) = rest.find('=') else { break };
        let prefix = rest[..eq].trim().to_string();
        let Some(quote) = rest[eq + 1..].trim_start().chars().next().filter(|q| *q == '\'' || *q == '"') else { break };
        let body = rest[eq + 1..].trim_start();
        let Some(end) = body[1..].find(quote) else { break };
        out.push((prefix, unescape(&body[1..1 + end])));
        rest = &body[end + 2..];
    }
    out
}

impl Binding {
    /// From `w:dataBinding` as read; `None` for an XPath outside the subset.
    pub fn parse(xml: &str) -> Option<Binding> {
        let xpath = unescape(attr_of(xml, "xpath")?);
        let maps = attr_of(xml, "prefixMappings").map(|m| prefixes(&unescape(m))).unwrap_or_default();
        let store = attr_of(xml, "storeItemID").map(|s| s.to_string());
        Some(Binding { path: Path::parse(&xpath, &maps)?, store })
    }

    /// The control's binding (`None`: none, or one this doesn't evaluate).
    pub fn of(props: &ControlProps) -> Option<Binding> {
        Binding::parse(props.binding.as_deref()?)
    }

    /// The index of the data part it reads: the one its store item id names (compared without case), else the first
    /// with its node.
    pub fn part(&self, parts: &Parts) -> Option<usize> {
        match &self.store {
            Some(id) => parts.parts.iter().position(|p| same_item(&p.item_id, id)),
            None => (0..parts.parts.len()).find(|&k| parts.doc(k).is_some_and(|d| self.path.element(d).is_some())),
        }
    }

    /// The bound node's text, from the data parts.
    pub fn value(&self, parts: &Parts) -> Option<String> {
        read_in(parts.doc(self.part(parts)?)?, &self.path)
    }
}

/// The data parts, each parsed once when a binding first reads it: many controls bind into the same part.
pub struct Parts<'a> {
    parts: &'a [DataPart],
    docs: Vec<OnceCell<Option<roxmltree::Document<'a>>>>,
}

impl<'a> Parts<'a> {
    pub fn new(parts: &'a [DataPart]) -> Parts<'a> {
        Parts { parts, docs: parts.iter().map(|_| OnceCell::new()).collect() }
    }

    fn doc(&self, k: usize) -> Option<&roxmltree::Document<'a>> {
        let xml: &'a str = &self.parts.get(k)?.xml;
        self.docs[k].get_or_init(|| roxmltree::Document::parse(xml).ok()).as_ref()
    }
}

impl Path {
    /// `/p:a[1]/p:b[2]/@p:c`, `/a/b/text()`: element steps by name (`*`: any) and position; `None` for anything else.
    pub fn parse(xpath: &str, maps: &[(String, String)]) -> Option<Path> {
        let ns = |qname: &str| -> Option<(Option<String>, String)> {
            match qname.split_once(':') {
                Some((p, n)) => Some((Some(maps.iter().find(|(k, _)| k == p)?.1.clone()), n.to_string())),
                None => Some((None, qname.to_string())),
            }
        };
        let body = xpath.trim().strip_prefix('/')?;
        let parts: Vec<&str> = body.split('/').collect();
        if parts.len() > MAX_STEPS || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        let mut steps = Vec::new();
        let mut attr = None;
        for (k, part) in parts.iter().enumerate() {
            let last = k + 1 == parts.len();
            if last && *part == "text()" {
                break;
            }
            if let Some(a) = part.strip_prefix('@') {
                if !last {
                    return None;
                }
                attr = Some(ns(a)?);
                break;
            }
            let (name, index) = match part.split_once('[') {
                Some((n, rest)) => (n, rest.strip_suffix(']')?.trim().parse::<usize>().ok().filter(|i| *i > 0)?),
                None => (*part, 1),
            };
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '*')) {
                return None;
            }
            let (ns, name) = if name == "*" { (None, "*".to_string()) } else { ns(name)? };
            steps.push(Step { ns, name, index });
        }
        (!steps.is_empty()).then_some(Path { steps, attr })
    }

    fn element<'a, 'i>(&self, doc: &'a roxmltree::Document<'i>) -> Option<Node<'a, 'i>> {
        let matches =
            |n: &Node, s: &Step| n.is_element() && (s.name == "*" || (n.tag_name().name() == s.name && n.tag_name().namespace() == s.ns.as_deref()));
        let root = doc.root_element();
        let first = &self.steps[0];
        if first.index != 1 || !matches(&root, first) {
            return None;
        }
        let mut at = root;
        for s in &self.steps[1..] {
            at = at.children().filter(|c| matches(c, s)).nth(s.index - 1)?;
        }
        Some(at)
    }
}

/// The bound node's text in a data part's XML: an attribute's value, or an element's text.
pub fn read(xml: &str, path: &Path) -> Option<String> {
    read_in(&roxmltree::Document::parse(xml).ok()?, path)
}

fn read_in(doc: &roxmltree::Document, path: &Path) -> Option<String> {
    let el = path.element(doc)?;
    match &path.attr {
        Some((ns, name)) => el.attributes().find(|a| a.name() == name && a.namespace() == ns.as_deref()).map(|a| a.value().to_string()),
        None => Some(el.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()),
    }
}

/// The data part's XML with the bound node set to `value`: an attribute's value, or the text of an element without
/// element children. `None` when the node isn't there or holds elements.
pub fn write(xml: &str, path: &Path, value: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let el = path.element(&doc)?;
    let mut out = xml.to_string();
    match &path.attr {
        Some((ns, name)) => {
            let a = el.attributes().find(|a| a.name() == name && a.namespace() == ns.as_deref())?;
            let r = a.range_value();
            // Either quote can delimit it.
            out.replace_range(r, &esc(value).replace('\'', "&apos;"));
        }
        None => {
            if el.children().any(|c| c.is_element()) {
                return None;
            }
            let r = el.range();
            let tag = &xml[r.clone()];
            let qname = &tag[1..tag.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?];
            // Start tag's end (`tag_end`)
            let open = match el.first_child() {
                Some(c) => xml[r.start..c.range().start].to_string(),
                None => match tag.strip_suffix("/>") {
                    Some(head) => format!("{head}>"),
                    None => tag[..tag.rfind("</")?].to_string(),
                },
            };
            out.replace_range(r, &format!("{open}{}</{qname}>", esc(value)));
        }
    }
    Some(out)
}

/// A drop-down's or combo box's items (`w:listItem`): their text and value.
fn items(props: &ControlProps) -> Vec<(String, String)> {
    let Some(x) = props.kind_xml.as_deref() else { return Vec::new() };
    x.split("listItem")
        .skip(1)
        .filter_map(|tag| {
            let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
            let value = attr_of(tag, "value").map(unescape);
            let text = attr_of(tag, "displayText").map(unescape);
            Some((text.clone().or_else(|| value.clone())?, value.or(text)?))
        })
        .collect()
}

/// What a control bound to `value` shows: a drop-down's item for it, a date in the control's format.
pub fn shown(props: &ControlProps, value: &str) -> String {
    match &props.kind {
        ControlKind::DropDown => items(props).into_iter().find(|(_, v)| v == value).map_or_else(|| value.to_string(), |(t, _)| t),
        ControlKind::Date { format, .. } => match super::review::Timestamp::parse(value.trim()).utc {
            Some(secs) => super::fields::date_text(if format.is_empty() { "M/d/yyyy" } else { format }, secs),
            None => value.to_string(),
        },
        _ => value.to_string(),
    }
}

/// What a control showing `text` stores: a drop-down's item's value, a date's date (as `w:storeMappedDataAs` says:
/// a date and time, a date, or the text).
pub fn stored(props: &ControlProps, text: &str) -> String {
    match &props.kind {
        ControlKind::DropDown => items(props).into_iter().find(|(t, _)| t == text).map_or_else(|| text.to_string(), |(_, v)| v),
        ControlKind::Date { .. } => {
            let mapped = props.kind_xml.as_deref().and_then(|x| attr_of(&x[x.find("storeMappedDataAs")?..], "val")).unwrap_or("dateTime");
            match (mapped, full_date(props, text)) {
                ("text", _) | (_, None) => text.to_string(),
                ("date", Some(full)) => full.get(..10).unwrap_or(&full).to_string(),
                (_, Some(full)) => full,
            }
        }
        _ => text.to_string(),
    }
}

/// A control's text as inlines: its lines (`\n` or `\r\n` apart) with line breaks between them.
pub fn text_inlines(text: &str, run: &Run) -> Vec<Inline> {
    let mut out = Vec::new();
    for (k, line) in text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).enumerate() {
        if k > 0 {
            out.push(Inline::Break(BreakKind::Line, run.clone()));
        }
        if !line.is_empty() {
            out.push(Inline::Text(line.to_string(), run.clone()));
        }
    }
    out
}

/// The full date (`w:fullDate`, `YYYY-MM-DDTHH:MM:SSZ`) of a date control showing `text`: its own when it shows it,
/// else the date `text` is in the control's format; `None` for none.
pub fn full_date(props: &ControlProps, text: &str) -> Option<String> {
    let ControlKind::Date { format, .. } = &props.kind else { return None };
    if text.is_empty() {
        return None;
    }
    let own = props.kind_xml.as_deref().and_then(|x| attr_of(x, "fullDate")).map(unescape);
    if let Some(own) = own.filter(|f| shown(props, f) == text) {
        return Some(own);
    }
    let secs = super::fields::parse_date_text(if format.is_empty() { "M/d/yyyy" } else { format }, text)?;
    Some(super::review::Timestamp::from_utc(secs).raw.to_string())
}

/// A date control's `w:date` with the full date `text` shows ([`full_date`]), or without one; `w` its prefix (for
/// one not read). `None` when it already is so.
pub fn with_full_date(props: &ControlProps, text: &str, w: &str) -> Option<String> {
    let ControlKind::Date { format, lid } = &props.kind else { return None };
    let want = full_date(props, text);
    let xml = props.kind_xml.clone().unwrap_or_else(|| {
        format!(
            "<{w}date><{w}dateFormat {w}val=\"{}\"/>{}<{w}storeMappedDataAs {w}val=\"dateTime\"/><{w}calendar {w}val=\"gregorian\"/></{w}date>",
            esc(format),
            lid.as_ref().map_or(String::new(), |l| format!("<{w}lid {w}val=\"{}\"/>", esc(l)))
        )
    });
    let have = attr_of(&xml, "fullDate").map(unescape);
    if have == want {
        return None;
    }
    let end = tag_end(&xml)?;
    let mut head = xml[..end].to_string();
    if let Some(r) = attr_range(&head, "fullDate") {
        head.replace_range(head[..r.start].trim_end().len()..r.end, "");
    }
    let name_end = head.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(head.len());
    let prefix = head.get(1..name_end).and_then(|n| n.split_once(':')).map_or(String::new(), |(p, _)| format!("{p}:"));
    if let Some(f) = want {
        head.insert_str(name_end, &format!(" {prefix}fullDate=\"{}\"", esc(&f)));
    }
    Some(head + &xml[end..])
}

/// Where element `x`'s start tag ends (its `>`): quoted attribute values may hold one.
pub fn tag_end(x: &str) -> Option<usize> {
    let mut quote = None;
    for (k, c) in x.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(k),
            _ => {}
        }
    }
    None
}

/// Where attribute `local` (any prefix, either quote) is in start tag `head` (up to its `>`).
fn attr_range(head: &str, local: &str) -> Option<Range<usize>> {
    let b = head.as_bytes();
    let skip = |k: &mut usize| {
        while b.get(*k).is_some_and(u8::is_ascii_whitespace) {
            *k += 1;
        }
    };
    let mut k = head.find(|c: char| c.is_whitespace() || c == '/')?;
    loop {
        skip(&mut k);
        let start = k;
        while b.get(k).is_some_and(|c| !c.is_ascii_whitespace() && *c != b'=' && *c != b'/') {
            k += 1;
        }
        let name = &head[start..k];
        skip(&mut k);
        if name.is_empty() || b.get(k) != Some(&b'=') {
            return None;
        }
        k += 1;
        skip(&mut k);
        let q = *b.get(k).filter(|q| **q == b'"' || **q == b'\'')?;
        k += 1 + head[k + 1..].find(q as char)? + 1;
        if name.rsplit(':').next() == Some(local) {
            return Some(start..k);
        }
    }
}

/// The run a control's content takes once it stops showing its placeholder (whose run is the placeholder's style):
/// its own (`w:sdtPr/w:rPr`, which `resolve` resolves in its paragraph), else that of the text before its start
/// marker (inline `start`), else its paragraph mark's.
pub fn own_run(props: &ControlProps, inlines: &[Inline], start: usize, mark: &Run, resolve: impl FnOnce(&str) -> Option<Run>) -> Run {
    props
        .rest
        .iter()
        .find(|(n, _)| n == "rPr")
        .and_then(|(_, x)| resolve(x))
        .or_else(|| inlines[..start.min(inlines.len())].iter().rev().find_map(|i| i.run().cloned()))
        .unwrap_or_else(|| mark.clone())
}

/// A bound control inline in a paragraph: the inlines between its markers, its text and its binding.
pub struct Span {
    pub control: ControlId,
    /// Inline indexes of its content (between its start and end markers).
    pub inner: Range<usize>,
    pub text: String,
    pub run: Option<Run>,
    pub binding: Binding,
}

/// The bound inline controls in `inlines` (text, date and drop-down kinds), innermost first.
pub fn spans(inlines: &[Inline], controls: &HashMap<ControlId, Control>) -> Vec<Span> {
    let mut out = Vec::new();
    let mut open: Vec<(ControlId, usize)> = Vec::new();
    for (k, i) in inlines.iter().enumerate() {
        match i {
            Inline::ControlStart(c) => open.push((*c, k)),
            Inline::ControlEnd(c) => {
                let Some(at) = open.iter().rposition(|(o, _)| o == c) else { continue };
                let (c, start) = open.remove(at);
                let Some(ctl) = controls.get(&c) else { continue };
                if !matches!(ctl.props.kind, ControlKind::PlainText { .. } | ControlKind::RichText | ControlKind::Date { .. } | ControlKind::DropDown)
                {
                    continue;
                }
                let Some(binding) = Binding::of(&ctl.props) else { continue };
                let inner = start + 1..k;
                let content = &inlines[inner.clone()];
                let text = if ctl.props.showing_placeholder {
                    String::new()
                } else {
                    content
                        .iter()
                        .filter_map(|i| match i {
                            Inline::Text(t, _) => Some(t.as_str()),
                            Inline::Break(BreakKind::Line, _) => Some("\n"),
                            _ => None,
                        })
                        .collect()
                };
                let run = content.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r.clone()) } else { None });
                out.push(Span { control: c, inner, text, run, binding });
            }
            _ => {}
        }
    }
    // One holding or crossing another isn't shown or written: Word doesn't bind nested controls, and replacing both
    // would leave the outer one's range stale.
    let holds = |a: &Range<usize>, b: &Range<usize>| a.start <= b.start && b.end <= a.end;
    let crosses = |a: &Range<usize>, b: &Range<usize>| a.start < b.start && b.start < a.end && a.end < b.end;
    let ranges: Vec<Range<usize>> = out.iter().map(|s| s.inner.clone()).collect();
    out.retain(|s| !ranges.iter().any(|r| *r != s.inner && (holds(&s.inner, r) || crosses(&s.inner, r) || crosses(r, &s.inner))));
    out
}

/// Shows each bound control in the paragraph with its node's value from the data parts (one with no value, or one
/// already showing it, is left); whether one changed. Read time: Word shows the data a document opens with. `resolve`
/// resolves a control's own `w:rPr` in the paragraph ([`own_run`]).
pub fn refresh(
    p: &mut Paragraph,
    controls: &mut HashMap<ControlId, Control>,
    parts: &Parts,
    resolve: &dyn Fn(&Paragraph, &str) -> Option<Run>,
) -> bool {
    let mut changed = false;
    let mut found = spans(&p.inlines, controls);
    // From the end, so earlier ranges stay put; nested controls aren't bound in Word.
    found.sort_by_key(|s| std::cmp::Reverse(s.inner.start));
    for s in found {
        let Some(value) = s.binding.value(parts).filter(|v| !v.is_empty()) else { continue };
        let Some(ctl) = controls.get_mut(&s.control) else { continue };
        let want = shown(&ctl.props, &value);
        if want == s.text && !ctl.props.showing_placeholder {
            continue;
        }
        let run = match s.run.filter(|_| !ctl.props.showing_placeholder) {
            Some(r) => r,
            None => {
                let para = &*p;
                own_run(&ctl.props, &para.inlines, s.inner.start - 1, &para.mark, |x| resolve(para, x))
            }
        };
        p.inlines.splice(s.inner, text_inlines(&want, &run));
        ctl.props.showing_placeholder = false;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_node_keeps_attributes_holding_a_greater_than() {
        let path = Path::parse("/r/a", &[]).expect("a path");
        for (xml, want) in [
            (r#"<r><a n="x>y">1</a></r>"#, r#"<r><a n="x>y">2</a></r>"#),
            (r#"<r><a n="x>y"/></r>"#, r#"<r><a n="x>y">2</a></r>"#),
            (r#"<r><a n="x>y"></a ></r>"#, r#"<r><a n="x>y">2</a></r>"#),
        ] {
            let out = write(xml, &path, "2").expect("writes");
            assert!(roxmltree::Document::parse(&out).is_ok(), "well formed: {out}");
            assert_eq!(out, want);
            assert_eq!(read(&out, &path).as_deref(), Some("2"));
        }
    }

    #[test]
    fn a_dates_full_date_is_replaced_whatever_its_quotes() {
        let props = |x: &str| ControlProps {
            kind: ControlKind::Date { format: "M/d/yyyy".into(), lid: None },
            kind_xml: Some(x.into()),
            ..ControlProps::default()
        };
        for x in [
            r#"<w:date w:fullDate='2026-01-01T00:00:00Z'><w:dateFormat w:val="M/d/yyyy"/></w:date>"#,
            r#"<w:date w:x="a>b" w:fullDate="2026-01-01T00:00:00Z"><w:dateFormat w:val="M/d/yyyy"/></w:date>"#,
        ] {
            let out = with_full_date(&props(x), "2/3/2027", "w:").expect("a new date");
            let xml = out.replacen("<w:date", "<w:date xmlns:w=\"urn:w\"", 1);
            let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("well formed: {out}: {e}"));
            let dates: Vec<_> = doc.root_element().attributes().filter(|a| a.name() == "fullDate").map(|a| a.value()).collect();
            assert_eq!(dates, ["2027-02-03T00:00:00Z"], "{out}");
        }
    }

    #[test]
    fn item_ids_are_guids_compared_without_case_or_braces() {
        assert!(same_item("{8A2D6C3E-1F4B-4C5D-9E6F-0A1B2C3D4E5F}", "8a2d6c3e-1f4b-4c5d-9e6f-0a1b2c3d4e5f"));
        assert!(!same_item("{8A2D6C3E-1F4B-4C5D-9E6F-0A1B2C3D4E5F}", "{8A2D6C3E-1F4B-4C5D-9E6F-0A1B2C3D4E50}"));
        assert_eq!(guid("8a2d6c3e-1f4b-4c5d-9e6f-0a1b2c3d4e5f").as_deref(), Some("{8A2D6C3E-1F4B-4C5D-9E6F-0A1B2C3D4E5F}"));
        for bad in ["", "orders", "{1234}", "a\"b", "8a2d6c3e-1f4b-4c5d-9e6f-0a1b2c3d4e5", "8a2d6c3e-1f4b-4c5d-9e6f-0a1b2c3d4e5g"] {
            assert_eq!(guid(bad), None, "{bad}");
        }
    }

    #[test]
    fn bindings_read_and_write_their_node() {
        let xml =
            r#"<c:case xmlns:c="urn:case"><c:party role="plaintiff">Acme &amp; Co</c:party><c:party role="defendant"/><c:no>42</c:no></c:case>"#;
        let b = |xpath: &str| {
            Binding::parse(&format!(r#"<w:dataBinding w:prefixMappings="xmlns:ns0='urn:case'" w:xpath="{xpath}" w:storeItemID="{{A}}"/>"#))
                .expect("in the subset")
        };
        assert_eq!(read(xml, &b("/ns0:case[1]/ns0:party[1]").path).as_deref(), Some("Acme & Co"));
        assert_eq!(read(xml, &b("/ns0:case[1]/ns0:party[2]/@role").path).as_deref(), Some("defendant"));
        assert_eq!(read(xml, &b("/ns0:case/ns0:no/text()").path).as_deref(), Some("42"));
        assert_eq!(read(xml, &b("/ns0:case[1]/ns0:party[3]").path), None);
        let w = write(xml, &b("/ns0:case[1]/ns0:party[2]").path, "Smith <Jr>").expect("written");
        assert!(w.contains(r#"<c:party role="defendant">Smith &lt;Jr&gt;</c:party>"#), "{w}");
        let w = write(&w, &b("/ns0:case[1]/ns0:party[1]/@role").path, "claimant").expect("written");
        assert_eq!(read(&w, &b("/ns0:case[1]/ns0:party[1]/@role").path).as_deref(), Some("claimant"));
        assert_eq!(write(xml, &b("/ns0:case[1]").path, "x"), None, "an element holding elements");
        for bad in ["ns0:case", "/ns0:case[last()]", "//ns0:no", "/ns9:case"] {
            assert!(Binding::parse(&format!(r#"<w:dataBinding w:prefixMappings="xmlns:ns0='urn:case'" w:xpath="{bad}"/>"#)).is_none(), "{bad}");
        }
        let deep = format!("/{}", vec!["a"; 65].join("/"));
        assert!(Path::parse(&deep, &[]).is_none());
    }
}
