//! The numbering part

use crate::docx::list_xml::NumberingXml;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::super::Session;
use super::super::model::Document;
use super::super::package;
use super::super::xml::{attr, int, is_w};
use super::Splice;
use crate::docx::list_xml::ListXml;

const NUMBERING_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";

/// The numbering part after list edits: each `w:numPicBullet`, `w:abstractNum` and `w:num` written as the numbering
/// table holds it, unchanged ones kept as they were, removed ones spliced out, and new ones added where CT_Numbering
/// has them (picture bullets first, then the definitions, then the `w:num`s).
pub(super) fn numbering(doc: &Document, session: &Session, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = session.source.as_ref().ok_or("the document was not read for editing")?;
    let Some(res) = doc.definitions.as_ref() else { return Ok(()) };
    let typed = res.typed();
    let n = &typed.numbering;
    let (colours, cx) = (typed.cascade.colours, n.cx());
    // Each of the document's own definitions, lists and picture bullets as its holder writes it
    let abstracts: BTreeMap<i64, String> = n.defs.iter().map(|(k, d)| (k.0, d.xml(&colours, &cx).into_owned())).collect();
    let nums: BTreeMap<i64, String> = n.lists.iter().map(|(k, d)| (k.0, d.xml(&colours, &cx).into_owned())).collect();
    let pics: BTreeMap<i64, String> = n.pictures.iter().map(|(k, d)| (*k, d.xml(&colours, &cx).into_owned())).collect();
    let name = package::related(&src.package, parts, &src.main, NUMBERING_REL)?;
    let Some(xml) = (match &name {
        Some(p) => package::read(&src.package, parts, p)?,
        None => None,
    }) else {
        if abstracts.is_empty() && nums.is_empty() {
            return Ok(());
        }
        // A document without a numbering part: one, with every list.
        let root = if n.root().is_empty() { format!("<w:numbering xmlns:w=\"{}\">", super::props::W_NS) } else { n.root().to_string() };
        let close = format!("</{}numbering>", cx.w);
        let body: String = pics.values().chain(abstracts.values()).chain(nums.values()).map(String::as_str).collect();
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n{root}{body}{close}");
        let part = match src.main.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/numbering.xml"),
            None => "numbering.xml".to_string(),
        };
        let part = package::free_name(&src.package, parts, &part)?;
        package::add_part(&src.package, parts, &src.main, &part, NUMBERING_TYPE, NUMBERING_REL)?;
        package::set(parts, &part, xml.into_bytes());
        return Ok(());
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let mut splices: Vec<Splice> = Vec::new();
    let tables = [&pics, &abstracts, &nums];
    let (mut first_abstract, mut first_num, mut cleanup) = (None, None, None);
    let mut found: Vec<(usize, i64, std::ops::Range<usize>)> = Vec::new();
    for c in root.children().filter(|c| c.is_element()) {
        let (kind, id) = if is_w(c, "numPicBullet") {
            (0, "numPicBulletId")
        } else if is_w(c, "abstractNum") {
            first_abstract.get_or_insert(c.range().start);
            (1, "abstractNumId")
        } else if is_w(c, "num") {
            first_num.get_or_insert(c.range().start);
            (2, "numId")
        } else {
            if is_w(c, "numIdMacAtCleanup") {
                cleanup.get_or_insert(c.range().start);
            }
            continue;
        };
        if let Some(id) = int(attr(c, id)) {
            found.push((kind, id, c.range()));
        }
    }
    // A repeated id: the table holds the last one read, so the earlier ones are kept as they are.
    let last: HashMap<(usize, i64), usize> = found.iter().enumerate().map(|(i, (kind, id, _))| ((*kind, *id), i)).collect();
    let mut seen: [HashSet<i64>; 3] = Default::default();
    for (i, (kind, id, range)) in found.iter().enumerate() {
        seen[*kind].insert(*id);
        match tables[*kind].get(id) {
            Some(_) if last[&(*kind, *id)] != i => {}
            Some(raw) if raw.as_str() == &xml[range.clone()] => {}
            Some(raw) => splices.push((range.clone(), raw.to_string())),
            None => splices.push((range.clone(), String::new())),
        }
    }
    // A self-closing root (`<w:numbering …/>`, a numbering part with no lists) is opened only when something goes in.
    let (open, end) = (root.range().start, root.range().end);
    let empty = xml[open..end].ends_with("/>");
    let close = if empty { end } else { xml.rfind("</").ok_or("a numbering part without its end tag")? };
    let new = |now: &BTreeMap<i64, String>, seen: &HashSet<i64>| -> String {
        now.iter().filter(|(k, _)| !seen.contains(k)).map(|(_, r)| r.as_str()).collect()
    };
    let (new_pics, new_abstracts, new_nums) = (new(&pics, &seen[0]), new(&abstracts, &seen[1]), new(&nums, &seen[2]));
    // The `w:num`s end before a `w:numIdMacAtCleanup`.
    let tail = cleanup.unwrap_or(close);
    let abstracts_at = first_abstract.or(first_num).unwrap_or(tail);
    if !new_pics.is_empty() {
        splices.push((abstracts_at..abstracts_at, new_pics));
    }
    if !new_abstracts.is_empty() {
        let at = first_num.unwrap_or(tail);
        splices.push((at..at, new_abstracts));
    }
    if !new_nums.is_empty() {
        splices.push((tail..tail, new_nums));
    }
    // Its `/>` becomes `>`, and its end tag follows what is added (sorted after the additions at the same place).
    if empty && !splices.is_empty() {
        let tag = &xml[open + 1..end];
        let tag = &tag[..tag.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(tag.len())];
        splices.push((end - 2..end, ">".to_string()));
        splices.push((end..end, format!("</{tag}>")));
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
