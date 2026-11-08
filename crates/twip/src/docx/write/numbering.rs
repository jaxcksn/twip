//! The numbering part

use std::collections::{BTreeMap, HashSet};

use super::super::model::Document;
use super::super::package;
use super::super::styles::StyleOrigin;
use super::super::xml::{attr, int, is_w};
use super::Splice;

const NUMBERING_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";

/// The numbering part after list edits: each `w:numPicBullet`, `w:abstractNum` and `w:num` written as the numbering
/// table holds it, unchanged ones kept as they were, removed ones spliced out, and new ones added where CT_Numbering
/// has them (picture bullets first, then the definitions, then the `w:num`s).
pub(super) fn numbering(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let Some(n) = doc.resolver.as_ref().map(|r| &r.numbering) else { return Ok(()) };
    let abstracts: BTreeMap<i64, &str> =
        n.abstracts.iter().filter(|(_, a)| a.origin == StyleOrigin::Document).map(|(k, a)| (*k, a.raw.as_str())).collect();
    let nums: BTreeMap<i64, &str> = n.nums.iter().filter(|(_, m)| m.origin == StyleOrigin::Document).map(|(k, m)| (*k, m.raw.as_str())).collect();
    let pics: BTreeMap<i64, &str> = n.pic_bullets.iter().map(|(k, p)| (*k, p.raw.as_str())).collect();
    let name = package::related(&src.package, parts, &src.main, NUMBERING_REL)?;
    let Some(xml) = (match &name {
        Some(p) => package::read(&src.package, parts, p)?,
        None => None,
    }) else {
        if abstracts.is_empty() && nums.is_empty() {
            return Ok(());
        }
        // A document without a numbering part: one, with every list.
        let root = if n.root.is_empty() { format!("<w:numbering xmlns:w=\"{}\">", super::props::W_NS) } else { n.root.clone() };
        let close = format!("</{}numbering>", n.cx().w);
        let body: String = pics.values().chain(abstracts.values()).chain(nums.values()).copied().collect();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{root}{body}{close}");
        let part = match src.main.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/numbering.xml"),
            None => "numbering.xml".to_string(),
        };
        package::add_part(&src.package, parts, &src.main, &part, NUMBERING_TYPE, NUMBERING_REL)?;
        package::set(parts, &part, xml.into_bytes());
        return Ok(());
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let mut splices: Vec<Splice> = Vec::new();
    let (mut seen_pics, mut seen_abstracts, mut seen_nums) = (HashSet::new(), HashSet::new(), HashSet::new());
    let (mut first_abstract, mut first_num) = (None, None);
    let mut each = |id: Option<i64>, now: &BTreeMap<i64, &str>, seen: &mut HashSet<i64>, range: std::ops::Range<usize>| {
        let Some(id) = id else { return };
        // A repeated id: the table holds the last one read, so the earlier ones are kept as they are.
        if !seen.insert(id) {
            return;
        }
        let was = &xml[range.clone()];
        match now.get(&id) {
            Some(raw) if *raw == was => {}
            Some(raw) => splices.push((range, raw.to_string())),
            None => splices.push((range, String::new())),
        }
    };
    for c in root.children().filter(|c| c.is_element()) {
        if is_w(c, "numPicBullet") {
            each(int(attr(c, "numPicBulletId")), &pics, &mut seen_pics, c.range());
        } else if is_w(c, "abstractNum") {
            first_abstract.get_or_insert(c.range().start);
            each(int(attr(c, "abstractNumId")), &abstracts, &mut seen_abstracts, c.range());
        } else if is_w(c, "num") {
            first_num.get_or_insert(c.range().start);
            each(int(attr(c, "numId")), &nums, &mut seen_nums, c.range());
        }
    }
    let close = xml.rfind("</").ok_or("a numbering part without its end tag")?;
    let new = |now: &BTreeMap<i64, &str>, seen: &HashSet<i64>| -> String { now.iter().filter(|(k, _)| !seen.contains(k)).map(|(_, r)| *r).collect() };
    let (new_pics, new_abstracts, new_nums) = (new(&pics, &seen_pics), new(&abstracts, &seen_abstracts), new(&nums, &seen_nums));
    let abstracts_at = first_abstract.or(first_num).unwrap_or(close);
    if !new_pics.is_empty() {
        splices.push((abstracts_at..abstracts_at, new_pics));
    }
    if !new_abstracts.is_empty() {
        let at = first_num.unwrap_or(close);
        splices.push((at..at, new_abstracts));
    }
    if !new_nums.is_empty() {
        splices.push((close..close, new_nums));
    }
    if splices.is_empty() {
        return Ok(());
    }
    // What goes in at a place goes before what is replaced from it; a stable sort keeps picture bullets first.
    splices.sort_by_key(|s| (s.0.start, s.0.end));
    let mut out = String::with_capacity(xml.len());
    let mut at = 0;
    for (range, with) in &splices {
        out.push_str(&xml[at..range.start]);
        out.push_str(with);
        at = range.end;
    }
    out.push_str(&xml[at..]);
    let name = name.ok_or("a numbering part without its name")?;
    package::set(parts, &name, out.into_bytes());
    Ok(())
}
