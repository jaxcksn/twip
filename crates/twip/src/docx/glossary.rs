//! The glossary document part (`w:glossaryDocument`): building blocks (`w:docPart`) and how the style and numbering
//! ids their bodies use map onto the document's tables. A style whose name the document also defines resolves with
//! the document's definition; a style only the glossary defines joins the document's table, marked as the
//! glossary's, under its own id or, when the document already uses that id for another style, under a new one. The
//! glossary's lists join the document's numbering under ids past the document's.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use roxmltree::Node;

use super::model::Gallery;
use super::numbering::Numbering;
use super::props::{ParaProps, RunProps};
use super::styles::{StyleKind, StyleOrigin, Styles};
use super::xml::*;

/// Ids as the glossary's parts write them, and the ids the document's tables hold them under. Ids that are the same
/// in both aren't stored.
#[derive(Clone, Debug, PartialEq)]
pub struct IdMap<K: Eq + Hash> {
    to_view: HashMap<K, K>,
    to_glossary: HashMap<K, K>,
}

impl<K: Eq + Hash> Default for IdMap<K> {
    fn default() -> Self {
        IdMap { to_view: HashMap::new(), to_glossary: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone> IdMap<K> {
    fn insert(&mut self, glossary: K, view: K) {
        if glossary != view {
            self.to_glossary.insert(view.clone(), glossary.clone());
            self.to_view.insert(glossary, view);
        }
    }

    /// The document table's id for a glossary id.
    pub fn view(&self, glossary: &K) -> K {
        self.to_view.get(glossary).unwrap_or(glossary).clone()
    }

    /// The glossary's id for a document table id.
    pub fn glossary(&self, view: &K) -> K {
        self.to_glossary.get(view).unwrap_or(view).clone()
    }
}

/// How the glossary's style, list and list definition ids map onto the document's tables.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossaryMap {
    pub styles: IdMap<String>,
    pub nums: IdMap<i64>,
    pub abstracts: IdMap<i64>,
}

impl GlossaryMap {
    /// The glossary part's XML with its body's style and list ids as the document's tables hold them.
    pub fn to_view_xml<'a>(&self, xml: &'a str) -> Cow<'a, str> {
        remap_xml(xml, |s| self.styles.view(&s.to_string()), |n| self.nums.view(&n))
    }

    /// Part XML written from the model, with its ids back as the glossary's parts name them.
    pub fn to_glossary_xml<'a>(&self, xml: &'a str) -> Cow<'a, str> {
        remap_xml(xml, |s| self.styles.glossary(&s.to_string()), |n| self.nums.glossary(&n))
    }

    fn map_ppr(&self, p: &mut ParaProps) {
        if let Some(s) = &mut p.style {
            *s = self.styles.view(s);
        }
        // numId 0 removes numbering; it names no list.
        if let Some(n) = p.num_id.as_mut().filter(|n| **n != 0) {
            *n = self.nums.view(n);
        }
    }

    fn map_rpr(&self, p: &mut RunProps) {
        if let Some(s) = &mut p.style {
            *s = self.styles.view(s);
        }
    }
}

/// `xml` with every `w:pStyle`, `w:rStyle` and `w:tblStyle` value through `style` and every `w:numId` (but 0) through
/// `num`; unchanged (borrowed) when nothing maps or it isn't well-formed.
fn remap_xml<'a>(xml: &'a str, style: impl Fn(&str) -> String, num: impl Fn(i64) -> i64) -> Cow<'a, str> {
    let Ok(dom) = roxmltree::Document::parse(xml) else { return Cow::Borrowed(xml) };
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for n in dom.descendants().filter(|n| n.is_element() && n.tag_name().namespace() == Some(W)) {
        let Some(a) = n.attributes().find(|a| a.name() == "val" && a.namespace() == Some(W)) else { continue };
        let new = match n.tag_name().name() {
            "pStyle" | "rStyle" | "tblStyle" => style(a.value()),
            "numId" => match a.value().trim().parse::<i64>() {
                Ok(v) if v != 0 => num(v).to_string(),
                _ => continue,
            },
            _ => continue,
        };
        if new != a.value() {
            edits.push((a.range_value(), super::write::props::esc(&new)));
        }
    }
    if edits.is_empty() {
        return Cow::Borrowed(xml);
    }
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(xml.len() + 16 * edits.len());
    let mut at = 0;
    for (r, text) in edits {
        out.push_str(&xml[at..r.start]);
        out.push_str(&text);
        at = r.end;
    }
    out.push_str(&xml[at..]);
    Cow::Owned(out)
}

/// Adds the glossary's styles and lists to the document's tables and returns how their ids map.
pub fn merge_tables(styles: &mut Styles, numbering: &mut Numbering, glossary_styles: Styles, glossary_numbering: Numbering) -> GlossaryMap {
    let mut map = GlossaryMap::default();
    let fresh = |ids: Vec<i64>, from: i64, m: &mut IdMap<i64>| {
        for (k, g) in ids.into_iter().enumerate() {
            m.insert(g, from + k as i64);
        }
    };
    let sorted = |mut v: Vec<i64>| {
        v.sort_unstable();
        v
    };
    let next_abstract = numbering.abstracts.keys().max().map_or(0, |m| m + 1);
    fresh(sorted(glossary_numbering.abstracts.keys().copied().collect()), next_abstract, &mut map.abstracts);
    let next_num = numbering.nums.keys().max().map_or(1, |m| m + 1).max(1);
    fresh(sorted(glossary_numbering.nums.keys().copied().collect()), next_num, &mut map.nums);

    // Style names compare as names do in the styles part: without case.
    let name = |id: &str, n: &str| if n.is_empty() { id.to_lowercase() } else { n.to_lowercase() };
    let by_name: HashMap<String, String> = styles.by_id.values().map(|s| (name(&s.id, &s.name), s.id.clone())).collect();
    let mut ids: Vec<&String> = glossary_styles.by_id.keys().collect();
    ids.sort();
    let mut own = Vec::new();
    for id in ids {
        let g = &glossary_styles.by_id[id];
        match by_name.get(&name(&g.id, &g.name)) {
            Some(doc_id) => map.styles.insert(g.id.clone(), doc_id.clone()),
            None => {
                let view = if styles.by_id.contains_key(&g.id) { format!("~g~{}", g.id) } else { g.id.clone() };
                map.styles.insert(g.id.clone(), view);
                own.push(g.id.clone());
            }
        }
    }
    for id in &own {
        let mut s = glossary_styles.by_id[id].clone();
        s.id = map.styles.view(&s.id);
        s.based_on = s.based_on.map(|b| map.styles.view(&b));
        s.next = s.next.map(|n| map.styles.view(&n));
        map.map_ppr(&mut s.base.ppr);
        map.map_rpr(&mut s.base.rpr);
        s.origin = StyleOrigin::Glossary;
        if s.kind == StyleKind::Paragraph {
            styles.paragraph_order.push(s.id.clone());
        }
        styles.by_id.insert(s.id.clone(), s);
    }
    for (g, mut a) in glossary_numbering.abstracts {
        for l in a.levels.iter_mut().flatten() {
            l.pstyle = l.pstyle.take().map(|s| map.styles.view(&s));
        }
        a.num_style_link = a.num_style_link.map(|s| map.styles.view(&s));
        a.origin = StyleOrigin::Glossary;
        numbering.abstracts.insert(map.abstracts.view(&g), a);
    }
    for (g, mut n) in glossary_numbering.nums {
        n.abstract_id = map.abstracts.view(&n.abstract_id);
        n.origin = StyleOrigin::Glossary;
        numbering.nums.insert(map.nums.view(&g), n);
    }
    map
}

/// A `w:docPart`'s properties: name, category, gallery and guid. `None` when it has no name.
pub(super) fn part_props(part: Node) -> Option<(String, String, Gallery, Option<String>)> {
    let pr = child(part, "docPartPr")?;
    let name = child(pr, "name").and_then(val)?.to_string();
    let category = child(pr, "category");
    let category_name = category.and_then(|c| child(c, "name")).and_then(val).unwrap_or("").to_string();
    let gallery = Gallery::parse(category.and_then(|c| child(c, "gallery")).and_then(val).unwrap_or(""));
    let guid = child(pr, "guid").and_then(val).map(str::to_string);
    Some((name, category_name, gallery, guid))
}

#[cfg(test)]
mod tests {
    use super::super::model::*;
    use super::super::styles::StyleOrigin;

    fn fixture() -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-SUBTEMPLATES.docx");
        std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    fn paras(blocks: &[Block]) -> Vec<&Paragraph> {
        let mut out = Vec::new();
        for b in blocks {
            match b {
                Block::Para(p) => out.push(p),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| out.extend(paras(&c.blocks))),
            }
        }
        out
    }

    fn text(p: &Paragraph) -> String {
        p.inlines.iter().filter_map(|i| if let Inline::Text(s, _) = i { Some(s.as_str()) } else { None }).collect()
    }

    fn first_run(p: &Paragraph) -> &Run {
        p.inlines.iter().find_map(|i| if let Inline::Text(_, r) = i { Some(r) } else { None }).expect("a text run")
    }

    #[test]
    fn reads_building_blocks_with_their_properties() {
        let doc = super::super::read_editable(&fixture()).unwrap();
        let g = doc.glossary.as_ref().expect("a glossary");
        let names: Vec<(&str, &str)> = g.parts.iter().map(|b| (b.name.as_str(), b.category.as_str())).collect();
        assert_eq!(
            names,
            [
                ("Violation", "Sub-templates"),
                ("FeeRow", "Sub-templates"),
                ("Appeals notice", "Sub-templates"),
                ("Cycle-A", "Sub-templates"),
                ("Cycle-B", "Sub-templates"),
                ("Greeting", "General")
            ]
        );
        assert_eq!(g.parts[0].gallery, Gallery::Custom(1));
        assert_eq!(g.parts[5].gallery, Gallery::AutoText);
        assert!(g.parts.iter().all(|b| b.editable && b.state == PartState::AsRead));
        assert_eq!(g.find("Violation", "Sub-templates", None), Some(0));
        assert_eq!(g.find("violation", "Sub-templates", None), None);
        let src = doc.source.as_ref().and_then(|s| s.glossary.as_ref()).expect("the glossary as read");
        assert_eq!(src.parts.len(), 6);
        assert!(src.xml[src.parts[1].0.clone()].contains("FeeRow"));
    }

    #[test]
    fn body_styles_resolve_by_name_with_the_document_first() {
        let doc = super::super::read_editable(&fixture()).unwrap();
        let g = doc.glossary.as_ref().unwrap();
        let res = doc.resolver.as_ref().unwrap();
        let body = paras(&g.parts[0].body);
        // Heading 1 is in both parts: the document's (16pt), not the glossary's (24pt).
        assert_eq!(body[0].style.style_id.as_deref(), Some("Heading1"));
        assert_eq!(first_run(body[0]).size, 16.0);
        // Notice is only the glossary's: its own definition, kept under its id.
        assert_eq!(body[1].style.style_id.as_deref(), Some("Notice"));
        assert!(first_run(body[1]).italic);
        assert_eq!(res.styles.by_id["Notice"].origin, StyleOrigin::Glossary);
        assert_eq!(res.styles.by_id["Heading1"].origin, StyleOrigin::Document);
        // The glossary's Header is another style than the document's Header: it moves to a free id.
        assert_eq!(g.map.styles.view(&"Header".to_string()), "~g~Header");
        assert_eq!(g.map.styles.glossary(&"~g~Header".to_string()), "Header");
        let fee = paras(&g.parts[1].body);
        assert_eq!(fee[1].style.style_id.as_deref(), Some("~g~Header"));
        assert_eq!(res.styles.by_id["Header"].name, "header");
    }

    #[test]
    fn body_lists_pictures_revisions_and_comments_are_read() {
        let doc = super::super::read_editable(&fixture()).unwrap();
        let g = doc.glossary.as_ref().unwrap();
        let body = paras(&g.parts[0].body);
        let labels: Vec<&str> = body[2..4].iter().map(|p| p.list.as_ref().map_or("", |l| l.text.as_str())).collect();
        assert_eq!(labels, ["1.", "2."]);
        // The glossary's list 1 is held past the document's lists.
        assert_ne!(g.map.nums.view(&1), 1);
        let picture = body[5].inlines.iter().find_map(|i| if let Inline::Picture(p, _) = i { p.image } else { None }).expect("a picture");
        assert!(doc.images[picture].starts_with(b"\x89PNG"));
        assert!(text(body[1]).contains("again"));
        let inserted = body[1].inlines.iter().any(|i| matches!(i, Inline::Text(s, r) if s.contains("again") && r.rev.ins.is_some()));
        assert!(inserted, "the tracked insertion is a revision");
        // The comment is the glossary's own: its markers aren't orphans, and they stay in the paragraph's source.
        assert_eq!(doc.diagnostics.count(super::super::diagnostics::WarningCode::CommentReferenceOrphan), 0);
        let src = body[1].src.as_ref().expect("a source");
        let xml = &doc.source.as_ref().unwrap().glossary.as_ref().unwrap().xml;
        assert!(src.ranges.iter().any(|r| xml[r.clone()].contains("commentReference")));
    }

    #[test]
    fn a_part_without_a_name_is_skipped_with_a_warning() {
        let bytes = fixture();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).unwrap();
            let name = f.name().unwrap().to_string();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut data).unwrap();
            if name == "word/glossary/document.xml" {
                data = String::from_utf8(data).unwrap().replacen(r#"<w:name w:val="FeeRow"/>"#, "", 1).into_bytes();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut out, &data).unwrap();
        }
        let doc = super::super::read_editable(&out.finish().unwrap().into_inner()).unwrap();
        let g = doc.glossary.as_ref().unwrap();
        assert_eq!(g.parts.len(), 5);
        assert!(g.parts.iter().all(|b| b.name != "FeeRow"));
        assert_eq!(doc.diagnostics.count(super::super::diagnostics::WarningCode::GlossaryUnreadable), 1);
    }
}
