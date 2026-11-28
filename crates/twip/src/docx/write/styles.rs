//! The styles part

use crate::docx::style_xml::StylesPart;
use std::collections::HashMap;

use super::super::Session;
use super::super::model::Document;
use super::super::package;
use super::super::xml::{attr, is_w};
use super::Splice;
use crate::docx::style_xml::StyleXml;
use crate::format::DefinedIn;

const STYLES_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";

/// The styles part after style edits: each `w:style` written as the styles table holds it, unchanged ones kept as they
/// were, deleted ones spliced out and new ones added at the end; `w:latentStyles` and `w:docDefaults` the same.
pub(super) fn styles(doc: &Document, session: &Session, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = session.source.as_ref().ok_or("the document was not read for editing")?;
    let Some(res) = doc.definitions.as_ref() else { return Ok(()) };
    let typed = res.typed();
    let part = &typed.styles;
    let (colours, cx) = (typed.cascade.colours, part.cx());
    // Each of the document's own styles as its holder writes it
    let written: HashMap<&str, String> = part
        .sheet
        .styles
        .values()
        .filter(|e| e.origin == DefinedIn::Document)
        .filter_map(|e| Some((&*e.style.id, part.styles.get(&e.style.id)?.xml(&colours, &cx).into_owned())))
        .collect();
    let defaults_raw = part.defaults.as_ref().map(|d| d.xml(&colours, &cx).into_owned());
    let latent_raw = part.latent.as_ref().map(|d| d.xml(&colours, &cx).into_owned());
    let name = package::related(&src.package, parts, &src.main, STYLES_REL)?;
    let Some(xml) = (match &name {
        Some(n) => package::read(&src.package, parts, n)?,
        None => None,
    }) else {
        if written.is_empty() && defaults_raw.is_none() && latent_raw.is_none() {
            return Ok(());
        }
        // A document without a styles part: one, with every style and the defaults and latent styles set.
        let mut sorted: Vec<(&&str, &String)> = written.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let root = if part.root().is_empty() { format!("<w:styles xmlns:w=\"{}\">", super::props::W_NS) } else { part.root().to_string() };
        let close = format!("</{}styles>", cx.w);
        let body: String = defaults_raw.iter().chain(latent_raw.iter()).cloned().chain(sorted.iter().map(|s| s.1.clone())).collect();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{root}{body}{close}");
        let part = match src.main.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/styles.xml"),
            None => "styles.xml".to_string(),
        };
        let part = package::free_name(&src.package, parts, &part)?;
        package::add_part(&src.package, parts, &src.main, &part, STYLES_TYPE, STYLES_REL)?;
        package::set(parts, &part, xml.into_bytes());
        return Ok(());
    };
    let open = expanded(&xml, roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?.root_element());
    let xml = open.unwrap_or(xml);
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let mut splices: Vec<Splice> = Vec::new();
    // A repeated id: the table holds the last one read, so that one is written and the earlier ones are kept as they
    // are (a deleted style's go with it).
    let mut last: HashMap<&str, usize> = HashMap::new();
    for n in root.children().filter(|n| is_w(*n, "style")) {
        if let Some(id) = attr(n, "styleId") {
            last.insert(id, n.range().start);
        }
    }
    // Where defaults or latent styles the part lacks go: after the root's start tag, latent styles after the defaults.
    let open = root.range().start + crate::docx::binding::tag_end(&xml[root.range().start..]).ok_or("a styles part without its start tag")? + 1;
    let defaults = root.children().find(|n| is_w(*n, "docDefaults"));
    if defaults.is_none()
        && let Some(d) = &defaults_raw
    {
        splices.push((open..open, d.clone()));
    }
    if !root.children().any(|n| is_w(n, "latentStyles"))
        && let Some(l) = &latent_raw
    {
        let at = defaults.map_or(open, |d| d.range().end);
        splices.push((at..at, l.clone()));
    }
    for n in root.children().filter(|n| n.is_element()) {
        let was = &xml[n.range()];
        if is_w(n, "style") {
            let Some(id) = attr(n, "styleId") else { continue };
            match written.get(id) {
                Some(_) if last.get(id) != Some(&n.range().start) => {}
                Some(raw) if raw == was => {}
                Some(raw) => splices.push((n.range(), raw.clone())),
                None => splices.push((n.range(), String::new())),
            }
        } else if is_w(n, "latentStyles")
            && let Some(now) = latent_raw.as_deref().filter(|now| *now != was)
        {
            splices.push((n.range(), now.to_string()));
        } else if is_w(n, "docDefaults")
            && let Some(now) = defaults_raw.as_deref().filter(|now| *now != was)
        {
            splices.push((n.range(), now.to_string()));
        }
    }
    let mut added: Vec<(&&str, &String)> = written.iter().filter(|(id, _)| !last.contains_key(**id)).collect();
    added.sort_by(|a, b| a.0.cmp(b.0));
    if !added.is_empty() {
        let at = xml.rfind("</").ok_or("a styles part without its end tag")?;
        splices.push((at..at, added.iter().map(|s| s.1.as_str()).collect()));
    }
    if splices.is_empty() {
        return Ok(());
    }
    // Stable: inserted defaults stay before inserted latent styles.
    splices.sort_by_key(|s| s.0.start);
    let mut out = String::with_capacity(xml.len());
    let mut at = 0;
    for (range, with) in &splices {
        out.push_str(&xml[at..range.start]);
        out.push_str(with);
        at = range.end;
    }
    out.push_str(&xml[at..]);
    let name = name.ok_or("a styles part without its name")?;
    package::set(parts, &name, out.into_bytes());
    Ok(())
}

/// `xml` with its root `<w:styles .../>` written as `<w:styles ...></w:styles>`, when it is the empty element form.
fn expanded(xml: &str, root: roxmltree::Node) -> Option<String> {
    let r = root.range();
    let tag = xml.get(r.clone())?;
    let name = tag.strip_prefix('<')?.split(|c: char| c.is_whitespace() || matches!(c, '/' | '>')).next()?;
    let head = tag.strip_suffix("/>")?;
    Some(format!("{}{head}></{name}>{}", &xml[..r.start], &xml[r.end..]))
}
