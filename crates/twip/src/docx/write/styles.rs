//! The styles part

use std::collections::HashMap;

use super::super::model::Document;
use super::super::package;
use super::super::styles::StyleOrigin;
use super::super::xml::{attr, is_w};
use super::Splice;

const STYLES_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";

/// The styles part after style edits: each `w:style` written as the styles table holds it, unchanged ones kept as they
/// were, deleted ones spliced out and new ones added at the end; `w:latentStyles` and `w:docDefaults` the same.
pub(super) fn styles(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let Some(styles) = doc.resolver.as_ref().map(|r| &r.styles) else { return Ok(()) };
    let ours: Vec<_> = styles.by_id.values().filter(|s| s.origin == StyleOrigin::Document).collect();
    let name = package::related(&src.package, parts, &src.main, STYLES_REL)?;
    let Some(xml) = (match &name {
        Some(n) => package::read(&src.package, parts, n)?,
        None => None,
    }) else {
        if ours.is_empty() {
            return Ok(());
        }
        // A document without a styles part: one, with every style.
        let mut sorted = ours;
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        let root = if styles.root.is_empty() { format!("<w:styles xmlns:w=\"{}\">", super::props::W_NS) } else { styles.root.clone() };
        let close = format!("</{}styles>", styles.cx().w);
        let body: String = styles.defaults_raw.iter().chain(styles.latent.raw.iter()).cloned().chain(sorted.iter().map(|s| s.raw.clone())).collect();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{root}{body}{close}");
        let part = match src.main.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/styles.xml"),
            None => "styles.xml".to_string(),
        };
        package::add_part(&src.package, parts, &src.main, &part, STYLES_TYPE, STYLES_REL)?;
        package::set(parts, &part, xml.into_bytes());
        return Ok(());
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let mut splices: Vec<Splice> = Vec::new();
    let mut seen: HashMap<&str, ()> = HashMap::new();
    for n in root.children().filter(|n| n.is_element()) {
        let was = &xml[n.range()];
        if is_w(n, "style") {
            let Some(id) = attr(n, "styleId") else { continue };
            // A repeated id: the table holds the last one read, so the earlier ones are kept as they are.
            if seen.insert(id, ()).is_some() {
                continue;
            }
            match styles.by_id.get(id).filter(|s| s.origin == StyleOrigin::Document) {
                Some(s) if s.raw == was => {}
                Some(s) => splices.push((n.range(), s.raw.clone())),
                None => splices.push((n.range(), String::new())),
            }
        } else if is_w(n, "latentStyles")
            && let Some(now) = styles.latent.raw.as_deref().filter(|now| *now != was)
        {
            splices.push((n.range(), now.to_string()));
        } else if is_w(n, "docDefaults")
            && let Some(now) = styles.defaults_raw.as_deref().filter(|now| *now != was)
        {
            splices.push((n.range(), now.to_string()));
        }
    }
    let mut added: Vec<_> = ours.iter().filter(|s| !seen.contains_key(s.id.as_str())).collect();
    added.sort_by(|a, b| a.id.cmp(&b.id));
    if !added.is_empty() {
        let at = xml.rfind("</").ok_or("a styles part without its end tag")?;
        splices.push((at..at, added.iter().map(|s| s.raw.as_str()).collect()));
    }
    if splices.is_empty() {
        return Ok(());
    }
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
