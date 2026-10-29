//! Lossless save: the package as read, with only the edited paragraphs of the main part, headers and footers rewritten.
//!
//! Every body paragraph and table remembers where it was read from ([`ParaSrc`], [`Table::src`]). Saving compares the
//! edited body with the body as read ([`Source::blocks`]), block list by block list: a paragraph or table still there
//! anchors a group with the new paragraphs after it and the vanished ones it absorbed. Unchanged groups keep their
//! bytes. A changed group's units (and paragraph marks) are diffed against its source's, and the source XML is walked
//! again: kept text stays in its run (and hyperlink, content control, ...), typed text joins the run it was typed
//! into, text in another formatting gets a run of its own (its run's direct `w:rPr`), deleted text and objects go,
//! and new or deleted marks split or join paragraphs. Everything that isn't text (bookmarks, comments, field codes,
//! hidden runs, ...) is copied where it was.

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::ops::Range;
use std::rc::Rc;

use roxmltree::Node;

use super::review::{CommentId, People, RangeKind, RangeMark, RevisionMark, SectionChange};
use super::xml::{W, child, is_w};
use super::*;

mod comments;
pub mod props;
mod revisions;

use super::props::{Border, Jc, Width};
use props::{Cx, Props};

/// The edited document as package bytes: the original package when nothing changed.
pub fn save(doc: &Document) -> Result<Vec<u8>, String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let parts = edited_parts(doc)?;
    if parts.is_empty() && doc.removed_data.is_empty() {
        return Ok(src.package.clone());
    }
    repackage_without(&src.package, &parts, &removed_data_files(doc), zip::CompressionMethod::Deflated)
}

/// The parts the edits changed, with their new XML: the main part, headers and footers, and the notes parts.
pub fn edited_parts(doc: &Document) -> Result<Vec<(String, Vec<u8>)>, String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut parts = Vec::new();
    if let Some(xml) = main_xml(doc, false)? {
        parts.push((src.main.clone(), xml.into_bytes()));
    }
    for (name, (xml, orig)) in &src.stories {
        let now = doc.stories.get(name).map_or(&[][..], Vec::as_slice);
        if let Some(xml) = part_xml(xml, &boxes(&[orig], &[now]), &[(orig, now)], false, &doc.controls, &gone_controls(doc), &doc.people)? {
            parts.push((name.clone(), xml.into_bytes()));
        }
    }
    for (name, xml, foot) in &src.notes {
        let (was, is) = if *foot { (&src.footnotes, &doc.footnotes) } else { (&src.endnotes, &doc.endnotes) };
        let mut ids: Vec<&i64> = was.keys().filter(|id| is.contains_key(id)).collect();
        ids.sort();
        let lists: Vec<(&[Block], &[Block])> = ids.iter().map(|id| (was[*id].as_slice(), is[*id].as_slice())).collect();
        let (o, n): (Vec<&[Block]>, Vec<&[Block]>) = lists.iter().copied().unzip();
        // Notes inserted since reading are added at the part's end; deleted ones go whole.
        let mut added: Vec<(&i64, &Vec<Block>)> = is.iter().filter(|(id, _)| !was.contains_key(id)).collect();
        added.sort_by_key(|(id, _)| **id);
        let removed: Vec<i64> = was.keys().filter(|id| !is.contains_key(id)).copied().collect();
        let tag = if *foot { "footnote" } else { "endnote" };
        if let Some(xml) = notes_xml(xml, &boxes(&o, &n), &lists, tag, &added, &removed, &doc.controls, &gone_controls(doc), &doc.people)? {
            parts.push((name.clone(), xml.into_bytes()));
        }
    }
    // Notes inserted in a document that had no notes part of their kind: the part, its relationship and its type.
    for (foot, notes) in [(true, &doc.footnotes), (false, &doc.endnotes)] {
        if notes.is_empty() || src.notes.iter().any(|(_, _, f)| *f == foot) {
            continue;
        }
        // Both kinds new: the second builds on the first's relationships and content types.
        let made = new_notes_part(&src.package, &src.main, foot, notes, &parts, &doc.people)?;
        for (n, b) in made {
            match parts.iter_mut().find(|(m, _)| *m == n) {
                Some(p) => p.1 = b,
                None => parts.push((n, b)),
            }
        }
    }
    section_pages(doc, &mut parts)?;
    new_stories(doc, &mut parts)?;
    added_lists(doc, &mut parts)?;
    filled_placeholders(doc, &mut parts)?;
    review_parts(doc, &mut parts)?;
    default_tab_part(doc, &mut parts)?;
    add_images(&src.package, &doc.images, &mut parts)?;
    data_parts(doc, &mut parts)?;
    Ok(parts)
}

const SETTINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";

/// The comment, people and settings parts, when the review they hold changed since reading: compared with the
/// document read again from its package, so a part the reader can't make the same again is never rewritten.
// shortcut: the package is read again to compare (only for documents with comment or people parts, review content, or
// a tracking setting that seems changed); keeping the review as read on `Source` would save the read.
fn review_parts(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let settings = package::related(&src.package, parts, &src.main, SETTINGS)?;
    let settings_xml = match &settings {
        Some(n) => package::read(&src.package, parts, n)?,
        None => None,
    };
    let tracking = settings_xml.as_deref().is_some_and(tracks_revisions);
    let mut has_parts = false;
    for rel in [
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        "http://schemas.microsoft.com/office/2011/relationships/people",
    ] {
        has_parts |= package::related(&src.package, parts, &src.main, rel)?.is_some();
    }
    let review = !doc.comments.by_id.is_empty() || doc.people.persons.iter().any(|p| p.identity.is_some());
    if !has_parts && !review && doc.settings.track_revisions == tracking {
        return Ok(());
    }
    let read = super::read_editable(&src.package)?;
    if doc.settings.track_revisions != read.settings.track_revisions
        && let (Some(name), Some(xml)) = (settings, settings_xml)
    {
        package::set(parts, &name, with_track_revisions(&xml, doc.settings.track_revisions)?.into_bytes());
    }
    comments::save(doc, &read, &src.package, &src.main, parts)
}

/// CT_Settings' children before `w:trackRevisions`.
const BEFORE: &[&str] = &[
    "writeProtection",
    "view",
    "zoom",
    "removePersonalInformation",
    "removeDateAndTime",
    "doNotDisplayPageBoundaries",
    "displayBackgroundShape",
    "printPostScriptOverText",
    "printFractionalCharacterWidth",
    "printFormsData",
    "embedTrueTypeFonts",
    "embedSystemFonts",
    "saveSubsetFonts",
    "saveFormsData",
    "mirrorMargins",
    "alignBordersAndEdges",
    "bordersDoNotSurroundHeader",
    "bordersDoNotSurroundFooter",
    "gutterAtTop",
    "hideSpellingErrors",
    "hideGrammaticalErrors",
    "activeWritingStyle",
    "proofState",
    "formsDesign",
    "attachedTemplate",
    "linkStyles",
    "stylePaneFormatFilter",
    "stylePaneSortMethod",
    "documentType",
    "mailMerge",
    "revisionView",
];

/// CT_Settings' children from `w:trackRevisions` to before `w:defaultTabStop`.
const BEFORE_DEFAULT_TAB: &[&str] =
    &["trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet"];

/// The settings part with `w:defaultTabStop` at the document's default tab spacing, when it changed since reading.
// shortcut: a document with no settings part keeps the default spacing it was read with.
fn default_tab_part(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let Some(name) = package::related(&src.package, parts, &src.main, SETTINGS)? else { return Ok(()) };
    let Some(xml) = package::read(&src.package, parts, &name)? else { return Ok(()) };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let old = root.children().find(|c| is_w(*c, "defaultTabStop"));
    let read = old.and_then(|c| c.attribute((W, "val"))?.parse::<f64>().ok()).map(|t| t / 20.0).filter(|t| *t > 0.0).unwrap_or(36.0);
    if (read - doc.settings.default_tab).abs() < 0.025 {
        return Ok(());
    }
    let w = match root.lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let el = format!("<{w}defaultTabStop {w}val=\"{}\"/>", (doc.settings.default_tab * 20.0).round() as i64);
    let mut out = xml.clone();
    match old {
        Some(c) => out.replace_range(c.range(), &el),
        None => {
            let after =
                root.children().rfind(|c| c.is_element() && BEFORE.iter().chain(BEFORE_DEFAULT_TAB).any(|b| is_w(*c, b))).map(|c| c.range().end);
            let r = root.range();
            let tag = &xml[r.clone()];
            if after.is_none() && tag.ends_with("/>") {
                let name = tag[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
                out.replace_range(r.end - 2..r.end, &format!(">{el}</{name}>"));
            } else {
                out.insert_str(after.unwrap_or_else(|| r.start + tag.find('>').map_or(0, |e| e + 1)), &el);
            }
        }
    }
    package::set(parts, &name, out.into_bytes());
    Ok(())
}

/// Whether the settings part turns tracking on (`w:trackRevisions`).
fn tracks_revisions(xml: &str) -> bool {
    let Ok(dom) = roxmltree::Document::parse(xml) else { return false };
    dom.root_element().children().any(|c| is_w(c, "trackRevisions") && c.attribute((W, "val")).is_none_or(|v| !matches!(v, "0" | "false" | "off")))
}

/// The settings part with `w:trackRevisions` set or removed, everything else as it was.
fn with_track_revisions(xml: &str, on: bool) -> Result<String, String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let mut out = xml.to_string();
    // Any `w:trackRevisions` there (an off one too) goes; an on one is written where the schema puts it.
    let old: Vec<Range<usize>> = root.children().filter(|c| is_w(*c, "trackRevisions")).map(|c| c.range()).collect();
    for r in old.iter().rev() {
        out.replace_range(r.clone(), "");
    }
    if on {
        let w = match root.lookup_prefix(W) {
            Some(p) if !p.is_empty() => format!("{p}:"),
            _ => String::new(),
        };
        let after = root.children().rfind(|c| c.is_element() && BEFORE.iter().any(|b| is_w(*c, b))).map(|c| c.range().end);
        let r = root.range();
        let el = &xml[r.clone()];
        if after.is_none() && el.ends_with("/>") {
            // An empty, self-closing settings root: opened and closed around it.
            let name = el[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
            out.replace_range(r.end - 2..r.end, &format!("><{w}trackRevisions/></{name}>"));
            return Ok(out);
        }
        let at = after.unwrap_or_else(|| r.start + el.find('>').map_or(0, |e| e + 1));
        let at = at - old.iter().filter(|o| o.end <= at).map(|o| o.len()).sum::<usize>();
        out.insert_str(at, &format!("<{w}trackRevisions/>"));
    }
    Ok(out)
}

/// The files of the data parts removed since reading: each item, its properties and its relationships.
fn removed_data_files(doc: &Document) -> Vec<String> {
    let Some(src) = doc.source.as_ref() else { return Vec::new() };
    let mut out = Vec::new();
    for item in &doc.removed_data {
        out.push(item.clone());
        if let Some((props, rels)) = item_parts(&src.package, item) {
            out.push(props);
            out.push(rels);
        }
    }
    out
}

/// A data item's properties part and relationships part, as the package has them.
fn item_parts(package: &[u8], item: &str) -> Option<(String, String)> {
    use std::io::Read;
    let (dir, name) = item.rsplit_once('/').unwrap_or(("", item));
    let rels = if dir.is_empty() { format!("_rels/{name}.rels") } else { format!("{dir}/_rels/{name}.rels") };
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).ok()?;
    let mut x = String::new();
    zin.by_name(&rels).ok()?.read_to_string(&mut x).ok()?;
    let d = roxmltree::Document::parse(&x).ok()?;
    let t = d.root_element().children().find(|r| r.attribute("Type").is_some_and(|t| t.ends_with("/customXmlProps")))?.attribute("Target")?;
    let props = if dir.is_empty() { t.to_string() } else { format!("{dir}/{t}") };
    Some((props, rels))
}

/// Data parts: changed ones rewritten in place; new ones added with their properties, relationships,
/// the main part's relationship and their content type; removed ones lose the main part's relationship and their
/// content type (their files go in [`removed_data_files`]).
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn data_parts(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let changed: Vec<&DataPart> = doc.data_parts.iter().filter(|p| p.changed).collect();
    if changed.is_empty() && doc.removed_data.is_empty() {
        return Ok(());
    }
    use std::io::Read;
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package)).map_err(|e| e.to_string())?;
    let names: Vec<String> = zin.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    let mut read = |n: &str, parts: &[(String, Vec<u8>)]| -> Result<String, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map_err(|e| e.to_string());
        }
        let mut s = String::new();
        zin.by_name(n).map_err(|e| format!("{n}: {e}"))?.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    };
    let main_rels = match src.main.rsplit_once('/') {
        Some((d, f)) => format!("{d}/_rels/{f}.rels"),
        None => format!("_rels/{}.rels", src.main),
    };
    let mut rels = read(&main_rels, parts)?;
    let mut types = read("[Content_Types].xml", parts)?;
    let set = |parts: &mut Vec<(String, Vec<u8>)>, name: &str, bytes: Vec<u8>| match parts.iter_mut().find(|(m, _)| m == name) {
        Some(p) => p.1 = bytes,
        None => parts.push((name.to_string(), bytes)),
    };
    // The main part's relationship target for an item part (relative to the main part's folder).
    let main_dir = src.main.rsplit_once('/').map_or("", |(d, _)| d);
    let target_of = |item: &str| if main_dir.is_empty() { item.to_string() } else { format!("../{item}") };
    let props_type = "application/vnd.openxmlformats-officedocument.customXmlProperties+xml";
    for item in &doc.removed_data {
        let t = target_of(item);
        if let Some(k) = rels.find(&format!("Target=\"{t}\"")) {
            let a = rels[..k].rfind("<Relationship").unwrap_or(k);
            let b = rels[k..].find("/>").map_or(rels.len(), |e| k + e + 2);
            rels.replace_range(a..b, "");
        }
        if let Some((props, _)) = item_parts(&src.package, item) {
            let o = format!("<Override PartName=\"/{props}\" ContentType=\"{props_type}\"/>");
            types = types.replace(&o, "");
        }
    }
    let mut next = (1..).find(|n| !names.iter().any(|f| *f == format!("customXml/item{n}.xml"))).unwrap_or(1);
    for p in changed {
        if let Some(item) = &p.src {
            set(parts, item, p.xml.clone().into_bytes());
            continue;
        }
        while names.iter().chain(parts.iter().map(|(n, _)| n)).any(|f| *f == format!("customXml/item{next}.xml")) {
            next += 1;
        }
        let (item, props, item_rels) =
            (format!("customXml/item{next}.xml"), format!("customXml/itemProps{next}.xml"), format!("customXml/_rels/item{next}.xml.rels"));
        next += 1;
        let decl = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>";
        let props_xml = format!(
            "{decl}<ds:datastoreItem ds:itemID=\"{}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"><ds:schemaRefs/></ds:datastoreItem>",
            p.item_id
        );
        let item_rels_xml = format!(
            "{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps\" Target=\"itemProps{}.xml\"/></Relationships>",
            next - 1
        );
        set(parts, &item, p.xml.clone().into_bytes());
        set(parts, &props, props_xml.into_bytes());
        set(parts, &item_rels, item_rels_xml.into_bytes());
        // After every other relationship, so the parts read back in the model's order (reader.rs `data_parts`).
        // (Ids are numbered by their trailing digits, as the reader orders them: rIdCustomXml1 is 1.)
        let number = |id: &str| id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().ok();
        let mut k = rels.match_indices(" Id=\"").filter_map(|(i, m)| number(rels[i + m.len()..].split('"').next()?)).max().unwrap_or(0) + 1;
        while rels.contains(&format!("Id=\"rId{k}\"")) {
            k += 1;
        }
        let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
        rels.insert_str(
            close,
            &format!(
                "<Relationship Id=\"rId{k}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml\" Target=\"{}\"/>",
                target_of(&item)
            ),
        );
        let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
        types.insert_str(close, &format!("<Override PartName=\"/{props}\" ContentType=\"{props_type}\"/>"));
    }
    set(parts, &main_rels, rels.into_bytes());
    set(parts, "[Content_Types].xml", types.into_bytes());
    Ok(())
}

/// Sections whose page was edited ([`Document::edited_sections`]): their `w:sectPr` with the page size, margins,
/// columns and different first page set, everything else in it kept.
fn section_pages(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    if doc.edited_sections.is_empty() {
        return Ok(());
    }
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut xml = match parts.iter().find(|(n, _)| *n == src.main) {
        Some((_, b)) => String::from_utf8(b.clone()).map_err(|e| e.to_string())?,
        None => src.xml.clone(),
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let sects: Vec<Range<usize>> = dom
        .descendants()
        .filter(|n| is_w(*n, "sectPr") && n.parent().is_some_and(|p| is_w(p, "body") || is_w(p, "pPr")))
        .map(|n| n.range())
        .collect();
    drop(dom);
    let mut edited: Vec<usize> = doc.edited_sections.iter().copied().filter(|&k| k < sects.len().min(doc.sections.len())).collect();
    edited.sort_unstable_by(|a, b| b.cmp(a));
    for k in edited {
        let r = sects[k].clone();
        let s = &doc.sections[k];
        let new = sect_with_page(&xml[r.clone()], &w, &s.page(), s.change.as_deref(), &doc.people);
        xml.replace_range(r, &new);
    }
    match parts.iter_mut().find(|(n, _)| *n == src.main) {
        Some(p) => p.1 = xml.into_bytes(),
        None => parts.push((src.main.clone(), xml.into_bytes())),
    }
    Ok(())
}

/// A `w:sectPr` with `p`'s page size, margins, columns and different first page, and `change` as its tracked change.
fn sect_with_page(sect: &str, w: &str, p: &SectionPage, change: Option<&SectionChange>, people: &People) -> String {
    // CT_SectPr's children in schema order.
    const ORDER: [&str; 22] = [
        "headerReference",
        "footerReference",
        "footnotePr",
        "endnotePr",
        "type",
        "pgSz",
        "pgMar",
        "paperSrc",
        "pgBorders",
        "lnNumType",
        "pgNumType",
        "cols",
        "formProt",
        "vAlign",
        "noEndnote",
        "titlePg",
        "textDirection",
        "bidi",
        "rtlGutter",
        "docGrid",
        "printerSettings",
        "sectPrChange",
    ];
    let mut s = sect.to_string();
    if s.ends_with("/>") {
        let name = s[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("w:sectPr").to_string();
        s = format!("{}></{name}>", &s[..s.len() - 2]);
    }
    let open_end = s.find('>').map_or(0, |e| e + 1);
    // The child `local` replaced by `tag` (None: removed), or `tag` inserted where the schema puts it.
    let put = |s: &mut String, local: &str, tag: Option<String>| {
        let found = find_tag(s, local, open_end).map(|(a, e)| element(&s[a..], local).map_or(a..e, |r| a + r.start..a + r.end));
        match (found, tag) {
            (Some(r), Some(t)) => s.replace_range(r, &t),
            (Some(r), None) => s.replace_range(r, ""),
            (None, Some(t)) => {
                let later = &ORDER[ORDER.iter().position(|x| *x == local).map_or(ORDER.len(), |i| i + 1)..];
                let at =
                    later.iter().filter_map(|l| find_tag(s, l, open_end).map(|(a, _)| a)).min().unwrap_or_else(|| s.rfind("</").unwrap_or(s.len()));
                s.insert_str(at, &t);
            }
            (None, None) => {}
        }
    };
    // The tracked change goes first (its earlier `w:sectPr` holds the same children) and comes back from the model last.
    put(&mut s, "sectPrChange", None);
    // A change recorded here has no earlier `w:sectPr` as read: it is this one as read, with the earlier page.
    let earlier = change.filter(|c| c.previous_raw.0.is_none()).map(|c| {
        let mut base = s.clone();
        for local in ["headerReference", "footerReference"] {
            while find_tag(&base, local, open_end).is_some() {
                put(&mut base, local, None);
            }
        }
        Raw(Some(sect_with_page(&base, w, &c.previous, None, people).into()))
    });
    let existing = |s: &str, local: &str| find_tag(s, local, open_end).map(|(a, e)| s[a..e].to_string());
    let mut sz = existing(&s, "pgSz").unwrap_or_else(|| format!("<{w}pgSz/>"));
    sz = set_attr(&sz, &format!("{w}w"), &twips(p.page_w).to_string());
    sz = set_attr(&sz, &format!("{w}h"), &twips(p.page_h).to_string());
    sz = if p.page_w > p.page_h { set_attr(&sz, &format!("{w}orient"), "landscape") } else { set_attr(&sz, &format!("{w}orient"), "portrait") };
    put(&mut s, "pgSz", Some(sz));
    let mut mar = existing(&s, "pgMar").unwrap_or_else(|| format!("<{w}pgMar/>"));
    for (name, v) in
        [("top", p.top), ("right", p.right), ("bottom", p.bottom), ("left", p.left), ("header", p.header), ("footer", p.footer), ("gutter", p.gutter)]
    {
        mar = set_attr(&mar, &format!("{w}{name}"), &twips(v).to_string());
    }
    put(&mut s, "pgMar", Some(mar));
    let c = &p.columns;
    let sep = if c.separator { format!(" {w}sep=\"1\"") } else { String::new() };
    let cols = if !c.equal && c.cols.len() > 1 {
        let each: String = c.cols.iter().map(|(cw, sp)| format!("<{w}col {w}w=\"{}\" {w}space=\"{}\"/>", twips(*cw), twips(*sp))).collect();
        format!("<{w}cols {w}num=\"{}\"{sep} {w}equalWidth=\"0\">{each}</{w}cols>", c.cols.len())
    } else {
        format!("<{w}cols {w}space=\"{}\" {w}num=\"{}\"{sep}/>", twips(c.space), c.num.max(1))
    };
    put(&mut s, "cols", Some(cols));
    put(&mut s, "titlePg", p.title_pg.then(|| format!("<{w}titlePg/>")));
    put(
        &mut s,
        "sectPrChange",
        change.map(|c| revisions::change(w, "sectPrChange", "sectPr", &c.mark, earlier.as_ref().unwrap_or(&c.previous_raw), people)),
    );
    s
}

/// Placeholders filled since reading: each filled image's media part with the new data (under a new name, its
/// relationships pointing there and its content type added, when the image type changed), and in every part showing
/// it, the picture's extents as fitted and its content control without `w:showingPlcHdr`.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn filled_placeholders(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use std::io::Read;
    if doc.filled.is_empty() {
        return Ok(());
    }
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(mut f) = zin.by_name(n) else { return Ok(None) };
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(Some(s))
    };
    let base = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    // A filled image's size as shown: its first picture's.
    fn size_of(blocks: &[Block], image: usize) -> Option<(f64, f64)> {
        blocks.iter().find_map(|b| match b {
            Block::Para(p) => p.inlines.iter().find_map(|i| match i {
                Inline::Picture(pic, _) if pic.image == Some(image) => Some((pic.w, pic.h)),
                _ => None,
            }),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| size_of(&c.blocks, image)),
        })
    }
    let all: Vec<&Vec<Block>> = doc.sections.iter().map(|s| &s.blocks).chain(doc.stories.values()).collect();
    let mut types = read(parts, "[Content_Types].xml")?.ok_or("no content types")?;
    let mut xml_parts: Vec<String> = vec![src.main.clone()];
    xml_parts.extend(src.stories.keys().cloned());
    let mut filled: Vec<usize> = doc.filled.iter().copied().collect();
    filled.sort_unstable();
    for idx in filled {
        let (Some(old), Some(data)) = (src.image_parts.get(idx).filter(|p| !p.is_empty()), doc.images.get(idx)) else { continue };
        let Some((_, ext)) = super::image::image_type(data) else { continue };
        let old_ext = old.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
        let same = old_ext == ext || (old_ext == "jpg" && ext == "jpeg");
        let new = if same { old.clone() } else { format!("{}.{ext}", old.rsplit_once('.').map_or(old.as_str(), |(s, _)| s)) };
        parts.retain(|(n, _)| n != &new);
        parts.push((new.clone(), data.clone()));
        if !same && !types.to_ascii_lowercase().contains(&format!("extension=\"{ext}\"")) {
            let at = types.rfind("</Types>").ok_or("content types without their end tag")?;
            let mime = super::image::image_type(data).map_or("application/octet-stream", |(m, _)| m);
            types.insert_str(at, &format!(r#"<Default Extension="{ext}" ContentType="{mime}"/>"#));
        }
        let size = all.iter().find_map(|b| size_of(b, idx));
        for part in &xml_parts {
            let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
            let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
            let Some(mut rels) = read(parts, &rels_name)? else { continue };
            // The relationships naming the image: their ids, and their targets pointed at the new name.
            let mut ids = Vec::new();
            let mut out = String::new();
            let mut rest = rels.as_str();
            while let Some(k) = rest.find("<Relationship ") {
                let end = rest[k..].find('>').map_or(rest.len(), |e| k + e + 1);
                let tag = &rest[k..end];
                let target = tag.split("Target=\"").nth(1).and_then(|t| t.split('"').next()).unwrap_or("");
                out.push_str(&rest[..k]);
                if base(target) == base(old) && tag.contains("relationships/image") {
                    if let Some(id) = tag.split("Id=\"").nth(1).and_then(|t| t.split('"').next()) {
                        ids.push(id.to_string());
                    }
                    out.push_str(&tag.replace(&base(old), &base(&new)));
                } else {
                    out.push_str(tag);
                }
                rest = &rest[end..];
            }
            out.push_str(rest);
            rels = out;
            if ids.is_empty() {
                continue;
            }
            let Some(mut xml) = read(parts, part)? else { continue };
            for id in &ids {
                let needle = format!(":embed=\"{id}\"");
                // Last to first: each patch is at or before its occurrence, so earlier ones keep their offsets.
                let at: Vec<usize> = xml.match_indices(&needle).map(|(k, _)| k).collect();
                for k in at.into_iter().rev() {
                    // Its drawing: the fitted extents.
                    if let (Some(d0), Some((w, h))) = (xml[..k].rfind(":drawing>"), size) {
                        let d0 = xml[..d0].rfind('<').unwrap_or(d0);
                        if let Some(d1) = xml[k..].find(":drawing>").map(|e| k + e + ":drawing>".len()) {
                            let patched = Placement { h: None, v: None, size: Some((w, h)), wrap: None, locked: None, alt: None }.apply(&xml[d0..d1]);
                            xml.replace_range(d0..d1, &patched);
                        }
                    }
                    // Its content control's properties (`<w:sdtPr>` … `</w:sdtPr>` just before its content): no
                    // longer showing the placeholder.
                    let Some(close) = xml[..k].rfind(":sdtPr>").and_then(|c| xml[..c].rfind("</")) else { continue };
                    let Some(open) = xml[..close].rfind(":sdtPr").and_then(|o| xml[..o].rfind('<')) else { continue };
                    if let Some(p0) = xml[open..close].find(":showingPlcHdr").map(|e| open + e) {
                        let p0 = xml[..p0].rfind('<').unwrap_or(p0);
                        let p1 = xml[p0..close].find('>').map_or(close, |e| p0 + e + 1);
                        xml.replace_range(p0..p1, "");
                    }
                }
            }
            for (n, b) in [(part.clone(), xml), (rels_name, rels)] {
                match parts.iter_mut().find(|(m, _)| *m == n) {
                    Some(p) => p.1 = b.into_bytes(),
                    None => parts.push((n, b.into_bytes())),
                }
            }
        }
    }
    match parts.iter_mut().find(|(m, _)| m == "[Content_Types].xml") {
        Some(p) => p.1 = types.into_bytes(),
        None => parts.push(("[Content_Types].xml".to_string(), types.into_bytes())),
    }
    Ok(())
}

/// Lists made since reading: their `w:abstractNum`s before the part's first `w:num` (CT_Numbering keeps them
/// first) and their `w:num`s at its end; a numbering part, its relationship and content type are made when there is none.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn added_lists(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use std::io::Read;
    if doc.added_lists.is_empty() {
        return Ok(());
    }
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(mut f) = zin.by_name(n) else { return Ok(None) };
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(Some(s))
    };
    let main = src.main.clone();
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", &main));
    let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let mut rels = read(parts, &rels_name)?.ok_or("no main part relationships")?;
    // The numbering part the main part names, else a new one beside it.
    let target = rels
        .split("<Relationship ")
        .find(|r| r.contains("relationships/numbering\""))
        .and_then(|r| r.split("Target=\"").nth(1))
        .and_then(|t| t.split('"').next())
        .map(str::to_string);
    let name = match &target {
        Some(t) if dir.is_empty() => t.trim_start_matches('/').to_string(),
        Some(t) if t.starts_with('/') => t.trim_start_matches('/').to_string(),
        Some(t) => format!("{dir}/{t}"),
        None => {
            if dir.is_empty() {
                "numbering.xml".to_string()
            } else {
                format!("{dir}/numbering.xml")
            }
        }
    };
    let mut xml = match read(parts, &name)? {
        Some(x) => x,
        None => r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"></w:numbering>"#
            .to_string(),
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let first_num = dom.root_element().children().find(|c| is_w(*c, "num")).map(|n| n.range().start);
    let close = xml.rfind(&format!("</{w}numbering>")).ok_or("numbering without its end tag")?;
    drop(dom);
    let abstracts: String = doc.added_lists.iter().map(|l| l.abstract_xml.clone()).collect();
    let nums: String = doc
        .added_lists
        .iter()
        .map(|l| {
            format!(
                r#"<w:num xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" w:numId="{}"><w:abstractNumId w:val="{}"/></w:num>"#,
                l.num_id, l.abstract_id
            )
        })
        .collect();
    xml.insert_str(close, &nums);
    xml.insert_str(first_num.unwrap_or(close), &abstracts);
    if target.is_none() {
        let mut k = 1;
        while rels.contains(&format!("Id=\"rId{k}\"")) {
            k += 1;
        }
        let at = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
        rels.insert_str(at, &format!(r#"<Relationship Id="rId{k}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#));
        let mut types = read(parts, "[Content_Types].xml")?.ok_or("no content types")?;
        let at = types.rfind("</Types>").ok_or("content types without their end tag")?;
        types.insert_str(
            at,
            &format!(r#"<Override PartName="/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#),
        );
        for (n, b) in [(rels_name, rels), ("[Content_Types].xml".to_string(), types)] {
            match parts.iter_mut().find(|(m, _)| *m == n) {
                Some(p) => p.1 = b.into_bytes(),
                None => parts.push((n, b.into_bytes())),
            }
        }
    }
    match parts.iter_mut().find(|(m, _)| *m == name) {
        Some(p) => p.1 = xml.into_bytes(),
        None => parts.push((name, xml.into_bytes())),
    }
    Ok(())
}

/// Headers and footers made since reading: each part, its relationship from the main part, its content type, and
/// the section references to it (inserted first in their sections' `w:sectPr`, where CT_SectPr has them).
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn new_stories(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use std::io::Read;
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let made: Vec<&String> = doc.stories.keys().filter(|n| !src.stories.contains_key(*n)).collect();
    if made.is_empty() {
        return Ok(());
    }
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<String, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map_err(|e| e.to_string());
        }
        let mut s = String::new();
        zin.by_name(n).map_err(|e| format!("{n}: {e}"))?.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    };
    let main = src.main.clone();
    let (dir, file) = main.rsplit_once('/').unwrap_or(("", &main));
    let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let mut rels = read(parts, &rels_name)?;
    let mut types = read(parts, "[Content_Types].xml")?;
    let mut xml = read(parts, &main)?;
    let mut ids: HashMap<&String, String> = HashMap::new();
    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    for name in &made {
        let footer = doc.sections.iter().any(|s| s.footers.iter().flatten().any(|f| f == *name));
        let (tag, kind) = if footer { ("ftr", "footer") } else { ("hdr", "header") };
        let shell = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:{tag} {NS}></w:{tag}>"#
        );
        let body = new_blocks_xml(&shell, &doc.stories[*name], &doc.people)?;
        let end = shell.rfind("</").ok_or("story shell")?;
        parts.retain(|(n, _)| n != *name);
        parts.push(((*name).clone(), format!("{}{body}{}", &shell[..end], &shell[end..]).into_bytes()));
        let mut k = 1;
        while rels.contains(&format!("Id=\"rId{k}\"")) {
            k += 1;
        }
        let target = name.strip_prefix(&format!("{dir}/")).unwrap_or(name);
        let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
        rels.insert_str(
            close,
            &format!(
                r#"<Relationship Id="rId{k}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#
            ),
        );
        let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
        types.insert_str(
            close,
            &format!(r#"<Override PartName="/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>"#),
        );
        ids.insert(name, format!("rId{k}"));
    }
    // The sections' `w:sectPr`s in document order are the sections.
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let sects: Vec<(usize, bool, Range<usize>)> = dom
        .descendants()
        .filter(|n| is_w(*n, "sectPr") && n.parent().is_some_and(|p| is_w(p, "body") || is_w(p, "pPr")))
        .map(|n| {
            let r = n.range();
            let self_closing = xml[r.clone()].ends_with("/>");
            let at = if self_closing { r.end - 2 } else { r.start + xml[r.clone()].find('>').map_or(0, |i| i + 1) };
            (at, self_closing, r)
        })
        .collect();
    drop(dom);
    let mut inserts: Vec<(usize, bool, Range<usize>, String)> = Vec::new();
    for (k, s) in doc.sections.iter().enumerate() {
        let mut refs = String::new();
        for (footer, list) in [(false, &s.headers), (true, &s.footers)] {
            for (slot, r) in list.iter().enumerate() {
                if let Some(id) = r.as_ref().and_then(|n| ids.get(n)) {
                    let t = ["default", "first", "even"][slot];
                    let el = if footer { "footerReference" } else { "headerReference" };
                    refs.push_str(&format!(
                        r#"<{w}{el} {w}type="{t}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="{id}"/>"#
                    ));
                }
            }
        }
        if refs.is_empty() {
            continue;
        }
        let (at, self_closing, r) = sects.get(k).cloned().ok_or("a section without its sectPr")?;
        inserts.push((at, self_closing, r, refs));
    }
    inserts.sort_by_key(|i| std::cmp::Reverse(i.0));
    for (at, self_closing, r, refs) in inserts {
        if self_closing {
            // `<w:sectPr .../>` opened and closed around them.
            xml.replace_range(at..r.end, &format!(">{refs}</{w}sectPr>"));
        } else {
            xml.insert_str(at, &refs);
        }
    }
    for (n, b) in [(main, xml), (rels_name, rels), ("[Content_Types].xml".to_string(), types)] {
        match parts.iter_mut().find(|(m, _)| *m == n) {
            Some(p) => p.1 = b.into_bytes(),
            None => parts.push((n, b.into_bytes())),
        }
    }
    Ok(())
}

/// A new `footnotes.xml` or `endnotes.xml` holding `notes` (with Word's separator notes, ids -1 and 0), and the main
/// part's relationships and the content types with it added.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn new_notes_part(
    package: &[u8],
    main: &str,
    foot: bool,
    notes: &HashMap<i64, Vec<Block>>,
    made: &[(String, Vec<u8>)],
    people: &People,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    let (tag, file, rel) = if foot { ("footnote", "footnotes.xml", "footnotes") } else { ("endnote", "endnotes.xml", "endnotes") };
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let sep = |id: i64, kind: &str| {
        format!(
            r#"<w:{tag} w:type="{kind}" w:id="{id}"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:{kind}/></w:r></w:p></w:{tag}>"#
        )
    };
    let shell = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:{tag}s {ns}>{}{}</w:{tag}s>"#,
        sep(-1, "separator"),
        sep(0, "continuationSeparator")
    );
    let mut ids: Vec<&i64> = notes.keys().collect();
    ids.sort();
    let mut body = String::new();
    for id in ids {
        body.push_str(&format!(r#"<w:{tag} w:id="{id}">{}</w:{tag}>"#, new_blocks_xml(&shell, &notes[id], people)?));
    }
    let end = shell.rfind("</").ok_or("notes shell")?;
    let xml = format!("{}{body}{}", &shell[..end], &shell[end..]);
    // The part sits beside the main part; its relationship and content type go with it.
    let dir = main.rsplit_once('/').map_or("", |(d, _)| d);
    let name = if dir.is_empty() { file.to_string() } else { format!("{dir}/{file}") };
    let rels_name = match main.rsplit_once('/') {
        Some((d, f)) => format!("{d}/_rels/{f}.rels"),
        None => format!("_rels/{main}.rels"),
    };
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).map_err(|e| e.to_string())?;
    let mut read = |n: &str| -> Result<String, String> {
        if let Some((_, b)) = made.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map_err(|e| e.to_string());
        }
        let mut s = String::new();
        zin.by_name(n).map_err(|e| format!("{n}: {e}"))?.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    };
    let rels = read(&rels_name)?;
    let mut k = 1;
    while rels.contains(&format!("Id=\"rId{k}\"")) {
        k += 1;
    }
    let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
    let rels = format!(
        r#"{}<Relationship Id="rId{k}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{rel}" Target="{file}"/>{}"#,
        &rels[..close],
        &rels[close..]
    );
    let types = read("[Content_Types].xml")?;
    let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
    let types = format!(
        r#"{}<Override PartName="/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{rel}+xml"/>{}"#,
        &types[..close],
        &types[close..]
    );
    Ok(vec![(name, xml.into_bytes()), (rels_name, rels.into_bytes()), ("[Content_Types].xml".into(), types.into_bytes())])
}

/// A notes part's XML after the edits: its kept notes' edits, `added` notes at the end, `removed` ones taken out.
#[allow(clippy::too_many_arguments, reason = "the part, its note lists and the document state its rewrite reads")]
fn notes_xml(
    xml: &str,
    inner: &[(&[Block], &[Block])],
    lists: &[(&[Block], &[Block])],
    tag: &str,
    added: &[(&i64, &Vec<Block>)],
    removed: &[i64],
    controls: &HashMap<ControlId, Control>,
    gone: &[Control],
    people: &People,
) -> Result<Option<String>, String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let w = match root.lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let mut splices: Vec<Splice> = Vec::new();
    for n in root.children().filter(|n| is_w(*n, tag)) {
        if n.attribute((W, "id")).and_then(|v| v.parse::<i64>().ok()).is_some_and(|id| removed.contains(&id)) {
            splices.push((n.range(), String::new()));
        }
    }
    let mut text = String::new();
    for (id, blocks) in added {
        let body = new_blocks_xml(xml, blocks, people)?;
        text.push_str(&format!("<{w}{tag} {w}id=\"{id}\">{body}</{w}{tag}>"));
    }
    if !text.is_empty() {
        let r = root.range();
        let el = &xml[r.clone()];
        if let Some(open) = el.strip_suffix("/>") {
            // An empty, self-closing part root: opened and closed around the new notes.
            let name = el[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
            splices.push((r.clone(), format!("{open}>{text}</{name}>")));
        } else {
            let close = xml[..r.end].rfind("</").ok_or("a notes part without its end tag")?;
            splices.push((close..close, text));
        }
    }
    // One pass: the kept notes' edits and these whole-note splices never overlap (removed notes aren't in `lists`).
    part_xml_with(xml, inner, lists, false, &splices, controls, gone, people)
}

/// The controls read that the document no longer has (removed, or emptied of every member).
fn gone_controls(doc: &Document) -> Vec<Control> {
    doc.source.as_ref().map_or(Vec::new(), |s| s.controls.iter().filter(|(id, _)| !doc.controls.contains_key(id)).map(|(_, c)| c.clone()).collect())
}

/// Blocks with no source written from the model, for a part (new notes).
fn new_blocks_xml(xml: &str, blocks: &[Block], people: &People) -> Result<String, String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let mut s = Saver {
        xml,
        paras: HashMap::new(),
        tables: HashMap::new(),
        w,
        cx: Cx::of_xml(xml),
        force: false,
        splices: Vec::new(),
        nested: Vec::new(),
        float_src: HashMap::new(),
        controls: HashMap::new(),
        ids: std::cell::Cell::new(DRAWING_IDS),
        people,
    };
    s.blocks_xml(None, blocks)
}

/// The text boxes in `orig` with what they hold now (by float), innermost first.
fn boxes<'d>(orig: &[&'d [Block]], now: &[&'d [Block]]) -> Vec<(&'d [Block], &'d [Block])> {
    fn walk<'d>(blocks: &'d [Block], out: &mut Vec<(u64, &'d [Block])>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        if let Inline::Float(f, _) = i
                            && let Some(tb) = &f.text_box
                        {
                            out.push((f.id, &tb.blocks));
                            walk(&tb.blocks, out);
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let (mut o, mut n) = (Vec::new(), Vec::new());
    orig.iter().for_each(|b| walk(b, &mut o));
    now.iter().for_each(|b| walk(b, &mut n));
    o.into_iter().rev().filter_map(|(id, a)| Some((a, n.iter().find(|(m, _)| *m == id)?.1))).collect()
}

/// The main part's XML after the edits; `None` when nothing changed. `force` rewrites unchanged paragraphs too
/// (tests: the rewrite of an unedited paragraph must be the paragraph).
pub fn main_xml(doc: &Document, force: bool) -> Result<Option<String>, String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    if doc.sections.len() != src.blocks.len() {
        return Err("sections were added or removed (not supported yet)".into());
    }
    let lists: Vec<(&[Block], &[Block])> = src.blocks.iter().zip(&doc.sections).map(|(a, s)| (a.as_slice(), s.blocks.as_slice())).collect();
    let (o, n): (Vec<&[Block]>, Vec<&[Block]>) = lists.iter().copied().unzip();
    part_xml(&src.xml, &boxes(&o, &n), &lists, force, &doc.controls, &gone_controls(doc), &doc.people)
}

/// A part's XML with its block lists (as read, now) written back; `None` when none changed.
fn part_xml(
    xml: &str,
    inner: &[(&[Block], &[Block])],
    lists: &[(&[Block], &[Block])],
    force: bool,
    controls: &HashMap<ControlId, Control>,
    gone: &[Control],
    people: &People,
) -> Result<Option<String>, String> {
    part_xml_with(xml, inner, lists, force, &[], controls, gone, people)
}

/// [`part_xml`] with `extra` splices of the original part added (whole notes in and out).
#[allow(clippy::too_many_arguments, reason = "the part, its block lists and the document state its rewrite reads")]
fn part_xml_with<'a>(
    xml: &'a str,
    inner: &[(&[Block], &[Block])],
    lists: &[(&[Block], &[Block])],
    force: bool,
    extra: &[Splice],
    controls: &HashMap<ControlId, Control>,
    gone: &[Control],
    people: &'a People,
) -> Result<Option<String>, String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let paras = dom.descendants().filter(|n| is_w(*n, "p")).map(|n| (n.range().start, n)).collect();
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let tables = dom.descendants().filter(|n| is_w(*n, "tbl") || is_w(*n, "tr")).map(|n| (n.range().start, n)).collect();
    fn floats_of(blocks: &[Block], out: &mut HashMap<u64, (Range<usize>, Float)>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        if let Inline::Float(f, _) = i {
                            if let Some(r) = &f.src {
                                out.insert(f.id, (r.clone(), (**f).clone()));
                            }
                            if let Some(tb) = &f.text_box {
                                floats_of(&tb.blocks, out);
                            }
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| floats_of(&c.blocks, out)),
            }
        }
    }
    let mut float_src = HashMap::new();
    lists.iter().for_each(|(orig, _)| floats_of(orig, &mut float_src));
    let mut s = Saver {
        xml,
        paras,
        tables,
        w,
        cx: Cx::of_xml(xml),
        force,
        splices: Vec::new(),
        nested: Vec::new(),
        float_src,
        controls: controls.clone(),
        ids: std::cell::Cell::new(DRAWING_IDS),
        people,
    };
    for (orig, now) in inner {
        s.container(orig, now)?;
        // An inner text box's rewrite is applied where its outer box's source is copied.
        let made = std::mem::take(&mut s.splices);
        s.nested.extend(made);
        s.nested.sort_by_key(|(r, _)| (r.start, r.end));
    }
    for (orig, now) in lists {
        s.container(orig, now)?;
    }
    // Nested rewrites not inside a rewritten range are written on their own (the outermost of them).
    let outer: Vec<Range<usize>> = s.splices.iter().map(|(r, _)| r.clone()).collect();
    let free: Vec<Splice> = s.nested.iter().filter(|(r, _)| !outer.iter().any(|o| o.start <= r.start && r.end <= o.end)).cloned().collect();
    let top: Vec<Splice> = free.iter().filter(|(r, _)| !free.iter().any(|(o, _)| o != r && o.start <= r.start && r.end <= o.end)).cloned().collect();
    s.splices.extend(top);
    s.splices.extend(s.changed_controls(gone));
    s.splices.extend(extra.iter().cloned());
    if s.splices.is_empty() {
        return Ok(None);
    }
    s.splices.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = String::with_capacity(xml.len() + 1024);
    let mut at = 0;
    for (r, text) in &s.splices {
        if r.start < at {
            return Err("overlapping rewrites".into());
        }
        out.push_str(&xml[at..r.start]);
        out.push_str(text);
        at = r.end;
    }
    out.push_str(&xml[at..]);
    Ok(Some(out))
}

/// `package` with `parts` (name, content) replaced or added; every other entry is copied as stored.
pub fn repackage(package: &[u8], parts: &[(String, Vec<u8>)], method: zip::CompressionMethod) -> Result<Vec<u8>, String> {
    repackage_without(package, parts, &[], method)
}

/// [`repackage`], leaving the files `removed` out.
pub fn repackage_without(package: &[u8], parts: &[(String, Vec<u8>)], removed: &[String], method: zip::CompressionMethod) -> Result<Vec<u8>, String> {
    let err = |e: zip::result::ZipError| e.to_string();
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).map_err(err)?;
    let mut zout = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(method);
    let mut done = vec![false; parts.len()];
    for i in 0..zin.len() {
        let f = zin.by_index_raw(i).map_err(err)?;
        if removed.iter().any(|r| f.name().is_ok_and(|m| m == r.as_str())) {
            continue;
        }
        match parts.iter().position(|(n, _)| f.name().is_ok_and(|m| m == n.as_str())) {
            Some(k) => {
                drop(f);
                done[k] = true;
                zout.start_file(parts[k].0.as_str(), opts).map_err(err)?;
                zout.write_all(&parts[k].1).map_err(|e| e.to_string())?;
            }
            None => zout.raw_copy_file(f).map_err(err)?,
        }
    }
    for (k, (name, bytes)) in parts.iter().enumerate().filter(|(k, _)| !done[*k]) {
        let _ = k;
        zout.start_file(name.as_str(), opts).map_err(err)?;
        zout.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(zout.finish().map_err(err)?.into_inner())
}

type Splice = (Range<usize>, String);

struct Saver<'a, 'i> {
    xml: &'a str,
    /// Every `w:p` by its start offset.
    paras: HashMap<usize, Node<'a, 'i>>,
    /// Every `w:tbl` and `w:tr` by its start offset.
    tables: HashMap<usize, Node<'a, 'i>>,
    /// The WordprocessingML prefix with its colon (`w:`), for written elements.
    w: String,
    cx: Cx,
    force: bool,
    splices: Vec<Splice>,
    /// Rewrites inside other content (text boxes' paragraphs, made first): applied wherever that content's source is
    /// copied, and written on their own where it isn't rewritten.
    nested: Vec<Splice>,
    /// Every float of the part as read, by id, with its source drawing: a float re-anchored into another paragraph is
    /// written there from it.
    float_src: HashMap<u64, (Range<usize>, Float)>,
    /// The document's content controls: a rewritten table's rows are wrapped in theirs.
    controls: HashMap<ControlId, Control>,
    /// The next `wp:docPr/@id` for a new drawing.
    ids: std::cell::Cell<u32>,
    /// Who made the revisions written (their names as `w:author`).
    people: &'a People,
}

fn block_src(b: &Block) -> Option<*const ()> {
    match b {
        Block::Para(p) => p.src.as_ref().map(|s| Rc::as_ptr(s) as *const ()),
        Block::Table(t) => t.src.as_ref().map(|s| Rc::as_ptr(s) as *const ()),
    }
}

fn block_range(b: &Block) -> Option<Range<usize>> {
    match b {
        Block::Para(p) => p.src.as_ref().map(|s| s.ranges[0].start..s.ranges.last().unwrap().end),
        Block::Table(t) => t.src.as_deref().cloned(),
    }
}

impl<'a, 'i> Saver<'a, 'i> {
    /// Controls read from this part whose properties changed, around blocks, rows or cells whose source is kept: their
    /// start markup written from the properties (inline ones are written with their paragraph, rewritten for it, and
    /// a table written again wraps its rows from the properties).
    ///
    /// Controls read from this part that are `gone` lose their start and end markup, their content (what is left of
    /// it) kept in place; inline ones go with their rewritten paragraph.
    fn changed_controls(&self, gone: &[Control]) -> Vec<Splice> {
        let mut out = Vec::new();
        let inline = |at: usize| self.paras.values().any(|n| n.range().start < at && at < n.range().end);
        let free = |r: &Range<usize>| !self.splices.iter().any(|(x, _)| x.start < r.end && r.start < x.end);
        for c in gone {
            let Some((a, b)) = c.at else { continue };
            let (open, close) = (a..a + c.open.len(), b..b + c.close.len());
            if self.xml.get(open.clone()) == Some(c.open.as_str())
                && self.xml.get(close.clone()) == Some(c.close.as_str())
                && !inline(a)
                && free(&open)
                && free(&close)
            {
                out.push((open, String::new()));
                out.push((close, String::new()));
            }
        }
        for (id, c) in &self.controls {
            let Some((at, _)) = c.at else { continue };
            let r = at..at + c.open.len();
            if c.as_read() || self.xml.get(r.clone()) != Some(c.open.as_str()) {
                continue;
            }
            let inline = self.paras.values().any(|n| n.range().start < at && at < n.range().end);
            if inline || self.splices.iter().any(|(x, _)| x.start < r.end && r.start < x.end) {
                continue;
            }
            out.push((r, control_open(&self.w, *id, c)));
        }
        out
    }

    /// Whether `p` holds an inline control whose properties changed (its paragraph is written again for it).
    fn changed_inline(&self, p: &Paragraph) -> bool {
        p.inlines.iter().any(|i| match i {
            Inline::ControlStart(c) | Inline::Sym { control: Some(c), .. } => self.controls.get(c).is_some_and(|k| !k.as_read()),
            _ => false,
        })
    }

    /// One block list: as read (`orig`) and now (`cur`).
    fn container(&mut self, orig: &[Block], cur: &[Block]) -> Result<(), String> {
        let index: HashMap<*const (), usize> = orig.iter().enumerate().filter_map(|(i, b)| Some((block_src(b)?, i))).collect();
        // Anchors (orig index, cur index); edits never reorder blocks, so they increase together.
        let mut starts: Vec<(usize, usize)> = Vec::new();
        // Paragraphs sharing a block control made since reading are one group, so one wrapper goes around them all.
        // shortcut: a table among them still starts a group of its own (the control is written once per group).
        let new_ctl = |b: &Block| -> Vec<ControlId> {
            match b {
                Block::Para(p) => p.controls.iter().copied().filter(|c| self.controls.get(c).is_some_and(|k| k.open.is_empty())).collect(),
                Block::Table(_) => Vec::new(),
            }
        };
        for (j, b) in cur.iter().enumerate() {
            if j > 0 && matches!(b, Block::Para(_)) && new_ctl(b).iter().any(|c| new_ctl(&cur[j - 1]).contains(c)) {
                continue;
            }
            if let Some(&i) = block_src(b).and_then(|k| index.get(&k))
                && starts.last().is_none_or(|&(l, _)| i > l)
            {
                starts.push((i, j));
            }
        }
        if starts.first() != Some(&(0, 0)) {
            starts.insert(0, (0, 0));
        }
        for g in 0..starts.len() {
            let (i0, j0) = starts[g];
            let (i1, j1) = starts.get(g + 1).copied().unwrap_or((orig.len(), cur.len()));
            // Where new blocks go when the group has no source paragraph: before the next block as read.
            let at = orig.get(i1).and_then(block_range).map(|r| r.start).or_else(|| orig.last().and_then(block_range).map(|r| r.end));
            self.group(&orig[i0..i1], &cur[j0..j1], at)?;
        }
        Ok(())
    }

    fn group(&mut self, q: &[Block], p: &[Block], at: Option<usize>) -> Result<(), String> {
        if let (Some(Block::Table(a)), Some(Block::Table(b))) = (q.first(), p.first())
            && block_src(&q[0]) == block_src(&p[0])
        {
            self.table(a, b)?;
            return self.paragraphs(&q[1..], &p[1..], a.src.as_ref().map(|r| r.end));
        }
        self.paragraphs(q, p, at)
    }

    fn table(&mut self, a: &Table, b: &Table) -> Result<(), String> {
        let same_rows = a.rows.len() == b.rows.len()
            && a.rows.iter().zip(&b.rows).all(|(x, y)| {
                x.cells.len() == y.cells.len()
                    && x.controls == y.controls
                    && x.cells.iter().zip(&y.cells).all(|(c, d)| c.controls == d.controls)
                    && same_src(&x.src, &y.src)
                    && x.height == y.height
                    && x.header == y.header
                    && (x.grid_before, x.grid_after) == (y.grid_before, y.grid_after)
                    && x.rev == y.rev
                    && x.cells
                        .iter()
                        .zip(&y.cells)
                        .all(|(c, d)| c.width == d.width && c.span == d.span && c.direct.0 == d.direct.0 && c.v_merge == d.v_merge && c.rev == d.rev)
            })
            && a.grid == b.grid
            && (&a.change, &a.grid_change) == (&b.change, &b.grid_change)
            && a.width == b.width
            && (&a.style_id, a.look, a.jc, a.ind) == (&b.style_id, b.look, b.jc, b.ind);
        if !same_rows {
            // Rows or cells came or went: the table is written again, its kept rows from their source.
            let range = (**a.src.as_ref().ok_or("a table without its source")?).clone();
            let text = self.table_xml(Some(a), b)?;
            self.splices.push((range, text));
            return Ok(());
        }
        for (x, y) in a.rows.iter().zip(&b.rows) {
            for (c, d) in x.cells.iter().zip(&y.cells) {
                self.container(&c.blocks, &d.blocks)?;
            }
        }
        Ok(())
    }

    /// A paragraph group: the paragraphs as read (`q`, tables among them were deleted) and now (`p`).
    fn paragraphs(&mut self, q: &[Block], p: &[Block], at: Option<usize>) -> Result<(), String> {
        let mut qs = Vec::new();
        let mut at = at;
        for b in q {
            match b {
                Block::Para(x) => qs.push(x),
                Block::Table(t) => {
                    let r = (**t.src.as_ref().ok_or("a table without its source")?).clone();
                    at = Some(at.map_or(r.start, |a| a.min(r.start)));
                    self.splices.push((r, String::new()));
                }
            }
        }
        if p.iter().any(|b| matches!(b, Block::Table(_))) {
            return self.with_new_tables(&qs, p, at);
        }
        let ps: Vec<&Paragraph> = p.iter().filter_map(|b| if let Block::Para(x) = b { Some(x) } else { None }).collect();
        if qs.is_empty() && ps.is_empty() {
            return Ok(());
        }
        if !self.force && qs.len() == 1 && ps.len() == 1 && same_para(qs[0], ps[0]) && qs[0].controls == ps[0].controls && !self.changed_inline(ps[0])
        {
            return Ok(());
        }
        // A deleted frame (frames.rs made its paragraphs a float of the next one): its paragraphs go too.
        let kept: Vec<u64> =
            ps.iter().flat_map(|p| p.inlines.iter()).filter_map(|i| if let Inline::Float(f, _) = i { Some(f.id) } else { None }).collect();
        for q in &qs {
            for i in &q.inlines {
                if let Inline::Float(f, _) = i
                    && let Some(r) = &f.frame_src
                    && !kept.contains(&f.id)
                {
                    self.splices.push((r.clone(), String::new()));
                }
            }
        }
        let srcs: Vec<&ParaSrc> = qs.iter().map(|x| x.src.as_deref().ok_or("a paragraph without its source")).collect::<Result<_, _>>()?;
        if ps.is_empty() {
            // Every paragraph of the group went (with the blocks around them).
            for r in srcs.iter().flat_map(|s| s.ranges.iter()) {
                self.splices.push((r.clone(), String::new()));
            }
            return Ok(());
        }
        let mut text = Emit::group(self, &qs, &srcs, &ps)?;
        match srcs.first() {
            Some(first) => {
                // Frames between joined paragraphs (frames.rs anchored them in the next paragraph, now joined into
                // this group's output) go before the output, or they would frame whatever follows.
                // shortcut: before the group, not before the output paragraph that now holds the frame.
                let span = first.ranges[0].start..srcs.last().unwrap().ranges.last().unwrap().end;
                let parent = self.paras.get(&span.start).and_then(|n| n.parent()).map(|n| n.id());
                let mut frames: Vec<Node> = self
                    .paras
                    .values()
                    .filter(|n| span.contains(&n.range().start) && n.parent().map(|n| n.id()) == parent)
                    .filter(|n| !srcs.iter().any(|s| s.ranges.iter().any(|r| r.start == n.range().start)))
                    .filter(|n| child(**n, "pPr").and_then(|p| child(p, "framePr")).is_some())
                    .copied()
                    .collect();
                frames.sort_by_key(|n| n.range().start);
                let moved: String = frames.iter().map(|n| &self.xml[n.range()]).collect();
                for n in &frames {
                    self.splices.push((n.range(), String::new()));
                }
                text.insert_str(0, &moved);
                self.splices.push((first.ranges[0].clone(), text));
                for r in srcs.iter().flat_map(|s| s.ranges.iter()).skip(1) {
                    self.splices.push((r.clone(), String::new()));
                }
            }
            None => {
                let at = at.ok_or("nowhere to put new paragraphs")?;
                self.splices.push((at..at, text));
            }
        }
        Ok(())
    }

    /// A paragraph group with new tables among its paragraphs: each run of paragraphs is written with the source
    /// paragraphs it continues (deleted ones with the first run), each table from the model.
    // shortcut: a section break in the group goes with the run its source paragraph went to, which may not be the last.
    fn with_new_tables(&mut self, qs: &[&Paragraph], p: &[Block], at: Option<usize>) -> Result<(), String> {
        let mut runs: Vec<Vec<&Paragraph>> = vec![Vec::new()];
        for b in p {
            match b {
                Block::Para(x) => runs.last_mut().expect("a run").push(x),
                Block::Table(_) => runs.push(Vec::new()),
            }
        }
        let run_of = |q: &Paragraph| {
            runs.iter().position(|r| r.iter().any(|x| x.src.as_ref().zip(q.src.as_ref()).is_some_and(|(a, b)| Rc::ptr_eq(a, b)))).unwrap_or(0)
        };
        let mut text = String::new();
        let mut k = 0;
        for b in p.iter().map(Some).chain(std::iter::once(None)) {
            match b {
                Some(Block::Para(_)) => continue,
                _ => {
                    let mine: Vec<&Paragraph> = qs.iter().copied().filter(|q| run_of(q) == k).collect();
                    let srcs: Vec<&ParaSrc> =
                        mine.iter().map(|x| x.src.as_deref().ok_or("a paragraph without its source")).collect::<Result<_, _>>()?;
                    if !runs[k].is_empty() || !mine.is_empty() {
                        text.push_str(&emit(self, &mine, &srcs, &runs[k])?);
                    }
                    k += 1;
                }
            }
            if let Some(Block::Table(t)) = b {
                // shortcut: a new table's new block controls wrap it alone, not the paragraphs around it with it.
                let new: Vec<(ControlId, &Control)> =
                    t.controls.iter().filter_map(|c| self.controls.get(c).filter(|k| k.open.is_empty()).map(|k| (*c, k))).collect();
                let opens: String = new.iter().map(|(id, c)| control_open(&self.w, *id, c)).collect();
                let closes: String = new.iter().rev().map(|(_, c)| control_close(&self.w, c)).collect();
                text.push_str(&opens);
                text.push_str(&self.table_xml(None, t)?);
                text.push_str(&closes);
            }
        }
        let mut ranges = qs.iter().filter_map(|q| q.src.as_ref()).flat_map(|s| s.ranges.iter().cloned());
        match ranges.next() {
            Some(first) => {
                self.splices.push((first, text));
                for r in ranges {
                    self.splices.push((r, String::new()));
                }
            }
            None => {
                let at = at.ok_or("nowhere to put new blocks")?;
                self.splices.push((at..at, text));
            }
        }
        Ok(())
    }

    /// A table written from the model: as `orig` (its source) with rows added, removed, merged or split, or new. Kept
    /// rows keep their XML with their cells' edits spliced in; other rows and cells are written from the model, their
    /// paragraphs from their sources where they have one.
    fn table_xml(&mut self, orig: Option<&Table>, t: &Table) -> Result<String, String> {
        let w = self.w.clone();
        let node = orig.and_then(|a| a.src.as_ref()).and_then(|r| self.tables.get(&r.start).copied());
        let mut out = String::new();
        let close = match node {
            Some(n) => {
                let (open, close) = self.tags(n);
                out.push_str(&open);
                out.push_str(&match (child(n, "tblPr"), orig) {
                    // The table's own properties that changed (a border drag's width, Table Properties, Table Design),
                    // patched into its source properties in schema order.
                    (Some(p), Some(a)) => {
                        const ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
                        let before = |name: &str| &ORDER[..ORDER.iter().position(|n| *n == name).unwrap_or(0)];
                        let mut fix = Vec::new();
                        if a.style_id != t.style_id {
                            let tag = t.style_id.as_ref().map_or(String::new(), |id| format!("<{w}tblStyle {w}val=\"{}\"/>", props::esc(id)));
                            fix.push(self.set_child(p, "tblStyle", tag, before("tblStyle")));
                        }
                        if a.width != t.width {
                            fix.push(self.set_child(p, "tblW", width(&w, "tblW", &t.width), before("tblW")));
                        }
                        if a.jc != t.jc {
                            let v = match t.jc {
                                Jc::Center => "center",
                                Jc::Right => "right",
                                _ => "left",
                            };
                            fix.push(self.set_child(p, "jc", format!("<{w}jc {w}val=\"{v}\"/>"), before("jc")));
                        }
                        if a.ind != t.ind {
                            fix.push(self.set_child(p, "tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", twips(t.ind)), before("tblInd")));
                        }
                        if a.look != t.look
                            && let Some(l) = t.look
                        {
                            let bits = [(l.first_row, 0x20), (l.last_row, 0x40), (l.first_col, 0x80), (l.last_col, 0x100), (l.no_h_band, 0x200), (l.no_v_band, 0x400)];
                            let val: u32 = bits.iter().filter(|(on, _)| *on).map(|(_, b)| b).sum();
                            let on = |b: bool| if b { "1" } else { "0" };
                            let tag = format!(
                                "<{w}tblLook {w}val=\"{val:04X}\" {w}firstRow=\"{}\" {w}lastRow=\"{}\" {w}firstColumn=\"{}\" {w}lastColumn=\"{}\" {w}noHBand=\"{}\" {w}noVBand=\"{}\"/>",
                                on(l.first_row),
                                on(l.last_row),
                                on(l.first_col),
                                on(l.last_col),
                                on(l.no_h_band),
                                on(l.no_v_band)
                            );
                            fix.push(self.set_child(p, "tblLook", tag, before("tblLook")));
                        }
                        if a.change != t.change {
                            let tag = t.change.as_ref().map_or(String::new(), |c| revisions::table_change(&w, "tblPrChange", "tblPr", c, self.people));
                            fix.push(self.set_child(p, "tblPrChange", tag, before("tblPrChange")));
                        }
                        if fix.is_empty() { self.xml[p.range()].to_string() } else { self.spliced(p.range(), fix)? }
                    }
                    (Some(p), None) => self.xml[p.range()].to_string(),
                    (None, _) => tbl_pr(&w, t, self.people),
                });
                close
            }
            None => {
                out.push_str(&format!("<{w}tbl>"));
                out.push_str(&tbl_pr(&w, t, self.people));
                format!("</{w}tbl>")
            }
        };
        out.push_str(&format!("<{w}tblGrid>"));
        for g in &t.grid {
            out.push_str(&format!("<{w}gridCol {w}w=\"{}\"/>", twips(*g)));
        }
        if let Some(g) = &t.grid_change {
            out.push_str(&revisions::grid_change(&w, g, self.people));
        }
        out.push_str(&format!("</{w}tblGrid>"));
        let old_rows: Vec<&Row> = orig.map(|a| a.rows.iter().collect()).unwrap_or_default();
        // Row- and cell-level controls (repeating sections and their items): opened and closed around maximal runs
        // of the rows (or a row's cells) they wrap.
        let mut open: Vec<ControlId> = Vec::new();
        let wrap = |out: &mut String, want: &[ControlId], open: &mut Vec<ControlId>, s: &Self| {
            let keep = open.iter().zip(want).take_while(|(a, b)| a == b).count();
            while open.len() > keep {
                if let Some(c) = open.pop().and_then(|id| s.controls.get(&id)) {
                    out.push_str(&control_close(&s.w, c));
                }
            }
            for id in &want[keep..] {
                if let Some(c) = s.controls.get(id) {
                    out.push_str(&control_open(&s.w, *id, c));
                    open.push(*id);
                }
            }
        };
        for row in &t.rows {
            wrap(&mut out, &row.controls, &mut open, self);
            let kept = row.src.as_ref().and_then(|r| old_rows.iter().find(|o| o.src.as_ref().is_some_and(|s| Rc::ptr_eq(s, r))).copied());
            let row_node = kept.and_then(|k| k.src.as_ref()).and_then(|r| self.tables.get(&r.start).copied());
            match (kept, row_node) {
                (Some(k), Some(n)) if k.cells.len() == row.cells.len() => {
                    let outer = std::mem::take(&mut self.splices);
                    for (c, d) in k.cells.iter().zip(&row.cells) {
                        self.container(&c.blocks, &d.blocks)?;
                    }
                    let mut inner = std::mem::replace(&mut self.splices, outer);
                    inner.extend(self.row_fixes(n, k, row));
                    out.push_str(&self.spliced(n.range(), inner)?);
                }
                (_, n) => {
                    let (open, close) = match n {
                        Some(n) => self.tags(n),
                        None => (format!("<{w}tr>"), format!("</{w}tr>")),
                    };
                    out.push_str(&open);
                    out.push_str(&self.row_props(n, kept, row));
                    let mut cells_open = Vec::new();
                    for c in &row.cells {
                        wrap(&mut out, &c.controls, &mut cells_open, self);
                        out.push_str(&format!("<{w}tc>{}", tc_pr(&w, c, self.people)));
                        out.push_str(&self.blocks_xml(orig, &c.blocks)?);
                        out.push_str(&format!("</{w}tc>"));
                    }
                    wrap(&mut out, &[], &mut cells_open, self);
                    out.push_str(&close);
                }
            }
        }
        wrap(&mut out, &[], &mut open, self);
        out.push_str(&close);
        Ok(out)
    }

    /// Blocks written from the model: paragraphs from their source paragraph in `orig` when they have one, tables
    /// as [`Saver::table_xml`].
    fn blocks_xml(&mut self, orig: Option<&Table>, blocks: &[Block]) -> Result<String, String> {
        fn find<'t>(blocks: &'t [Block], src: &Rc<ParaSrc>) -> Option<&'t Paragraph> {
            blocks.iter().find_map(|b| match b {
                Block::Para(p) => p.src.as_ref().is_some_and(|s| Rc::ptr_eq(s, src)).then_some(p),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| find(&c.blocks, src)),
            })
        }
        fn find_table<'t>(blocks: &'t [Block], src: &Rc<Range<usize>>) -> Option<&'t Table> {
            blocks.iter().find_map(|b| match b {
                Block::Para(_) => None,
                Block::Table(t) if t.src.as_ref().is_some_and(|s| Rc::ptr_eq(s, src)) => Some(t),
                Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| find_table(&c.blocks, src)),
            })
        }
        let scope: Vec<Block> = orig.map(|a| vec![Block::Table(a.clone())]).unwrap_or_default();
        let mut out = String::new();
        let mut new: Vec<&Paragraph> = Vec::new();
        for b in blocks {
            match b {
                Block::Para(p) => match p.src.as_ref().and_then(|s| find(&scope, s)) {
                    Some(q) => {
                        out.push_str(&emit(self, &[], &[], &std::mem::take(&mut new))?);
                        let src = q.src.as_deref().ok_or("a paragraph without its source")?;
                        out.push_str(&emit(self, &[q], &[src], &[p])?);
                    }
                    None => new.push(p),
                },
                Block::Table(t) => {
                    out.push_str(&emit(self, &[], &[], &std::mem::take(&mut new))?);
                    let o = t.src.as_ref().and_then(|s| find_table(&scope, s)).cloned();
                    out.push_str(&self.table_xml(o.as_ref(), t)?);
                }
            }
        }
        out.push_str(&emit(self, &[], &[], &new)?);
        Ok(out)
    }

    /// A rewritten row's `w:tblPrEx` and `w:trPr`: its source's (`n`, read as `was`) with its revisions as the model
    /// has them now, else written from the model.
    fn row_props(&self, n: Option<Node>, was: Option<&Row>, row: &Row) -> String {
        let w = &self.w;
        let same = was.is_some_and(|k| k.rev == row.rev);
        let src = |name: &str| n.and_then(|n| child(n, name)).map(|p| &self.xml[p.range()]);
        let mut out = String::new();
        match src("tblPrEx") {
            Some(x) if same => out.push_str(x),
            x => {
                let mut p = Props::parse(x, "tblPrEx", &self.cx);
                p.reorder(revisions::TBLPREX);
                p.remove("tblPrExChange");
                let change = revisions::ex_change(w, &row.rev, self.people);
                if !change.is_empty() {
                    p.set("tblPrExChange", change);
                }
                out.push_str(&p.xml().unwrap_or_default());
            }
        }
        match src("trPr") {
            Some(x) if same => out.push_str(x),
            Some(x) => {
                let mut p = Props::parse(Some(x), "trPr", &self.cx);
                p.reorder(revisions::TRPR);
                for (name, tag) in revisions::row_marks(w, &row.rev, self.people) {
                    p.remove(name);
                    if !tag.is_empty() {
                        p.set(name, tag);
                    }
                }
                out.push_str(&p.xml().unwrap_or_default());
            }
            None => out.push_str(&tr_pr(w, row, self.people)),
        }
        out
    }

    /// A kept row's height and its cells' widths changed by a border drag, and its and its cells' revisions, patched
    /// into its source properties.
    fn row_fixes(&self, n: Node, was: &Row, now: &Row) -> Vec<Splice> {
        let w = self.w.clone();
        let mut out = Vec::new();
        // The table exceptions' tracked change.
        if was.rev.ex_change != now.rev.ex_change {
            let tag = revisions::ex_change(&w, &now.rev, self.people);
            match child(n, "tblPrEx") {
                Some(p) => out.push(self.set_child(p, "tblPrExChange", tag, revisions::before(revisions::TBLPREX, "tblPrExChange"))),
                None if !tag.is_empty() => {
                    let at = self.open_end(n);
                    out.push((at..at, format!("<{w}tblPrEx>{tag}</{w}tblPrEx>")));
                }
                None => {}
            }
        }
        // Grid gaps before and after the cells (a border moved in some rows only).
        const BASE: &[&str] = &["cnfStyle", "divId"];
        let mut tr: Vec<(&str, String, &[&str])> = Vec::new();
        for (name, a, b) in [("gridBefore", was.grid_before, now.grid_before), ("gridAfter", was.grid_after, now.grid_after)] {
            if a != b {
                tr.push((name, if b > 0 { format!("<{w}{name} {w}val=\"{b}\"/>") } else { String::new() }, BASE));
            }
        }
        if was.cant_split != now.cant_split {
            tr.push(("cantSplit", if now.cant_split { format!("<{w}cantSplit/>") } else { format!("<{w}cantSplit {w}val=\"0\"/>") }, BASE));
        }
        if was.header != now.header {
            tr.push(("tblHeader", if now.header { format!("<{w}tblHeader/>") } else { format!("<{w}tblHeader {w}val=\"0\"/>") }, BASE));
        }
        if was.height != now.height {
            let tag = match &now.height {
                Some((h, rule)) => {
                    let rule = if rule == "atLeast" { String::new() } else { format!(" {w}hRule=\"{rule}\"") };
                    format!("<{w}trHeight {w}val=\"{}\"{rule}/>", twips(*h))
                }
                None => String::new(),
            };
            tr.push(("trHeight", tag, BASE));
        }
        // The row's insertion, deletion and property change, after its base properties.
        if was.rev != now.rev {
            for (name, tag) in revisions::row_marks(&w, &now.rev, self.people) {
                tr.push((name, tag, revisions::before(revisions::TRPR, name)));
            }
        }
        // One `w:trPr` (its base children are a choice group: any order).
        if !tr.is_empty() {
            match child(n, "trPr") {
                Some(p) => out.extend(self.set_children(p, tr)),
                None => {
                    let at = child(n, "tblPrEx").map_or_else(|| self.open_end(n), |x| x.range().end);
                    let kids: String = tr.into_iter().map(|(_, t, _)| t).collect();
                    if !kids.is_empty() {
                        out.push((at..at, format!("<{w}trPr>{kids}</{w}trPr>")));
                    }
                }
            }
        }
        let tcs: Vec<Node> = n.children().filter(|c| is_w(*c, "tc")).collect();
        if tcs.len() == now.cells.len() {
            for ((tc, a), b) in tcs.iter().zip(&was.cells).zip(&now.cells) {
                let mut fix: Vec<(&str, String, &[&str])> = Vec::new();
                if a.width != b.width {
                    fix.push(("tcW", b.width.as_ref().map_or(String::new(), |v| width(&w, "tcW", v)), &["cnfStyle"]));
                }
                if a.span.max(1) != b.span.max(1) {
                    let tag = if b.span > 1 { format!("<{w}gridSpan {w}val=\"{}\"/>", b.span) } else { String::new() };
                    fix.push(("gridSpan", tag, &["cnfStyle", "tcW"]));
                }
                if a.v_merge != b.v_merge {
                    let tag = match b.v_merge {
                        VMerge::Restart => format!("<{w}vMerge {w}val=\"restart\"/>"),
                        VMerge::Continue => format!("<{w}vMerge/>"),
                        VMerge::None => String::new(),
                    };
                    fix.push(("vMerge", tag, &["cnfStyle", "tcW", "gridSpan", "hMerge"]));
                }
                // Shading and vertical alignment that table formatting set as direct properties (resolved values that
                // a style region gives aren't written).
                let (da, db) = (Props::parse(a.direct.0.as_deref(), "tcPr", &self.cx), Props::parse(b.direct.0.as_deref(), "tcPr", &self.cx));
                for (name, after) in [
                    ("tcBorders", &["cnfStyle", "tcW", "gridSpan", "hMerge", "vMerge"][..]),
                    ("shd", &["cnfStyle", "tcW", "gridSpan", "hMerge", "vMerge", "tcBorders"][..]),
                    ("tcMar", &["cnfStyle", "tcW", "gridSpan", "hMerge", "vMerge", "tcBorders", "shd", "noWrap"][..]),
                    (
                        "vAlign",
                        &["cnfStyle", "tcW", "gridSpan", "hMerge", "vMerge", "tcBorders", "shd", "noWrap", "tcMar", "textDirection", "tcFitText"][..],
                    ),
                ] {
                    if da.get(name) != db.get(name) {
                        fix.push((name, db.get(name).unwrap_or("").to_string(), after));
                    }
                }
                // The cell's insertion, deletion or merge, and its property change.
                if a.rev != b.rev {
                    for (name, tag) in revisions::cell_marks(&w, &b.rev, self.people) {
                        fix.push((name, tag, revisions::before(revisions::TCPR, name)));
                    }
                }
                if fix.is_empty() {
                    continue;
                }
                match child(*tc, "tcPr") {
                    // Each fix on the source properties as they are (two inserts at one place keep their order).
                    Some(p) => out.extend(self.set_children(p, fix)),
                    None => {
                        let at = self.open_end(*tc);
                        out.push((at..at, format!("<{w}tcPr>{}</{w}tcPr>", fix.into_iter().map(|(_, t, _)| t).collect::<String>())));
                    }
                }
            }
        }
        out
    }

    /// Floats of `p` that came from another paragraph (re-anchored): their source drawings, placed as they
    /// are now, in runs at the paragraph's start.
    fn moved_in(&self, p: &Paragraph, qs: &[&Paragraph]) -> String {
        let here = |id: u64| qs.iter().flat_map(|q| q.inlines.iter()).any(|i| matches!(i, Inline::Float(f, _) if f.id == id));
        let mut out = String::new();
        for i in &p.inlines {
            let Inline::Float(f, _) = i else { continue };
            if here(f.id) {
                continue;
            }
            let Some((r, was)) = self.float_src.get(&f.id) else {
                if let Some(x) = self.new_float_xml(f) {
                    out.push_str(&format!("<{w}r>{x}</{w}r>", w = self.w));
                }
                continue;
            };
            let src = self.src_text(r.clone());
            let d = match Placement::changed(was, f) {
                Some(pl) => pl.apply(&src),
                None => src,
            };
            out.push_str(&format!("<{w}r>{d}</{w}r>", w = self.w));
        }
        out
    }

    /// Where an element's start tag ends.
    fn open_end(&self, n: Node) -> usize {
        let r = n.range();
        r.start + self.xml[r.clone()].find('>').map_or(0, |i| i + 1)
    }

    /// `tag` in place of `parent`'s child `name`, or inserted after the last of `after` (the children the schema puts
    /// first), else first.
    fn set_child(&self, parent: Node, name: &str, tag: String, after: &[&str]) -> Splice {
        if let Some(c) = child(parent, name) {
            return (c.range(), tag);
        }
        let r = parent.range();
        if self.xml[r.clone()].ends_with("/>") {
            // A self-closing parent: opened and closed around the new child.
            let (open, close) = self.tags(parent);
            return (r, format!("{open}{tag}{close}"));
        }
        let at =
            parent.children().rfind(|c| c.is_element() && after.iter().any(|a| is_w(*c, a))).map_or_else(|| self.open_end(parent), |c| c.range().end);
        (at..at, tag)
    }

    /// [`Saver::set_child`] for each of `fixes` (name, tag, after; an empty tag removes the child): one rewrite of a
    /// self-closing parent with all of them, in their order.
    fn set_children(&self, parent: Node, fixes: Vec<(&str, String, &[&str])>) -> Vec<Splice> {
        let fixes: Vec<_> = fixes.into_iter().filter(|(name, tag, _)| !tag.is_empty() || child(parent, name).is_some()).collect();
        let r = parent.range();
        if self.xml[r.clone()].ends_with("/>") {
            if fixes.is_empty() {
                return Vec::new();
            }
            let (open, close) = self.tags(parent);
            let kids: String = fixes.into_iter().map(|(_, t, _)| t).collect();
            return vec![(r, format!("{open}{kids}{close}"))];
        }
        fixes.into_iter().map(|(name, tag, after)| self.set_child(parent, name, tag, after)).collect()
    }

    /// The source text of `range` with `splices` (inside it) applied.
    fn spliced(&self, range: Range<usize>, mut splices: Vec<Splice>) -> Result<String, String> {
        splices.sort_by_key(|(r, _)| (r.start, r.end));
        let mut out = String::new();
        let mut at = range.start;
        for (r, text) in &splices {
            if r.start < at || r.end > range.end {
                return Err("overlapping rewrites".into());
            }
            out.push_str(&self.src_text(at..r.start));
            out.push_str(text);
            at = r.end;
        }
        out.push_str(&self.src_text(at..range.end));
        Ok(out)
    }

    /// Source bytes with the nested rewrites inside them applied (the outermost of those inside).
    fn src_text(&self, r: Range<usize>) -> String {
        let mut out = String::new();
        let mut at = r.start;
        for (n, text) in self.nested.iter().filter(|(n, _)| r.start <= n.start && n.end <= r.end) {
            if n.start < at {
                continue; // inside one already applied
            }
            out.push_str(&self.xml[at..n.start]);
            out.push_str(text);
            at = n.end;
        }
        out.push_str(&self.xml[at..r.end]);
        out
    }

    /// An element's start tag (a self-closing one opened) and its end tag.
    fn tags(&self, n: Node) -> (String, String) {
        let r = n.range();
        let all = &self.xml[r.clone()];
        let name = all[1..].split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or("");
        let close = format!("</{name}>");
        let open = match n.first_child() {
            Some(c) => self.xml[r.start..c.range().start].to_string(),
            None if all.ends_with("/>") => format!("{}>", all[..all.len() - 2].trim_end()),
            None => all[..all.len() - close.len()].to_string(),
        };
        (open, close)
    }

    /// A paragraph's `w:pPr` after formatting edits: the source's (`node`) or its own direct properties with its
    /// tracked change, its mark's formatting and revisions when that changed (`mark`), and `sect` as its section break.
    fn edited_ppr(&self, node: Option<Node>, p: &Paragraph, mark: bool, sect: Option<&str>) -> String {
        let base = match node {
            Some(n) => child(n, "pPr").map(|x| self.xml[x.range()].to_string()),
            None => p.style.direct.0.as_deref().map(str::to_string),
        };
        let mut pr = Props::parse(base.as_deref(), "pPr", &self.cx);
        if mark {
            let rpr = if p.mark.rev.is_empty() {
                p.mark.direct.0.as_deref().map(str::to_string)
            } else {
                revisions::mark_rpr(&p.mark, &self.cx, self.people)
            };
            match rpr {
                Some(r) => pr.set("rPr", r),
                None => pr.remove("rPr"),
            }
        }
        if node.is_none() {
            // Its own properties (not its source's): its tracked change is the model's.
            pr.remove("pPrChange");
            if let Some(c) = &p.style.change {
                pr.set("pPrChange", revisions::change(&self.w, "pPrChange", "pPr", &c.mark, &c.previous_raw, self.people));
                if let (Some(nc), Some(num)) = (c.numbering_change.0.as_deref(), pr.get("numPr")) {
                    let mut n = Props::parse(Some(num), "numPr", &self.cx);
                    n.set("numberingChange", nc.to_string());
                    if let Some(x) = n.xml() {
                        pr.set("numPr", x);
                    }
                }
            }
        }
        pr.remove("sectPr");
        if let Some(s) = sect {
            pr.set("sectPr", s.to_string());
        }
        pr.xml().unwrap_or_default()
    }

    /// The `w:pPr` of a source paragraph with `sect` as its section break (none: without one).
    fn ppr(&self, para: Node, sect: Option<&str>) -> String {
        let w = &self.w;
        let Some(ppr) = child(para, "pPr") else {
            return sect.map_or(String::new(), |s| format!("<{w}pPr>{s}</{w}pPr>"));
        };
        let base = ppr.range().start;
        let own = child(ppr, "sectPr");
        if own.map(|s| &self.xml[s.range()]) == sect {
            return self.xml[ppr.range()].to_string();
        }
        let (open, close) = self.tags(ppr);
        let mut inner = match ppr.first_child() {
            Some(c) => self.xml[c.range().start..ppr.last_child().unwrap().range().end].to_string(),
            None => String::new(),
        };
        let inner_base = ppr.first_child().map_or(base, |c| c.range().start);
        if let Some(s) = own {
            inner.replace_range(s.range().start - inner_base..s.range().end - inner_base, "");
        }
        if let Some(s) = sect {
            // CT_PPr: the section break comes before a tracked property change.
            let at = ppr.children().find(|c| is_w(*c, "pPrChange")).map_or(inner.len(), |c| {
                let cut = own.filter(|o| o.range().start < c.range().start).map_or(0, |o| o.range().len());
                c.range().start - inner_base - cut
            });
            inner.insert_str(at, s);
        }
        format!("{open}{inner}{close}")
    }
}

/// Content with no units that sits before a unit and is written with it when a group is written from the model.
#[derive(Clone, Debug, PartialEq)]
enum Marker {
    /// An inline control's start (`true`) or end.
    Control(ControlId, bool),
    /// A comment's or a move's range start or end.
    Range(RangeMark),
    /// A comment's reference mark, in its run's formatting.
    Comment(CommentId, Run),
}

impl Marker {
    /// The element it is in the source, and the id that names it there.
    fn element(&self) -> Option<(&'static str, i64)> {
        match self {
            Marker::Control(..) => None,
            Marker::Range(r) => Some((range_element(r.kind), r.id)),
            Marker::Comment(id, _) => Some(("commentReference", id.0)),
        }
    }
}

/// The element a range marker is written as.
fn range_element(k: RangeKind) -> &'static str {
    match k {
        RangeKind::CommentStart => "commentRangeStart",
        RangeKind::CommentEnd => "commentRangeEnd",
        RangeKind::MoveFromStart => "moveFromRangeStart",
        RangeKind::MoveFromEnd => "moveFromRangeEnd",
        RangeKind::MoveToStart => "moveToRangeStart",
        RangeKind::MoveToEnd => "moveToRangeEnd",
    }
}

/// An inline's marker (a control's, a range's, a comment reference), onto the markers waiting for the next unit.
fn marker(i: &Inline, pend: &mut Vec<Marker>) {
    match i {
        Inline::ControlStart(c) => pend.push(Marker::Control(*c, true)),
        Inline::ControlEnd(c) => pend.push(Marker::Control(*c, false)),
        Inline::Range(r) => pend.push(Marker::Range(r.clone())),
        Inline::CommentRef { id, run } => pend.push(Marker::Comment(*id, run.clone())),
        _ => {}
    }
}

fn same_inline(x: &Inline, y: &Inline) -> bool {
    match (x, y) {
        (Inline::Text(s, r), Inline::Text(t, q)) => s == t && Rc::ptr_eq(r, q),
        (Inline::Bookmark(a), Inline::Bookmark(b)) => a == b,
        (Inline::Range(a), Inline::Range(b)) => a == b,
        (Inline::CommentRef { id: a, run: r }, Inline::CommentRef { id: b, run: q }) => a == b && Rc::ptr_eq(r, q),
        (Inline::ControlStart(a), Inline::ControlStart(b)) | (Inline::ControlEnd(a), Inline::ControlEnd(b)) => a == b,
        // A toggled checkbox keeps its run and changes its face.
        (Inline::Sym { ch: a, run: r, .. }, Inline::Sym { ch: b, run: q, .. }) => a == b && Rc::ptr_eq(r, q),
        // Moved or resized floats are written patched (their text boxes' content is its own container).
        (Inline::Float(a, r), Inline::Float(b, q)) => a.id == b.id && Placement::changed(a, b).is_none() && Rc::ptr_eq(r, q),
        // A resized picture is written patched.
        (Inline::Picture(a, r), Inline::Picture(b, q)) => a.image == b.image && a.w == b.w && a.h == b.h && a.alt == b.alt && Rc::ptr_eq(r, q),
        _ => {
            std::mem::discriminant(x) == std::mem::discriminant(y)
                && match (x.run(), y.run()) {
                    (Some(r), Some(q)) => Rc::ptr_eq(r, q),
                    _ => false,
                }
        }
    }
}

/// Unchanged since reading (the same formatting objects, the same text).
fn same_para(a: &Paragraph, b: &Paragraph) -> bool {
    Rc::ptr_eq(&a.style, &b.style)
        && Rc::ptr_eq(&a.mark, &b.mark)
        && a.inlines.len() == b.inlines.len()
        && a.inlines.iter().zip(&b.inlines).all(|(x, y)| same_inline(x, y))
}

// ---------------- unit streams and their diff ----------------

const ATOM: u64 = 1 << 40;
const MARK: u64 = u64::MAX;

/// A unit's identity for the diff and the character it writes as: text as itself, tabs and breaks as control
/// characters, objects as U+FFFC keyed by what they are (so two different pictures don't swap).
fn unit_keys(i: &Inline, out: &mut Vec<(u64, char)>) {
    use std::hash::{Hash, Hasher};
    match i {
        Inline::Text(s, _) => out.extend(s.chars().map(|c| (c as u64, c))),
        Inline::Tab(_) => out.push(('\t' as u64, '\t')),
        Inline::Break(BreakKind::Line, _) => out.push((0xB, '\u{b}')),
        Inline::Break(BreakKind::Column, _) => out.push((ATOM | BreakKind::Column as u64, '\u{e}')),
        Inline::Break(k, _) => out.push((ATOM | *k as u64, '\u{c}')),
        Inline::Bookmark(_) | Inline::Range(_) | Inline::CommentRef { .. } | Inline::Float(..) | Inline::ControlStart(_) | Inline::ControlEnd(_) => {}
        other => {
            let mut h = std::hash::DefaultHasher::new();
            // By what the object is, not its formatting: reformatting keeps it.
            std::mem::discriminant(other).hash(&mut h);
            match other {
                // By its image: a resized picture is kept, its size patched (`Emit::seg`).
                Inline::Picture(p, _) => p.image.hash(&mut h),
                Inline::NoteRef { id, .. } => id.hash(&mut h),
                // A checkbox by its control: toggling keeps it (its face is patched, `Emit::seg`).
                Inline::Sym { control: Some(c), .. } => c.hash(&mut h),
                Inline::Sym { ch, .. } => ch.hash(&mut h),
                Inline::PTab(t, _) => t.hash(&mut h),
                // A field's markers by their field: editing its code keeps them.
                Inline::FieldBegin { id, .. } | Inline::FieldSep { id, .. } | Inline::FieldEnd { id, .. } => id.hash(&mut h),
                _ => {}
            }
            // A symbol writes as its character (a checkbox's face); other objects as U+FFFC.
            let ch = if let Inline::Sym { ch, .. } = other { *ch } else { '\u{FFFC}' };
            out.push((ATOM | (h.finish() >> 24) << 41, ch));
        }
    }
}

/// For each unit of `a`, the unit of `b` it is kept as. Objects found once in each are matched first (in order, the
/// longest such chain), so no alignment of look-alike text can trade one away; the stretches between are diffed.
fn diff(a: &[u64], b: &[u64]) -> Vec<Option<usize>> {
    let once = |v: &[u64]| {
        let mut n: HashMap<u64, (usize, usize)> = HashMap::new();
        for (i, &k) in v.iter().enumerate().filter(|(_, k)| **k & ATOM != 0 && **k != MARK) {
            n.entry(k).or_insert((0, i)).0 += 1;
        }
        n.into_iter().filter(|(_, (c, _))| *c == 1).map(|(k, (_, i))| (k, i)).collect::<HashMap<_, _>>()
    };
    let (oa, ob) = (once(a), once(b));
    let mut pairs: Vec<(usize, usize)> = oa.iter().filter_map(|(k, &i)| Some((i, *ob.get(k)?))).collect();
    pairs.sort();
    // Longest chain increasing in b (patience sorting).
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; pairs.len()];
    for (p, &(_, j)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| pairs[t].1 < j);
        if at > 0 {
            prev[p] = tails[at - 1];
        }
        if at == tails.len() { tails.push(p) } else { tails[at] = p }
    }
    let mut chain = Vec::new();
    let mut p = tails.last().copied().unwrap_or(usize::MAX);
    while p != usize::MAX {
        chain.push(pairs[p]);
        p = prev[p];
    }
    chain.reverse();
    let mut out = vec![None; a.len()];
    let (mut i0, mut j0) = (0, 0);
    for (i, j) in chain.into_iter().chain([(a.len(), b.len())]) {
        for (k, m) in diff_run(&a[i0..i], &b[j0..j]).into_iter().enumerate() {
            out[i0 + k] = m.map(|m| j0 + m);
        }
        if i < a.len() {
            out[i] = Some(j);
        }
        (i0, j0) = (i + 1, j + 1);
    }
    out
}

/// A longest common subsequence: common ends, then Myers' O(ND) diff on the middle.
// shortcut: past 1000 differences the middle counts as replaced (its text rewritten in fresh runs).
fn diff_run(a: &[u64], b: &[u64]) -> Vec<Option<usize>> {
    let mut out = vec![None; a.len()];
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    for k in 0..pre {
        out[k] = Some(k);
    }
    for k in 0..suf {
        out[a.len() - 1 - k] = Some(b.len() - 1 - k);
    }
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    if !am.is_empty()
        && !bm.is_empty()
        && let Some(m) = myers(am, bm, 1000)
    {
        for (k, j) in m.into_iter().enumerate() {
            out[pre + k] = j.map(|j| pre + j);
        }
    }
    out
}

fn myers(a: &[u64], b: &[u64], cap: usize) -> Option<Vec<Option<usize>>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m) as usize;
    let off = max as isize + 1;
    let mut v = vec![0isize; 2 * max + 3];
    // trace[d][i] = furthest x on diagonal k = -d + 2i after d differences.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut end = None;
    for d in 0..=(max.min(cap) as isize) {
        let mut row = Vec::with_capacity(d as usize + 1);
        for k in (-d..=d).step_by(2) {
            let at = |k: isize| v[(off + k) as usize];
            let mut x = if k == -d || (k != d && at(k - 1) < at(k + 1)) { at(k + 1) } else { at(k - 1) + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(off + k) as usize] = x;
            row.push(x);
            if x >= n && y >= m {
                end = Some(d);
            }
        }
        trace.push(row);
        if end.is_some() {
            break;
        }
    }
    let d_end = end?;
    let get = |d: isize, k: isize| trace[d as usize][((k + d) / 2) as usize];
    let mut out = vec![None; a.len()];
    let (mut x, mut y) = (n, m);
    for d in (1..=d_end).rev() {
        let k = x - y;
        let pk = if k == -d || (k != d && get(d - 1, k - 1) < get(d - 1, k + 1)) { k + 1 } else { k - 1 };
        let px = get(d - 1, pk);
        let py = px - pk;
        while x > px && y > py {
            x -= 1;
            y -= 1;
            out[x as usize] = Some(y as usize);
        }
        (x, y) = (px, py);
    }
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        out[x as usize] = Some(y as usize);
    }
    Some(out)
}

// ---------------- writing a group ----------------

/// A unit as read: its key, the run it was in (identity), whether it is a paragraph mark.
struct Orig {
    key: u64,
    run: *const RunStyle,
    /// The character it wrote (a checkbox's face, to see it toggled).
    ch: char,
    /// An inline picture's size and alternative text (to see them changed).
    size: Option<(f64, f64, Rc<str>)>,
}

/// A unit now: what it writes, and in which formatting (`None`: a paragraph mark).
struct Cur {
    key: u64,
    ch: char,
    /// A field code's character (written in `w:instrText`).
    code: bool,
    run: Option<Run>,
    /// What a new object writes as, for the objects the model can write (note reference and note marks).
    obj: Option<String>,
    size: Option<(f64, f64, Rc<str>)>,
    /// The markers right before it (inline controls, comment and move ranges, comment references, in order): written
    /// when the group is written from the model.
    ctl: Vec<Marker>,
    /// A checkbox whose control's properties changed: its new `w:sdtPr`, patched into its source ([`Emit::seg`]).
    pr: Option<String>,
}

/// An element that produced units: its bytes, its first unit in the group's stream, how many.
struct Seg {
    range: Range<usize>,
    first: usize,
    n: usize,
}

/// Where pending run content goes: the source run being walked, or a run of its own in some formatting.
enum To {
    Src,
    Own(Run),
}

impl To {
    fn same(&self, o: &To) -> bool {
        match (self, o) {
            (To::Src, To::Src) => true,
            (To::Own(a), To::Own(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A source element being walked: what reopens and closes it in the output (a run reopens with its `w:rPr`).
struct Open {
    open: String,
    close: String,
    run: bool,
    /// A hyperlink: where it ends in the source.
    link_end: Option<usize>,
    /// A revision wrapper (`w:ins`, `w:del`, `w:moveFrom`, `w:moveTo`).
    rev: Option<Wrapper>,
}

/// A source revision wrapper: whether it deletes (`w:del`, `w:moveFrom`) or inserts, and its `w:id`.
#[derive(Clone, Copy)]
struct Wrapper {
    del: bool,
    id: Option<i64>,
}

impl Wrapper {
    fn of(n: Node) -> Option<Wrapper> {
        let del = if is_w(n, "ins") || is_w(n, "moveTo") {
            false
        } else if is_w(n, "del") || is_w(n, "moveFrom") {
            true
        } else {
            return None;
        };
        Some(Wrapper { del, id: n.attribute((W, "id")).and_then(|v| v.trim().parse().ok()) })
    }

    fn is(&self, m: &Option<Rc<RevisionMark>>) -> bool {
        m.as_ref().is_some_and(|m| Some(m.id.0) == self.id)
    }
}

/// Revision wrappers written around runs of their own, open at `level` of the walked elements, for these marks.
struct OwnWrap {
    level: usize,
    ins: Option<Rc<RevisionMark>>,
    del: Option<Rc<RevisionMark>>,
    close: String,
}

struct Emit<'s, 'a, 'i> {
    s: &'s Saver<'a, 'i>,
    out: String,
    /// Source elements of floats the group no longer has (deleted objects): not written.
    dropped: Vec<Range<usize>>,
    /// Source elements of floats moved or resized in place, with their new placement: written patched.
    patched: Vec<(Range<usize>, Placement)>,
    orig: Vec<Orig>,
    cur: Vec<Cur>,
    /// The cur unit each orig unit is kept as; and the cur units inserted before each orig unit (`ins[orig.len()]`:
    /// at the end).
    kept: Vec<Option<usize>>,
    ins: Vec<Vec<usize>>,
    segs: Vec<Seg>,
    seg_at: HashMap<usize, usize>,
    seg_starts: Vec<usize>,
    stack: Vec<Open>,
    /// How many of `stack` are open in the output.
    depth: usize,
    /// Each output paragraph's start tag and `w:pPr`, and end tag; the next one to open; whether one is open.
    heads: Vec<(String, String)>,
    next: usize,
    in_para: bool,
    /// The head and end tag of a hidden-mark paragraph being walked (STYLE-7), which opens instead of the next head.
    hidden: Option<(String, String)>,
    /// The formatting the last kept unit was given when it changed (runs without text inside it follow, see
    /// `span_run`).
    fmt_run: Option<Run>,
    /// Insertions after a hyperlink's last unit (gap, the link's stack index): they go after the link (text
    /// typed at a link's end is not part of it, nor in its style).
    after_link: Vec<(usize, usize)>,
    strip_link: bool,
    pend: String,
    text: String,
    /// Whether `text` is a field code's (`w:instrText`) or not (`w:t`).
    text_code: bool,
    to: Option<To>,
    /// The revision wrappers open around the last runs of their own.
    wrap: Option<OwnWrap>,
    /// The group is written from the model (its source can't be walked, or its inline controls changed): control
    /// markers are written from `Cur::ctl`.
    model: bool,
    /// Range markers and comment references in the group's source paragraphs, by element and id. Range markers are
    /// written from there when the group is written from the model (with what the model doesn't keep, such as a move
    /// range's author).
    marker_src: HashMap<(&'static str, i64), Range<usize>>,
    /// Those the group's paragraphs were read with that sit outside them (between blocks): kept there.
    marker_outside: Vec<(&'static str, i64)>,
    /// The move marks of the group's runs, for new move ranges' authors and dates.
    moves: Vec<Rc<RevisionMark>>,
}

impl<'s, 'a, 'i> Emit<'s, 'a, 'i> {
    fn group(s: &'s Saver<'a, 'i>, qs: &[&Paragraph], srcs: &[&ParaSrc], ps: &[&Paragraph]) -> Result<String, String> {
        // The source text can be walked when every unit has its element and no element holds another's.
        let mut segs = Vec::new();
        let mut orig = Vec::new();
        // The markers before each orig unit (see `Cur::ctl`).
        let mut orig_ctl: Vec<Vec<Marker>> = Vec::new();
        let mut mapped = srcs.iter().all(|x| !x.lossy);
        for (q, src) in qs.iter().zip(srcs) {
            let base = orig.len();
            let mut keys = Vec::new();
            let mut pend = Vec::new();
            for i in &q.inlines {
                let n = keys.len();
                unit_keys(i, &mut keys);
                marker(i, &mut pend);
                let run = i.run().map_or(std::ptr::null(), Rc::as_ptr);
                let size = picture_size(i);
                orig.extend(keys[n..].iter().map(|&(key, ch)| Orig { key, run, ch, size: size.clone() }));
                for k in n..keys.len() {
                    orig_ctl.push(if k == n { std::mem::take(&mut pend) } else { Vec::new() });
                }
            }
            let mut first = base;
            for (range, n) in &src.segs {
                segs.push(Seg { range: range.clone(), first, n: *n });
                first += n;
            }
            mapped &= first == orig.len();
            orig.push(Orig { key: MARK, run: std::ptr::null(), ch: '\n', size: None });
            orig_ctl.push(pend);
        }
        mapped &= segs.windows(2).all(|w| w[0].range.end <= w[1].range.start);
        let mut cur = Vec::new();
        for p in ps {
            let mut keys = Vec::new();
            // Whether each open field is still in its code (the innermost decides).
            let mut fields: Vec<bool> = Vec::new();
            let mut pend = Vec::new();
            for i in &p.inlines {
                let n = keys.len();
                unit_keys(i, &mut keys);
                marker(i, &mut pend);
                let run = i.run().cloned();
                let obj = s.object_xml(i);
                let size = picture_size(i);
                let code = matches!(i, Inline::Text(..)) && fields.last() == Some(&true);
                let pr = match i {
                    Inline::Sym { control: Some(c), .. } => {
                        s.controls.get(c).filter(|k| !k.as_read()).map(|k| sdt_pr(&control_open(&s.w, *c, k), &s.w).to_string())
                    }
                    _ => None,
                };
                match i {
                    Inline::FieldBegin { .. } => fields.push(true),
                    Inline::FieldSep { .. } => {
                        if let Some(f) = fields.last_mut() {
                            *f = false;
                        }
                    }
                    Inline::FieldEnd { .. } => {
                        fields.pop();
                    }
                    _ => {}
                }
                cur.extend(keys[n..].iter().enumerate().map(|(k, &(key, ch))| Cur {
                    key,
                    ch,
                    code,
                    run: run.clone(),
                    obj: obj.clone(),
                    size: size.clone(),
                    ctl: if k == 0 { std::mem::take(&mut pend) } else { Vec::new() },
                    pr: pr.clone(),
                }));
            }
            cur.push(Cur { key: MARK, ch: '\n', code: false, run: None, obj: None, size: None, ctl: pend, pr: None });
        }
        let keys_of = |orig: &[Orig]| orig.iter().map(|o| o.key).collect::<Vec<_>>();
        let cur_keys: Vec<u64> = cur.iter().map(|c| c.key).collect();
        let mut kept = diff(&keys_of(&orig), &cur_keys);
        // The source's inline controls, comment and move ranges and comment references are walked as they were: right
        // when every kept unit has the markers it had before it, and no deleted or inserted unit has any. A marker
        // inserted, removed or moved against the text (or a control emptied, unwrapped or changed) means writing the
        // group from the model, its controls from their properties.
        let faithful = || {
            if orig_ctl.iter().all(Vec::is_empty) && cur.iter().all(|c| c.ctl.is_empty()) {
                return true;
            }
            let mut taken = vec![false; cur.len()];
            for (i, k) in kept.iter().enumerate() {
                match k {
                    Some(j) if orig_ctl[i] != cur[*j].ctl => return false,
                    Some(j) => taken[*j] = true,
                    None if !orig_ctl[i].is_empty() => return false,
                    None => {}
                }
            }
            cur.iter().zip(&taken).all(|(c, t)| *t || c.ctl.is_empty())
                && !cur
                    .iter()
                    .flat_map(|c| c.ctl.iter())
                    .any(|m| matches!(m, Marker::Control(id, _) if s.controls.get(id).is_some_and(|k| !k.as_read())))
        };
        mapped &= faithful();
        if !mapped {
            // shortcut: written from the model (an unmappable source such as a field across runs of different parents,
            // or changed inline markers), so the group's content the model doesn't hold (bookmarks, hyperlinks) is lost.
            // Revisions, comment and move ranges and comment references are written from the model.
            orig.retain(|o| o.key == MARK);
            segs.clear();
            kept = diff(&keys_of(&orig), &cur_keys);
        }
        let mut ins = vec![Vec::new(); orig.len() + 1];
        let mut taken = vec![false; cur.len()];
        for j in kept.iter().flatten() {
            taken[*j] = true;
        }
        let mut gap = 0;
        let mut ki = 0;
        for j in 0..cur.len() {
            if taken[j] {
                while ki < kept.len() && kept[ki] != Some(j) {
                    ki += 1;
                }
                ki += 1;
                gap = ki;
            } else {
                ins[gap].push(j);
            }
        }

        // Output paragraph heads: the source paragraph with the same paragraph formatting (a split's new paragraph
        // shares it), its section break moved to the group's last paragraph.
        let nodes: Vec<Node> = srcs
            .iter()
            .map(|x| s.paras.get(&x.ranges.last().unwrap().start).copied())
            .collect::<Option<_>>()
            .ok_or("a paragraph's source is missing")?;
        let sect = srcs
            .iter()
            .flat_map(|x| x.ranges.iter())
            .filter_map(|r| s.paras.get(&r.start).and_then(|n| child(*n, "pPr")).and_then(|p| child(p, "sectPr")))
            .last()
            .map(|n| &s.xml[n.range()]);
        let mut used = vec![false; qs.len()];
        // Written from the model, the group's source drawings aren't walked: every float goes at its paragraph's start,
        // as a re-anchored one does.
        let own: &[&Paragraph] = if mapped { qs } else { &[] };
        let heads = ps
            .iter()
            .enumerate()
            .map(|(m, p)| {
                let last = m + 1 == ps.len();
                // The paragraph it continues: same formatting, else the one it came from, else the group's first.
                let by_style = qs.iter().position(|q| Rc::ptr_eq(&q.style, &p.style));
                let by_src = qs.iter().position(|q| q.src.as_ref().zip(p.src.as_ref()).is_some_and(|(a, b)| Rc::ptr_eq(a, b)));
                let sect = sect.filter(|_| last);
                match by_style.or(by_src).or((!qs.is_empty()).then_some(0)) {
                    Some(k) => {
                        let (mut open, close) = s.tags(nodes[k]);
                        if std::mem::replace(&mut used[k], true) {
                            open = strip_attr(&strip_attr(&open, "paraId"), "textId");
                        }
                        let ppr = match by_style {
                            Some(k) if Rc::ptr_eq(&qs[k].mark, &p.mark) => s.ppr(nodes[k], sect),
                            _ => s.edited_ppr(by_style.map(|k| nodes[k]), p, !Rc::ptr_eq(&qs[k].mark, &p.mark), sect),
                        };
                        (open + &ppr + &s.moved_in(p, own), close)
                    }
                    None => (format!("<{}p>", s.w) + &s.edited_ppr(None, p, true, sect) + &s.moved_in(p, own), format!("</{}p>", s.w)),
                }
            })
            .collect();

        // New block-level controls (pasted, or made since reading) open before the first of their paragraphs in the group
        // and close after the last; the source's own wrap its paragraphs already.
        // shortcut: one spanning several groups is written as one control per group.
        let mut heads: Vec<(String, String)> = heads;
        let new_ctl = |m: Option<usize>| -> Vec<ControlId> {
            m.and_then(|m| ps.get(m))
                .map(|p| p.controls.iter().copied().filter(|c| s.controls.get(c).is_some_and(|k| k.open.is_empty())).collect())
                .unwrap_or_default()
        };
        for (m, head) in heads.iter_mut().enumerate() {
            let (prev, now, next) = (new_ctl(m.checked_sub(1)), new_ctl(Some(m)), new_ctl(Some(m + 1)));
            let opens: String =
                now.iter().filter(|c| !prev.contains(c)).filter_map(|c| s.controls.get(c).map(|k| control_open(&s.w, *c, k))).collect();
            let closes: String =
                now.iter().rev().filter(|c| !next.contains(c)).filter_map(|c| s.controls.get(c).map(|k| control_close(&s.w, k))).collect();
            head.0.insert_str(0, &opens);
            head.1.push_str(&closes);
        }
        let floats = |ps: &[&Paragraph]| -> Vec<(u64, Option<Range<usize>>)> {
            ps.iter()
                .flat_map(|p| p.inlines.iter())
                .filter_map(|i| if let Inline::Float(f, _) = i { Some((f.id, f.src.clone())) } else { None })
                .collect()
        };
        let now: Vec<u64> = floats(ps).into_iter().map(|(id, _)| id).collect();
        let dropped = floats(qs).into_iter().filter(|(id, _)| !now.contains(id)).filter_map(|(_, src)| src).collect();
        let all = |ps: &[&Paragraph]| -> Vec<Float> {
            ps.iter().flat_map(|p| p.inlines.iter()).filter_map(|i| if let Inline::Float(f, _) = i { Some((**f).clone()) } else { None }).collect()
        };
        let (was, is) = (all(qs), all(ps));
        let patched = was
            .iter()
            .filter_map(|a| {
                let b = is.iter().find(|b| b.id == a.id)?;
                let p = Placement::changed(a, b)?;
                Some((a.src.clone()?, p))
            })
            .collect();
        let seg_at = segs.iter().enumerate().map(|(k, g)| (g.range.start, k)).collect();
        let seg_starts = segs.iter().map(|g| g.range.start).collect();
        let (mut marker_src, mut marker_outside, mut moves) = (HashMap::new(), Vec::new(), Vec::new());
        if !mapped {
            const MARKERS: [&str; 7] = [
                "commentRangeStart",
                "commentRangeEnd",
                "moveFromRangeStart",
                "moveFromRangeEnd",
                "moveToRangeStart",
                "moveToRangeEnd",
                "commentReference",
            ];
            for n in srcs.iter().flat_map(|x| x.ranges.iter()).filter_map(|r| s.paras.get(&r.start)) {
                for d in n.descendants().filter(|d| d.is_element() && d.tag_name().namespace() == Some(W)) {
                    let (Some(name), Some(id)) =
                        (MARKERS.iter().find(|m| **m == d.tag_name().name()), d.attribute((W, "id")).and_then(|v| v.trim().parse::<i64>().ok()))
                    else {
                        continue;
                    };
                    marker_src.entry((*name, id)).or_insert(d.range());
                }
            }
            let mut read = Vec::new();
            qs.iter().flat_map(|q| q.inlines.iter()).for_each(|i| marker(i, &mut read));
            marker_outside = read.iter().filter_map(Marker::element).filter(|k| !marker_src.contains_key(k)).collect();
            moves = ps
                .iter()
                .flat_map(|p| p.inlines.iter())
                .filter_map(Inline::run)
                .flat_map(|r| [&r.rev.ins, &r.rev.del])
                .flatten()
                .filter(|m| m.move_name.is_some())
                .cloned()
                .collect();
        }
        let mut e = Emit {
            s,
            out: String::new(),
            dropped,
            patched,
            orig,
            cur,
            kept,
            ins,
            segs,
            seg_at,
            seg_starts,
            stack: Vec::new(),
            depth: 0,
            heads,
            next: 0,
            in_para: false,
            hidden: None,
            fmt_run: None,
            after_link: Vec::new(),
            strip_link: false,
            pend: String::new(),
            text: String::new(),
            text_code: false,
            to: None,
            wrap: None,
            model: !mapped,
            marker_src,
            marker_outside,
            moves,
        };
        // Hidden marks are kept only while no paragraph mark comes or goes (they would mark the wrong paragraph).
        let stable = e.ins.iter().flatten().all(|&j| e.cur[j].run.is_some()) && e.orig.iter().zip(&e.kept).all(|(o, k)| o.key != MARK || k.is_some());
        let mut mark = 0;
        for (q, src) in qs.iter().zip(srcs) {
            mark += if mapped { q.inlines.iter().map(Inline::units).sum::<usize>() } else { 0 };
            if mapped {
                for (k, r) in src.ranges.iter().enumerate() {
                    let node = s.paras[&r.start];
                    // STYLE-7: a paragraph joined to the next by its hidden mark stays a paragraph of its own (unless
                    // something before it is still open).
                    if stable && k + 1 < src.ranges.len() && !e.in_para {
                        let (open, close) = s.tags(node);
                        e.hidden = Some((open + child(node, "pPr").map_or("", |p| &s.xml[p.range()]), close));
                    }
                    e.walk(node);
                    if e.hidden.is_some() {
                        e.flush();
                        e.open_to(0);
                        e.out.push_str(&e.hidden.take().unwrap().1);
                        e.in_para = false;
                    }
                }
            }
            e.mark(mark);
            mark += 1;
        }
        e.insert(e.orig.len(), None, true);
        if e.next < e.heads.len() {
            e.close_para();
        }
        if e.next != e.heads.len() {
            return Err(format!("wrote {} of {} paragraphs", e.next, e.heads.len()));
        }
        Ok(e.out)
    }

    /// Where insertions at gap `g` split between its neighbours: those in the next unit's run go before it (the
    /// tail), the rest after the unit before (the head); after a paragraph mark or at the start, all go before.
    fn split(&self, g: usize) -> usize {
        if g == 0 || self.orig[g - 1].key == MARK {
            return 0;
        }
        let next = self.orig.get(g).filter(|o| o.key != MARK).map(|o| o.run);
        let v = &self.ins[g];
        v.len() - v.iter().rev().take_while(|&&j| next.is_some() && self.cur[j].run.as_ref().map(Rc::as_ptr) == next).count()
    }

    fn target(&self, ctx: Option<*const RunStyle>, c: &Cur) -> To {
        let run = c.run.clone().expect("a unit in a run");
        if ctx == Some(Rc::as_ptr(&run)) { To::Src } else { To::Own(run) }
    }

    fn walk(&mut self, parent: Node) {
        let in_run = is_w(parent, "r");
        let mut skip_to = 0;
        for c in parent.children() {
            let r = c.range();
            if r.start < skip_to || (is_w(c, "pPr") && is_w(parent, "p")) || (in_run && is_w(c, "rPr")) {
                continue;
            }
            // A deleted float's element (or the run or wrapper holding only it).
            if self.dropped.iter().any(|d| r.start <= d.start && d.end <= r.end) && !self.seg_starts.iter().any(|&x| x >= r.start && x < r.end) {
                continue;
            }
            if let Some(&k) = self.seg_at.get(&r.start) {
                let end = self.segs[k].range.end;
                let text = ["t", "instrText", "delText", "delInstrText"].iter().any(|t| is_w(c, t));
                self.seg(k, in_run && end == r.end, text, c);
                skip_to = end;
                continue;
            }
            let inside = self.seg_starts.partition_point(|&x| x <= r.start);
            if c.is_element() && self.seg_starts.get(inside).is_some_and(|&x| x < r.end) {
                let (mut open, close) = self.s.tags(c);
                let run = is_w(c, "r");
                if let Some(rpr) = child(c, "rPr").filter(|_| run) {
                    open.push_str(&self.s.xml[rpr.range()]);
                }
                self.flush();
                let link_end = is_w(c, "hyperlink").then_some(r.end);
                self.stack.push(Open { open, close, run, link_end, rev: Wrapper::of(c) });
                self.walk(c);
                self.flush();
                if self.depth == self.stack.len() {
                    self.close_wrap();
                    self.depth -= 1;
                    self.out.push_str(&self.stack[self.depth].close);
                }
                self.stack.pop();
                self.depth = self.depth.min(self.stack.len());
                let level = self.stack.len();
                let after: Vec<usize> = self.after_link.iter().filter(|(_, l)| *l == level).map(|(g, _)| *g).collect();
                self.after_link.retain(|(_, l)| *l != level);
                for g in after {
                    self.strip_link = true;
                    self.insert(g, None, false);
                    self.flush();
                    self.strip_link = false;
                }
            } else if let Some(run) = is_w(c, "r").then(|| self.span_run(r.start)).flatten() {
                self.restyled(c, r.end, &run);
            } else {
                self.verbatim(r);
            }
        }
    }

    /// The formatting a run without text takes when it sits inside reformatted text (field codes, hidden runs): the
    /// text before it and after it went to the same new formatting.
    fn span_run(&self, pos: usize) -> Option<Run> {
        let run = self.fmt_run.clone()?;
        let k = self.seg_starts.partition_point(|&x| x <= pos);
        let i = self.segs.get(k)?.first;
        let j = self.kept[i]?;
        let cur = self.cur[j].run.as_ref()?;
        (Rc::ptr_eq(cur, &run) && Rc::as_ptr(cur) != self.orig[i].run).then_some(run)
    }

    /// Whole runs from `first` up to `end`, each with `run`'s formatting (and its formatting change) as its `w:rPr`.
    // shortcut: they stay in the revision wrappers they were read in; a run without text whose insertion or deletion
    // changed with the text around it keeps its old one.
    fn restyled(&mut self, first: Node, end: usize, run: &Run) {
        self.flush();
        self.open_to(self.stack.len());
        let rpr = revisions::run_rpr(run, &self.s.cx, self.s.people);
        for n in std::iter::successors(Some(first), |n| n.next_sibling()).take_while(|n| n.range().start < end) {
            self.restyle(n, &rpr);
        }
        self.fmt_run = Some(run.clone());
    }

    /// A run with `rpr` as its properties; a simple field's runs too.
    fn restyle(&mut self, n: Node, rpr: &str) {
        if is_w(n, "r") || is_w(n, "fldSimple") || is_w(n, "sdt") || is_w(n, "sdtContent") {
            let (open, close) = self.s.tags(n);
            self.out.push_str(&open);
            if is_w(n, "r") {
                // A run whose new formatting has no fonts keeps its own (a checkbox's face: its control's formatting
                // is kept without them).
                let fonts = child(n, "rPr").and_then(|p| child(p, "rFonts")).map(|f| &self.s.xml[f.range()]);
                match fonts {
                    Some(f) if !rpr.contains("rFonts") => {
                        let w = &self.s.w;
                        let body = rpr.strip_prefix(&format!("<{w}rPr>")).and_then(|b| b.strip_suffix(&format!("</{w}rPr>"))).unwrap_or("");
                        self.out.push_str(&format!("<{w}rPr>{f}{body}</{w}rPr>"));
                    }
                    _ => self.out.push_str(rpr),
                }
            }
            for c in n.children().filter(|c| !is_w(*c, "rPr")) {
                self.restyle(c, rpr);
            }
            self.out.push_str(&close);
        } else {
            let x = self.src(n.range());
            self.out.push_str(&x);
        }
    }

    /// An element that produced units `first..first + n`; `in_run` when it is a run's child (text, tab, drawing),
    /// not whole runs (a complex field) or a simple field.
    fn seg(&mut self, k: usize, in_run: bool, text: bool, node: Node) {
        let Seg { ref range, first, n } = self.segs[k];
        let range = range.clone();
        let ctx = |e: &Self, i: usize| in_run.then_some(e.orig[i].run);
        let resized = |e: &Self, i: usize| e.kept[i].and_then(|j| (e.cur[j].size != e.orig[i].size).then(|| e.cur[j].size.clone()).flatten());
        let unchanged = (first..first + n).all(|i| {
            self.kept[i].is_some_and(|j| self.cur[j].run.as_ref().is_some_and(|r| Rc::as_ptr(r) == self.orig[i].run)) && resized(self, i).is_none()
        }) && (first + 1..first + n).all(|g| self.ins[g].is_empty());
        // A checkbox toggled, or its control's properties changed: its control written with the new
        // properties, state and face.
        let checkbox = (n == 1 && !in_run && self.s.xml[range.clone()].contains("checkbox")).then(|| self.kept[first]).flatten();
        let toggled = checkbox.map(|j| (self.cur[j].ch, self.cur[j].pr.clone())).filter(|(ch, pr)| *ch != self.orig[first].ch || pr.is_some());
        if let Some((ch, pr)) = toggled {
            self.insert(first, None, true);
            let mut src = self.s.xml[range.clone()].to_string();
            if let Some(pr) = pr {
                let old = sdt_pr(&src, &self.s.w).to_string();
                if !old.is_empty() {
                    src = src.replacen(&old, &pr, 1);
                }
            }
            let x = checkbox_xml(&src, ch);
            self.flush();
            self.open_to(self.stack.len());
            self.out.push_str(&x);
            self.fmt_run = None;
            self.insert_after(first + n, None, range.end);
            return;
        }
        if unchanged {
            self.insert(first, ctx(self, first), true);
            if in_run {
                self.add(To::Src, None, Some(range.clone()));
            } else {
                self.verbatim(range.clone());
            }
            self.fmt_run = None;
            self.insert_after(first + n, ctx(self, first + n - 1), range.end);
            return;
        }
        if !in_run && n > 1 {
            // A changed element holding several units (a simple field): written from the model, as
            // what each unit is now (a simple field becomes a complex one).
            for i in first..first + n {
                self.insert(i, None, true);
                if let Some(j) = self.kept[i] {
                    self.emit(j, None);
                }
                self.insert(i + 1, None, false);
            }
            self.flush();
            self.fmt_run = None;
            self.insert_after(first + n, None, range.end);
            return;
        }
        for i in first..first + n {
            self.insert(i, ctx(self, i), true);
            if let Some(j) = self.kept[i] {
                let run = self.cur[j].run.clone().expect("a unit in a run");
                if !in_run {
                    // A whole field (its runs) or a simple field: reformatted, every run of it takes the formatting.
                    if Rc::as_ptr(&run) == self.orig[i].run {
                        self.verbatim(range.clone());
                        self.fmt_run = None;
                    } else {
                        self.restyled(node, range.end, &run);
                    }
                } else {
                    let to = self.target(ctx(self, i), &self.cur[j]);
                    self.fmt_run = matches!(to, To::Own(_)).then_some(run);
                    if text {
                        let (ch, code) = (self.cur[j].ch, self.cur[j].code);
                        self.add_char(to, ch, code)
                    } else if let Some(size) = resized(self, i) {
                        // A resized inline picture: its drawing with the new extents.
                        self.add(to, None, None);
                        self.flush_text();
                        let p = Placement { h: None, v: None, size: Some((size.0, size.1)), wrap: None, locked: None, alt: Some(size.2.to_string()) };
                        self.pend.push_str(&p.apply(&self.s.src_text(range.clone())));
                    } else {
                        self.add(to, None, Some(range.clone()))
                    }
                }
            }
            if i + 1 == first + n {
                self.insert_after(i + 1, ctx(self, i), range.end);
            } else {
                self.insert(i + 1, ctx(self, i), false);
            }
        }
    }

    /// The head of gap `g`'s insertions after an element ending at `end`: deferred past a hyperlink whose content ends
    /// there.
    fn insert_after(&mut self, g: usize, ctx: Option<*const RunStyle>, end: usize) {
        let link = self.stack.iter().rposition(|o| o.link_end.is_some());
        let at_end = link.is_some_and(|l| {
            let le = self.stack[l].link_end.unwrap();
            let k = self.seg_starts.partition_point(|&x| x < end);
            self.seg_starts.get(k).is_none_or(|&x| x >= le)
        });
        match link {
            Some(l) if at_end && self.split(g) > 0 => self.after_link.push((g, l)),
            _ => self.insert(g, ctx, false),
        }
    }

    /// The orig paragraph mark at `i`: kept, it ends the output paragraph; deleted, the paragraphs join.
    fn mark(&mut self, i: usize) {
        self.insert(i, None, true);
        if let Some(j) = self.kept[i] {
            self.markers(j);
            self.end_para();
        }
    }

    /// Writing from the model: the markers before cur unit `j`. Controls as their markup; ranges and comment
    /// references from their source in the group, else from the model (those read from outside the group's paragraphs
    /// stay there).
    fn markers(&mut self, j: usize) {
        if !self.model {
            return;
        }
        let w = self.s.w.clone();
        for m in std::mem::take(&mut self.cur[j].ctl) {
            let key = m.element();
            // A comment reference is written from its run (its revisions with it).
            let src = key.filter(|_| matches!(m, Marker::Range(_))).and_then(|k| self.marker_src.get(&k)).cloned();
            if let Some(r) = src {
                self.flush();
                let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
                self.open_to(level);
                let x = self.src(r);
                self.out.push_str(&x);
                continue;
            }
            if key.is_some_and(|k| self.marker_outside.contains(&k)) {
                continue;
            }
            match m {
                Marker::Control(id, start) => {
                    let Some(c) = self.s.controls.get(&id) else { continue };
                    self.flush();
                    self.open_to(self.stack.len());
                    let x = if start { control_open(&w, id, c) } else { control_close(&w, c) };
                    self.out.push_str(&x);
                }
                Marker::Range(r) => {
                    self.flush();
                    let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
                    self.open_to(level);
                    let x = self.range_xml(&r);
                    self.out.push_str(&x);
                }
                Marker::Comment(id, run) => {
                    self.add(To::Own(run), None, None);
                    self.flush_text();
                    self.pend.push_str(&format!("<{w}commentReference {w}id=\"{}\"/>", id.0));
                    self.flush();
                }
            }
        }
    }

    /// A range marker written from the model; a move range's start with the author and date of its moved text.
    fn range_xml(&self, r: &RangeMark) -> String {
        let w = &self.s.w;
        let el = range_element(r.kind);
        let mut a = format!(" {w}id=\"{}\"", r.id);
        if matches!(r.kind, RangeKind::MoveFromStart | RangeKind::MoveToStart) {
            if let Some(m) = self.moves.iter().find(|m| m.move_name == r.name) {
                let author = self.s.people.get(m.author).map_or("", |p| &p.name);
                a.push_str(&format!(" {w}author=\"{}\"", props::esc(author)));
                if let Some(d) = &m.date {
                    a.push_str(&format!(" {w}date=\"{}\"", props::esc(&d.raw)));
                }
            }
            if let Some(n) = &r.name {
                a.push_str(&format!(" {w}name=\"{}\"", props::esc(n)));
            }
        }
        format!("<{w}{el}{a}/>")
    }

    /// The head or tail of gap `g`'s insertions ([`Emit::split`]).
    fn insert(&mut self, g: usize, ctx: Option<*const RunStyle>, tail: bool) {
        let s = self.split(g);
        let part = if tail { self.ins[g][s..].to_vec() } else { self.ins[g][..s].to_vec() };
        for j in part {
            self.emit(j, ctx);
        }
    }

    /// A unit as the model has it now (an insertion, or a unit of an element rewritten from the model).
    fn emit(&mut self, j: usize, ctx: Option<*const RunStyle>) {
        self.markers(j);
        if self.cur[j].run.is_none() {
            self.end_para();
        } else if let Some(x) = self.cur[j].obj.as_deref().and_then(|x| x.strip_prefix(PARA_LEVEL)) {
            // Content that is not a run's (a new content control): written between runs.
            let x = x.to_string();
            self.flush();
            let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
            self.open_to(level);
            self.out.push_str(&x);
        } else if self.cur[j].ch != '\u{FFFC}' {
            let to = self.target(ctx, &self.cur[j]);
            let (ch, code) = (self.cur[j].ch, self.cur[j].code);
            self.add_char(to, ch, code);
        } else if let Some(x) = self.cur[j].obj.clone() {
            // shortcut: other inserted objects have no source to write; rich paste will bring their XML.
            let to = self.target(ctx, &self.cur[j]);
            if !self.to.as_ref().is_some_and(|t| t.same(&to)) {
                self.flush();
                self.to = Some(to);
            }
            self.flush_text();
            self.pend.push_str(&x);
        }
    }

    /// A character of run content: a field code's in `w:instrText`, others in `w:t`.
    fn add_char(&mut self, to: To, ch: char, code: bool) {
        if code != self.text_code {
            self.flush_text();
            self.text_code = code;
        }
        self.add(to, Some(ch), None);
    }

    /// Run content: a character, or source bytes of a run child.
    fn add(&mut self, to: To, ch: Option<char>, bytes: Option<Range<usize>>) {
        if !self.to.as_ref().is_some_and(|t| t.same(&to)) {
            self.flush();
            self.to = Some(to);
        }
        let w = &self.s.w;
        match (ch, bytes) {
            (Some('\t'), _) => {
                self.flush_text();
                self.pend.push_str(&format!("<{w}tab/>"));
            }
            (Some('\u{b}'), _) => {
                self.flush_text();
                self.pend.push_str(&format!("<{w}br/>"));
            }
            (Some('\u{e}'), _) => {
                self.flush_text();
                self.pend.push_str(&format!("<{w}br {w}type=\"column\"/>"));
            }
            (Some('\u{c}'), _) => {
                self.flush_text();
                self.pend.push_str(&format!("<{w}br {w}type=\"page\"/>"));
            }
            (Some(c), _) => self.text.push(c),
            (None, Some(r)) => {
                self.flush_text();
                self.pend.push_str(&self.s.xml[r]);
            }
            (None, None) => {}
        }
    }

    /// Pending text as run content: deleted text (a deleted run's, or a source run's in a deletion) in `w:delText` or
    /// `w:delInstrText`.
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let w = &self.s.w;
            let t = self.text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            let deleted = match &self.to {
                Some(To::Own(run)) => run.rev.del.is_some(),
                _ => self.stack.iter().any(|o| o.rev.is_some_and(|r| r.del)),
            };
            let tag = match (self.text_code, deleted) {
                (false, false) => "t",
                (true, false) => "instrText",
                (false, true) => "delText",
                (true, true) => "delInstrText",
            };
            self.pend.push_str(&format!("<{w}{tag} xml:space=\"preserve\">{t}</{w}{tag}>"));
            self.text.clear();
        }
    }

    /// Where a run of its own in `run`'s formatting goes: outside the source run being walked, and outside the first
    /// source revision wrapper that isn't one of its marks (so new text never takes a revision it doesn't carry). Also
    /// the marks it needs wrappers of its own for: those the wrappers it stays in don't give it.
    fn own_level(&self, run: &RunStyle) -> (usize, Option<Rc<RevisionMark>>, Option<Rc<RevisionMark>>) {
        let base = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
        let (mut ins, mut del, mut level) = (false, false, base);
        for (k, o) in self.stack[..base].iter().enumerate() {
            let Some(r) = o.rev else { continue };
            // An insertion wraps a deletion, never the other way round.
            let fits = if r.del { !del && r.is(&run.rev.del) && (run.rev.ins.is_none() || ins) } else { !ins && !del && r.is(&run.rev.ins) };
            if !fits {
                level = k;
                break;
            }
            if r.del {
                del = true;
            } else {
                ins = true;
            }
        }
        (level, run.rev.ins.clone().filter(|_| !ins), run.rev.del.clone().filter(|_| !del))
    }

    /// Closes the revision wrappers written around the last runs of their own.
    fn close_wrap(&mut self) {
        if let Some(w) = self.wrap.take() {
            self.out.push_str(&w.close);
        }
    }

    fn flush(&mut self) {
        self.flush_text();
        let Some(to) = self.to.take() else { return };
        if self.pend.is_empty() {
            return;
        }
        let pend = std::mem::take(&mut self.pend);
        match to {
            To::Src => {
                self.open_to(self.stack.len());
                self.out.push_str(&pend);
            }
            To::Own(run) => {
                // Consecutive runs with the same marks share their wrappers.
                let (level, ins, del) = self.own_level(&run);
                let same =
                    self.in_para && self.depth == level && self.wrap.as_ref().is_some_and(|x| x.level == level && x.ins == ins && x.del == del);
                if !same {
                    self.open_to(level);
                    if ins.is_some() || del.is_some() {
                        let (open, close) = revisions::wrappers(&self.s.w, ins.as_deref(), del.as_deref(), self.s.people);
                        self.out.push_str(&open);
                        self.wrap = Some(OwnWrap { level, ins, del, close });
                    }
                }
                let w = &self.s.w;
                let mut rpr = revisions::run_rpr(&run, &self.s.cx, self.s.people);
                if self.strip_link {
                    for style in [format!("<{w}rStyle {w}val=\"Hyperlink\"/>"), format!("<{w}rStyle {w}val=\"Hyperlink\" />")] {
                        rpr = rpr.replace(&style, "");
                    }
                    if rpr == format!("<{w}rPr></{w}rPr>") {
                        rpr.clear();
                    }
                }
                self.out.push_str(&format!("<{w}r>{rpr}{pend}</{w}r>"));
            }
        }
    }

    /// Closes the wrappers of runs of their own, opens the output paragraph if needed, then opens or closes walked
    /// elements until `n` are open.
    fn open_to(&mut self, n: usize) {
        self.close_wrap();
        if !self.in_para {
            let k = self.next.min(self.heads.len().saturating_sub(1));
            let head = self.hidden.as_ref().map_or(&self.heads[k].0, |h| &h.0);
            self.out.push_str(head);
            self.in_para = true;
        }
        while self.depth > n {
            self.depth -= 1;
            self.out.push_str(&self.stack[self.depth].close);
        }
        while self.depth < n {
            self.out.push_str(&self.stack[self.depth].open);
            self.depth += 1;
        }
    }

    fn verbatim(&mut self, r: Range<usize>) {
        self.flush();
        self.open_to(self.stack.len());
        let x = self.src(r);
        self.out.push_str(&x);
    }

    /// Source bytes, with moved or resized floats in them patched.
    fn src(&self, r: Range<usize>) -> String {
        // Floats patched in place, the rest with its nested rewrites (text boxes) applied.
        let mut inside: Vec<&(Range<usize>, Placement)> = self.patched.iter().filter(|(d, _)| r.start <= d.start && d.end <= r.end).collect();
        inside.sort_by_key(|(d, _)| d.start);
        let mut out = String::new();
        let mut at = r.start;
        for (d, p) in inside {
            out.push_str(&self.s.src_text(at..d.start));
            out.push_str(&p.apply(&self.s.src_text(d.clone())));
            at = d.end;
        }
        out.push_str(&self.s.src_text(at..r.end));
        out
    }

    /// A paragraph mark: the output paragraph ends, except the group's last, which takes what is left of the
    /// paragraphs joined into it (their bookmarks, field codes, ...) and ends with the group.
    fn end_para(&mut self) {
        if self.next + 1 < self.heads.len() {
            self.close_para();
        }
    }

    fn close_para(&mut self) {
        self.flush();
        self.open_to(0);
        let k = self.next.min(self.heads.len().saturating_sub(1));
        self.out.push_str(&self.heads[k].1);
        self.in_para = false;
        self.hidden = None;
        self.next += 1;
    }
}

/// [`Emit::group`], or nothing for no paragraphs (the sources' text is deleted).
fn emit(s: &Saver, qs: &[&Paragraph], srcs: &[&ParaSrc], ps: &[&Paragraph]) -> Result<String, String> {
    if ps.is_empty() { Ok(String::new()) } else { Emit::group(s, qs, srcs, ps) }
}

fn same_src(a: &Option<Rc<Range<usize>>>, b: &Option<Rc<Range<usize>>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// The `w:sdtPr` element in a control's markup (empty when there is none).
fn sdt_pr<'s>(xml: &'s str, w: &str) -> &'s str {
    let close = format!("</{w}sdtPr>");
    match (xml.find(&format!("<{w}sdtPr")), xml.find(&close)) {
        (Some(a), Some(b)) if a < b => &xml[a..b + close.len()],
        _ => "",
    }
}

/// A checkbox control's XML showing `face`: `w14:checked` set to whether `face` is its checked state's character, and
/// its text replaced.
fn checkbox_xml(src: &str, face: char) -> String {
    let state = |name: &str| -> Option<char> {
        let k = src.find(&format!(":{name} "))?;
        let v = src[k..].find("val=\"").map(|v| k + v + 5)?;
        let e = src[v..].find('"').map(|e| v + e)?;
        u32::from_str_radix(&src[v..e], 16).ok().and_then(char::from_u32)
    };
    let checked = state("checkedState").map_or(face == '\u{2612}', |c| c == face);
    let mut s = src.to_string();
    if let Some(k) = s.find(":checked ")
        && let Some(v) = s[k..].find("val=\"").map(|v| k + v + 5)
        && let Some(e) = s[v..].find('"').map(|e| v + e)
    {
        s.replace_range(v..e, if checked { "1" } else { "0" });
    }
    // The face: the content's first text (the first `t` start tag, any prefix).
    let start = s.match_indices(":t").map(|(k, _)| k).find(|&k| {
        let lt = s[..k].rfind('<');
        lt.is_some_and(|lt| !s[lt + 1..k].contains('/') && !s[lt + 1..k].contains(' ')) && matches!(s.as_bytes().get(k + 2), Some(b'>' | b' '))
    });
    if let Some(t) = start.and_then(|k| s[k..].find('>').map(|g| k + g + 1))
        && let Some(e) = s[t..].find('<').map(|e| t + e)
    {
        s.replace_range(t..e, &face.to_string());
    }
    s
}

fn picture_size(i: &Inline) -> Option<(f64, f64, Rc<str>)> {
    if let Inline::Picture(p, _) = i { Some((p.w, p.h, p.alt.clone())) } else { None }
}

/// Marks [`Saver::object_xml`] content written between runs, not in one.
const PARA_LEVEL: &str = "\u{0}P";

/// Where new drawings' `wp:docPr/@id`s start: above what Word gives a document's own drawings.
const DRAWING_IDS: u32 = 0x4345_0000;

/// The relationship id a new picture's image is embedded by (`rIdCeImg` and its index in [`Document::images`]):
/// [`add_images`] gives each part that names one the image and the relationship.
const NEW_IMAGE: &str = "rIdCeImg";

impl Saver<'_, '_> {
    /// A float with no source (an inserted text box, or a picture float): its `w:drawing`, from the model. `None` for
    /// what the model can't write (a text box whose content can't be).
    fn new_float_xml(&self, f: &Float) -> Option<String> {
        const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
        const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
        const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
        const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
        const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let emu = |pt: f64| (pt * 12700.0).round() as i64;
        let id = self.ids.get();
        self.ids.set(id + 1);
        let (cx, cy) = (emu(f.w), emu(f.h));
        let pos = |name: &str, rel: &str, align: &Option<String>, off: f64| match align {
            Some(a) => format!("<wp:{name} relativeFrom=\"{rel}\"><wp:align>{}</wp:align></wp:{name}>", props::esc(a)),
            None => format!("<wp:{name} relativeFrom=\"{rel}\"><wp:posOffset>{}</wp:posOffset></wp:{name}>", emu(off)),
        };
        let graphic = match (&f.text_box, f.image) {
            (Some(tb), _) => {
                let blocks = new_blocks_xml(self.xml, &tb.blocks, self.people).ok()?;
                let [l, t, r, b] = tb.insets.map(emu);
                let line = if tb.outline > 0.0 {
                    format!(r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#, emu(tb.outline))
                } else {
                    "<a:ln><a:noFill/></a:ln>".to_string()
                };
                let fit = if tb.auto_fit { "<a:spAutoFit/>" } else { "<a:noAutofit/>" };
                format!(
                    concat!(
                        r#"<a:graphicData uri="{wps}"><wps:wsp xmlns:wps="{wps}"><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/>"#,
                        r#"<a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFFFF"/>"#,
                        r#"</a:solidFill>{line}</wps:spPr><wps:txbx><{w}txbxContent>{blocks}</{w}txbxContent></wps:txbx><wps:bodyPr rot="0" vert="horz" "#,
                        r#"wrap="square" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}" anchor="t" anchorCtr="0">{fit}</wps:bodyPr></wps:wsp></a:graphicData>"#
                    ),
                    wps = WPS,
                    cx = cx,
                    cy = cy,
                    line = line,
                    w = self.w,
                    blocks = blocks,
                    l = l,
                    t = t,
                    r = r,
                    b = b,
                    fit = fit,
                )
            }
            (None, Some(image)) => format!(
                concat!(
                    r#"<a:graphicData uri="{pic}"><pic:pic xmlns:pic="{pic}"><pic:nvPicPr><pic:cNvPr id="0" name="Picture {id}"/><pic:cNvPicPr/>"#,
                    r#"</pic:nvPicPr><pic:blipFill><a:blip xmlns:r="{r}" r:embed="{new}{image}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
                    r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#,
                    r#"</pic:spPr></pic:pic></a:graphicData>"#
                ),
                pic = PIC,
                id = id,
                r = R,
                new = NEW_IMAGE,
                image = image,
                cx = cx,
                cy = cy,
            ),
            _ => return None,
        };
        let on = |b: bool| if b { "1" } else { "0" };
        let [dt, db, dl, dr] = f.dist.map(emu);
        Some(format!(
            concat!(
                r#"<{w}drawing><wp:anchor xmlns:wp="{wp}" distT="{dt}" distB="{db}" distL="{dl}" distR="{dr}" simplePos="0" relativeHeight="{id}" "#,
                r#"behindDoc="{behind}" locked="{locked}" layoutInCell="{cell}" allowOverlap="{overlap}"><wp:simplePos x="0" y="0"/>{h}{v}"#,
                r#"<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}<wp:docPr id="{id}" name="{kind} {id}"/>"#,
                r#"<wp:cNvGraphicFramePr/><a:graphic xmlns:a="{a}">{graphic}</a:graphic></wp:anchor></{w}drawing>"#
            ),
            w = self.w,
            wp = WP,
            a = A,
            dt = dt,
            db = db,
            dl = dl,
            dr = dr,
            id = id,
            behind = on(f.behind),
            locked = on(f.locked),
            cell = on(f.layout_in_cell),
            overlap = on(f.allow_overlap),
            h = pos("positionH", &f.h_relative, &f.h_align, f.h_offset),
            v = pos("positionV", &f.v_relative, &f.v_align, f.v_offset),
            cx = cx,
            cy = cy,
            wrap = wrap_xml("wp:", &f.wrap, &f.wrap_text),
            kind = if f.text_box.is_some() { "Text Box" } else { "Picture" },
            graphic = graphic,
        ))
    }

    /// The run content a new object writes as, for those the model can write without a source: note
    /// references and marks, and pictures (inserted, or copied from elsewhere in the document).
    fn object_xml(&self, i: &Inline) -> Option<String> {
        if let Inline::Sym { ch, font, run, control: Some(c) } = i
            && let Some(Control { props: ControlProps { kind: ControlKind::Checkbox { checked, unchecked }, .. }, .. }) = self.controls.get(c)
        {
            let w = &self.w;
            let hex = |c: char| format!("{:04X}", c as u32);
            let esc = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
            let rpr = run.direct.0.as_deref().unwrap_or("");
            let on = if ch == checked { 1 } else { 0 };
            let (cs, us, f) = (hex(*checked), hex(*unchecked), esc(font));
            // From its properties (alias, tag, lock and the rest), with the state it shows now.
            let mut ctl = self.controls[c].clone();
            ctl.open.clear();
            ctl.props.kind_xml = Some(format!(
                concat!(
                    "<w14:checkbox xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\">",
                    "<w14:checked w14:val=\"{on}\"/><w14:checkedState w14:val=\"{cs}\" w14:font=\"{f}\"/><w14:uncheckedState w14:val=\"{us}\" w14:font=\"{f}\"/>",
                    "</w14:checkbox>"
                ),
                on = on,
                cs = cs,
                us = us,
                f = f,
            ));
            return Some(format!("{PARA_LEVEL}{}<{w}r>{rpr}<{w}t>{ch}</{w}t></{w}r></{w}sdtContent></{w}sdt>", control_open(w, *c, &ctl)));
        }
        if let Inline::Picture(p, _) = i {
            let image = p.image?;
            let id = self.ids.get();
            self.ids.set(id + 1);
            let emu = |pt: f64| (pt * 12700.0).round() as i64;
            let (cx, cy) = (emu(p.w), emu(p.h));
            const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
            const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
            const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
            const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
            return Some(format!(
                concat!(
                    r#"<{w}drawing><wp:inline xmlns:wp="{wp}" distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>"#,
                    r#"<wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="Picture {id}"/><wp:cNvGraphicFramePr>"#,
                    r#"<a:graphicFrameLocks xmlns:a="{a}" noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic xmlns:a="{a}">"#,
                    r#"<a:graphicData uri="{pic}"><pic:pic xmlns:pic="{pic}"><pic:nvPicPr><pic:cNvPr id="0" name="Picture {id}"/>"#,
                    r#"<pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip xmlns:r="{r}" r:embed="{new}{image}"/><a:stretch><a:fillRect/>"#,
                    r#"</a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>"#,
                    r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></{w}drawing>"#
                ),
                w = self.w,
                wp = WP,
                a = A,
                pic = PIC,
                r = R,
                new = NEW_IMAGE,
                cx = cx,
                cy = cy,
                id = id,
                image = image,
            ));
        }
        object_xml(&self.w, i)
    }
}

/// Each made part that embeds a new picture's image ([`NEW_IMAGE`]): the image as a media part beside it, its
/// relationship, and the content type of its extension.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn add_images(package: &[u8], images: &[Vec<u8>], parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use std::io::Read;
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(mut f) = zin.by_name(n) else { return Ok(None) };
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(Some(s))
    };
    let names: Vec<String> = parts
        .iter()
        .filter(|(n, b)| n.ends_with(".xml") && !n.contains("_rels") && b.windows(NEW_IMAGE.len()).any(|w| w == NEW_IMAGE.as_bytes()))
        .map(|(n, _)| n.clone())
        .collect();
    for name in names {
        let xml = String::from_utf8(parts.iter().find(|(n, _)| *n == name).map(|p| p.1.clone()).unwrap_or_default()).map_err(|e| e.to_string())?;
        let mut used: Vec<usize> =
            xml.match_indices(NEW_IMAGE).filter_map(|(k, _)| xml[k + NEW_IMAGE.len()..].split('"').next()?.parse().ok()).collect();
        used.sort_unstable();
        used.dedup();
        let (dir, file) = name.rsplit_once('/').unwrap_or(("", &name));
        let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let mut rels = read(parts, &rels_name)?.unwrap_or_else(|| {
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_string()
        });
        let mut types = read(parts, "[Content_Types].xml")?.ok_or("no content types")?;
        for k in used {
            let data = images.get(k).ok_or("a new picture without its image")?;
            let (mime, ext) = super::image::image_type(data).ok_or("a new picture of an unknown image type")?;
            if rels.contains(&format!("Id=\"{NEW_IMAGE}{k}\"")) {
                continue;
            }
            let media = format!("media/ceimage{k}.{ext}");
            let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
            rels.insert_str(
                close,
                &format!(r#"<Relationship Id="{NEW_IMAGE}{k}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{media}"/>"#),
            );
            let full = if dir.is_empty() { media } else { format!("{dir}/{media}") };
            parts.retain(|(n, _)| *n != full);
            parts.push((full, data.clone()));
            if !types.to_ascii_lowercase().contains(&format!("extension=\"{ext}\"")) {
                let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
                types.insert_str(close, &format!(r#"<Default Extension="{ext}" ContentType="{mime}"/>"#));
            }
        }
        for (n, b) in [(rels_name, rels), ("[Content_Types].xml".to_string(), types)] {
            match parts.iter_mut().find(|(m, _)| *m == n) {
                Some(p) => p.1 = b.into_bytes(),
                None => parts.push((n, b.into_bytes())),
            }
        }
    }
    Ok(())
}

/// The run content a new object writes as, for those the model can write without a source.
fn object_xml(w: &str, i: &Inline) -> Option<String> {
    match i {
        Inline::NoteRef { kind: NoteKind::Foot, id, custom: false, .. } => Some(format!("<{w}footnoteReference {w}id=\"{id}\"/>")),
        Inline::NoteRef { kind: NoteKind::End, id, custom: false, .. } => Some(format!("<{w}endnoteReference {w}id=\"{id}\"/>")),
        Inline::NoteMark(NoteKind::Foot, ..) => Some(format!("<{w}footnoteRef/>")),
        Inline::NoteMark(NoteKind::End, ..) => Some(format!("<{w}endnoteRef/>")),
        // each marker is its `fldChar`, in a run of the field's formatting.
        Inline::FieldBegin { lock, dirty, .. } => Some(format!(
            "<{w}fldChar {w}fldCharType=\"begin\"{}{}/>",
            if *lock { format!(" {w}fldLock=\"1\"") } else { String::new() },
            if *dirty { format!(" {w}dirty=\"1\"") } else { String::new() }
        )),
        Inline::FieldSep { .. } => Some(format!("<{w}fldChar {w}fldCharType=\"separate\"/>")),
        Inline::FieldEnd { .. } => Some(format!("<{w}fldChar {w}fldCharType=\"end\"/>")),
        Inline::PTab(t, _) => {
            use crate::docx::props::{Leader, TabKind};
            let align = match t.align {
                TabKind::Center => "center",
                TabKind::Right => "right",
                _ => "left",
            };
            let leader = match t.leader {
                Leader::Dot => "dot",
                Leader::Hyphen => "hyphen",
                Leader::Underscore => "underscore",
                Leader::MiddleDot => "middleDot",
                Leader::None => "none",
            };
            let to = if t.margin { "margin" } else { "indent" };
            Some(format!("<{w}ptab {w}relativeTo=\"{to}\" {w}alignment=\"{align}\" {w}leader=\"{leader}\"/>"))
        }
        _ => None,
    }
}

/// A float's new offsets and size, each only when changed: patched into its `w:drawing` in place.
#[derive(Clone, Debug)]
struct Placement {
    h: Option<f64>,
    v: Option<f64>,
    size: Option<(f64, f64)>,
    /// Wrapping (`wrap`, `wrap_text`, behind the text) and the anchor lock, when changed.
    wrap: Option<(String, String, bool)>,
    locked: Option<bool>,
    /// The alternative text (`wp:docPr/@descr`).
    alt: Option<String>,
}

impl Placement {
    fn changed(a: &Float, b: &Float) -> Option<Placement> {
        let ne = |x: f64, y: f64| (x - y).abs() > 1e-6;
        let p = Placement {
            // An aligned position that became an offset is written as one.
            h: (b.h_align.is_none() && (ne(a.h_offset, b.h_offset) || a.h_align.is_some())).then_some(b.h_offset),
            v: (b.v_align.is_none() && (ne(a.v_offset, b.v_offset) || a.v_align.is_some())).then_some(b.v_offset),
            size: (ne(a.w, b.w) || ne(a.h, b.h)).then_some((b.w, b.h)),
            wrap: (a.wrap != b.wrap || a.wrap_text != b.wrap_text || a.behind != b.behind).then(|| (b.wrap.clone(), b.wrap_text.clone(), b.behind)),
            locked: (a.locked != b.locked).then_some(b.locked),
            alt: (a.alt != b.alt).then(|| b.alt.to_string()),
        };
        (p.h.is_some() || p.v.is_some() || p.size.is_some() || p.wrap.is_some() || p.locked.is_some() || p.alt.is_some()).then_some(p)
    }

    fn apply(&self, d: &str) -> String {
        let emu = |pt: f64| ((pt * 12700.0).round() as i64).to_string();
        let mut s = d.to_string();
        for (name, v) in [("positionH", self.h), ("positionV", self.v)] {
            if let Some(v) = v {
                set_content(&mut s, name, |p| format!("<{p}posOffset>{}</{p}posOffset>", emu(v)));
            }
        }
        if let Some((start, end)) = find_tag(&s, "anchor", 0) {
            let mut tag = s[start..end].to_string();
            if let Some((_, _, behind)) = &self.wrap {
                tag = set_attr(&tag, "behindDoc", if *behind { "1" } else { "0" });
            }
            if let Some(l) = self.locked {
                tag = set_attr(&tag, "locked", if l { "1" } else { "0" });
            }
            let p = tag[1..]
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .rsplit_once(':')
                .map_or(String::new(), |(p, _)| format!("{p}:"));
            s.replace_range(start..end, &tag);
            if let Some((wrap, text, _)) = &self.wrap {
                let x = wrap_xml(&p, wrap, text);
                let old = ["wrapSquare", "wrapTight", "wrapThrough", "wrapTopAndBottom", "wrapNone"].iter().find_map(|n| element(&s, n));
                match old {
                    Some(r) => s.replace_range(r, &x),
                    // None: after the extents (the schema's order: ..., extent, effectExtent, wrap, docPr, ...).
                    None => {
                        if let Some(at) = element(&s, "effectExtent").or_else(|| element(&s, "extent")).map(|r| r.end) {
                            s.insert_str(at, &x);
                        }
                    }
                }
            }
        }
        if let Some(alt) = &self.alt
            && let Some((start, end)) = find_tag(&s, "docPr", 0)
        {
            let esc = alt.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
            let tag = set_attr(&s[start..end], "descr", &esc);
            s.replace_range(start..end, &tag);
        }
        if let Some((w, h)) = self.size {
            // The anchor's extent and the shape's own size (`a:ext` with cx; extension lists' `a:ext` have none).
            for name in ["extent", "ext"] {
                let mut from = 0;
                while let Some((start, end)) = find_tag(&s, name, from) {
                    let tag = s[start..end].to_string();
                    if tag.contains(" cx=") {
                        let t = set_attr(&set_attr(&tag, "cx", &emu(w)), "cy", &emu(h));
                        s.replace_range(start..end, &t);
                        from = start + t.len();
                    } else {
                        from = end;
                    }
                }
            }
        }
        s
    }
}

/// The first start tag named `local` (any prefix) at or after `from`: its byte range.
fn find_tag(s: &str, local: &str, from: usize) -> Option<(usize, usize)> {
    let mut at = from;
    while let Some(k) = s[at..].find('<') {
        let start = at + k;
        let name_end = s[start + 1..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').map(|e| start + 1 + e)?;
        let name = &s[start + 1..name_end];
        if name.rsplit(':').next() == Some(local) && !name.starts_with('/') {
            let end = s[start..].find('>').map(|e| start + e + 1)?;
            return Some((start, end));
        }
        at = start + 1;
    }
    None
}

/// A drawing's wrap element (`p`: the drawing prefix with its colon).
fn wrap_xml(p: &str, wrap: &str, text: &str) -> String {
    match wrap {
        "square" => format!("<{p}wrapSquare wrapText=\"{text}\"/>"),
        "tight" | "through" => {
            // A rectangle the size of the object (21600 is the whole of each side).
            let n = if wrap == "tight" { "wrapTight" } else { "wrapThrough" };
            format!(
                "<{p}{n} wrapText=\"{text}\"><{p}wrapPolygon edited=\"0\"><{p}start x=\"0\" y=\"0\"/><{p}lineTo x=\"0\" y=\"21600\"/><{p}lineTo x=\"21600\" y=\"21600\"/><{p}lineTo x=\"21600\" y=\"0\"/><{p}lineTo x=\"0\" y=\"0\"/></{p}wrapPolygon></{p}{n}>"
            )
        }
        "topAndBottom" => format!("<{p}wrapTopAndBottom/>"),
        _ => format!("<{p}wrapNone/>"),
    }
}

/// The first element named `local` (any prefix): its byte range, start tag to end tag.
fn element(s: &str, local: &str) -> Option<Range<usize>> {
    let (start, end) = find_tag(s, local, 0)?;
    if s[..end].ends_with("/>") {
        return Some(start..end);
    }
    let name = s[start + 1..].split(|c: char| c.is_whitespace() || c == '>').next()?;
    let close = format!("</{name}>");
    s[end..].find(&close).map(|k| start..end + k + close.len())
}

/// Replaces the content of the first element named `local` with `with(prefix)` (its prefix, with the colon).
fn set_content(s: &mut String, local: &str, with: impl Fn(&str) -> String) {
    let Some((start, end)) = find_tag(s, local, 0) else { return };
    let name = s[start + 1..].split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("").to_string();
    let prefix = name.rsplit_once(':').map_or(String::new(), |(p, _)| format!("{p}:"));
    let Some(close) = s[end..].find(&format!("</{name}>")).map(|k| end + k) else { return };
    s.replace_range(end..close, &with(&prefix));
}

fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    match tag.find(&key) {
        Some(k) => {
            let v0 = k + key.len();
            let v1 = tag[v0..].find('"').map_or(tag.len(), |e| v0 + e);
            format!("{}{value}{}", &tag[..v0], &tag[v1..])
        }
        None => {
            let at = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
            format!("{} {name}=\"{value}\"{}", &tag[..at], &tag[at..])
        }
    }
}

fn twips(pt: f64) -> i64 {
    (pt * 20.0).round() as i64
}

fn width(w: &str, name: &str, v: &Width) -> String {
    let (n, kind) = match v {
        Width::Auto => (0, "auto"),
        Width::Nil => (0, "nil"),
        Width::Dxa(pt) => (twips(*pt), "dxa"),
        Width::Pct(f) => ((f * 5000.0).round() as i64, "pct"),
    };
    format!("<{w}{name} {w}w=\"{n}\" {w}type=\"{kind}\"/>")
}

fn border(w: &str, name: &str, b: &Option<Border>) -> String {
    let Some(b) = b else { return String::new() };
    let color = b.color.map_or("auto".to_string(), |c| format!("{c:06X}"));
    format!(
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\" {w}color=\"{color}\"/>",
        b.style,
        (b.width * 8.0).round() as i64,
        b.space.round() as i64
    )
}

fn shading(w: &str, fill: Option<u32>) -> String {
    fill.map_or(String::new(), |f| format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{f:06X}\"/>"))
}

/// A table's `w:tblPr` from the model (its resolved properties written as direct ones, its tracked change last), in
/// CT_TblPr order.
fn tbl_pr(w: &str, t: &Table, people: &People) -> String {
    let mut s = format!("<{w}tblPr>");
    if let Some(id) = &t.style_id {
        s.push_str(&format!("<{w}tblStyle {w}val=\"{}\"/>", props::esc(id)));
    }
    s.push_str(&width(w, "tblW", &t.width));
    match t.jc {
        Jc::Center => s.push_str(&format!("<{w}jc {w}val=\"center\"/>")),
        Jc::Right => s.push_str(&format!("<{w}jc {w}val=\"right\"/>")),
        _ => {}
    }
    if t.cell_spacing != 0.0 {
        s.push_str(&width(w, "tblCellSpacing", &Width::Dxa(t.cell_spacing)));
    }
    if t.ind != 0.0 {
        s.push_str(&width(w, "tblInd", &Width::Dxa(t.ind)));
    }
    if t.borders.iter().any(Option::is_some) {
        s.push_str(&format!("<{w}tblBorders>"));
        for (b, name) in t.borders.iter().zip(["top", "left", "bottom", "right", "insideH", "insideV"]) {
            s.push_str(&border(w, name, b));
        }
        s.push_str(&format!("</{w}tblBorders>"));
    }
    if t.fixed {
        s.push_str(&format!("<{w}tblLayout {w}type=\"fixed\"/>"));
    }
    if let Some(l) = t.look {
        let bit = |on: bool| if on { 1 } else { 0 };
        s.push_str(&format!(
            "<{w}tblLook {w}firstRow=\"{}\" {w}lastRow=\"{}\" {w}firstColumn=\"{}\" {w}lastColumn=\"{}\" {w}noHBand=\"{}\" {w}noVBand=\"{}\"/>",
            bit(l.first_row),
            bit(l.last_row),
            bit(l.first_col),
            bit(l.last_col),
            bit(l.no_h_band),
            bit(l.no_v_band)
        ));
    }
    if let Some(c) = &t.change {
        s.push_str(&revisions::table_change(w, "tblPrChange", "tblPr", c, people));
    }
    s.push_str(&format!("</{w}tblPr>"));
    s
}

/// A row's `w:trPr` from the model, in CT_TrPr order; empty when it has nothing to say.
fn tr_pr(w: &str, r: &Row, people: &People) -> String {
    let mut s = String::new();
    if r.grid_before > 0 {
        s.push_str(&format!("<{w}gridBefore {w}val=\"{}\"/>", r.grid_before));
    }
    if r.grid_after > 0 {
        s.push_str(&format!("<{w}gridAfter {w}val=\"{}\"/>", r.grid_after));
    }
    if r.cant_split {
        s.push_str(&format!("<{w}cantSplit/>"));
    }
    if let Some((h, rule)) = &r.height {
        let rule = if rule == "atLeast" { String::new() } else { format!(" {w}hRule=\"{rule}\"") };
        s.push_str(&format!("<{w}trHeight {w}val=\"{}\"{rule}/>", twips(*h)));
    }
    if r.header {
        s.push_str(&format!("<{w}tblHeader/>"));
    }
    for (_, tag) in revisions::row_marks(w, &r.rev, people) {
        s.push_str(&tag);
    }
    if s.is_empty() { s } else { format!("<{w}trPr>{s}</{w}trPr>") }
}

/// A cell's `w:tcPr` from the model, in CT_TcPr order.
fn tc_pr(w: &str, c: &Cell, people: &People) -> String {
    let mut s = format!("<{w}tcPr>");
    if let Some(width_) = &c.width {
        s.push_str(&width(w, "tcW", width_));
    }
    if c.span > 1 {
        s.push_str(&format!("<{w}gridSpan {w}val=\"{}\"/>", c.span));
    }
    match c.v_merge {
        VMerge::Restart => s.push_str(&format!("<{w}vMerge {w}val=\"restart\"/>")),
        VMerge::Continue => s.push_str(&format!("<{w}vMerge/>")),
        VMerge::None => {}
    }
    if c.top.is_some() || c.left.is_some() || c.bottom.is_some() || c.right.is_some() {
        s.push_str(&format!("<{w}tcBorders>"));
        s.push_str(&border(w, "top", &c.top));
        s.push_str(&border(w, "left", &c.left));
        s.push_str(&border(w, "bottom", &c.bottom));
        s.push_str(&border(w, "right", &c.right));
        s.push_str(&format!("</{w}tcBorders>"));
    }
    s.push_str(&shading(w, c.shading));
    if c.no_wrap {
        s.push_str(&format!("<{w}noWrap/>"));
    }
    let [t, l, b, r] = c.margins;
    s.push_str(&format!(
        "<{w}tcMar>{}{}{}{}</{w}tcMar>",
        width(w, "top", &Width::Dxa(t)),
        width(w, "left", &Width::Dxa(l)),
        width(w, "bottom", &Width::Dxa(b)),
        width(w, "right", &Width::Dxa(r))
    ));
    if c.text_direction != "lrTb" {
        s.push_str(&format!("<{w}textDirection {w}val=\"{}\"/>", c.text_direction));
    }
    if c.v_align != "top" {
        s.push_str(&format!("<{w}vAlign {w}val=\"{}\"/>", c.v_align));
    }
    for (_, tag) in revisions::cell_marks(w, &c.rev, people) {
        s.push_str(&tag);
    }
    s.push_str(&format!("</{w}tcPr>"));
    s
}

/// A start tag without the attribute `*:name="..."`.
fn strip_attr(tag: &str, name: &str) -> String {
    let pat = format!(":{name}=\"");
    let Some(at) = tag.find(&pat) else { return tag.to_string() };
    let start = tag[..at].rfind(char::is_whitespace).unwrap_or(at);
    let end = tag[at + pat.len()..].find('"').map_or(tag.len(), |e| at + pat.len() + e + 1);
    format!("{}{}", &tag[..start], &tag[end..])
}

/// A control's start markup and its content's start tag: as read while its properties are, else written
/// from them (`w:sdtPr` children in schema order, the settings the editor doesn't change as read).
pub(crate) fn control_open(w: &str, id: ControlId, c: &Control) -> String {
    if c.as_read() {
        return c.open.clone();
    }
    let p = &c.props;
    let esc = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
    let rest = |name: &str| p.rest.iter().filter(|(n, _)| n == name).map(|(_, x)| x.as_str()).collect::<String>();
    let mut pr = rest("rPr");
    if let Some(a) = &p.alias {
        pr.push_str(&format!("<{w}alias {w}val=\"{}\"/>", esc(a)));
    }
    if let Some(t) = &p.tag {
        pr.push_str(&format!("<{w}tag {w}val=\"{}\"/>", esc(t)));
    }
    let given_id = rest("id");
    if given_id.is_empty() {
        // Word's `w:id`: a signed 32-bit number, unique in the document.
        pr.push_str(&format!("<{w}id {w}val=\"{}\"/>", (id.0 % 0x7fff_ffff) as i64 + 1));
    } else {
        pr.push_str(&given_id);
    }
    if let Some(v) = p.lock.val() {
        pr.push_str(&format!("<{w}lock {w}val=\"{v}\"/>"));
    }
    if let Some(x) = &p.placeholder {
        pr.push_str(x);
    }
    pr.push_str(&rest("temporary"));
    if p.showing_placeholder {
        pr.push_str(&format!("<{w}showingPlcHdr/>"));
    }
    if let Some(x) = &p.binding {
        pr.push_str(x);
    }
    for (n, x) in &p.rest {
        if !matches!(n.as_str(), "rPr" | "id" | "temporary") {
            pr.push_str(x);
        }
    }
    let w15 = "http://schemas.microsoft.com/office/word/2012/wordml";
    let kind = match (&p.kind_xml, &p.kind) {
        (Some(x), _) => x.clone(),
        (None, ControlKind::PlainText { multi_line: true }) => format!("<{w}text {w}multiLine=\"1\"/>"),
        (None, ControlKind::PlainText { .. }) => format!("<{w}text/>"),
        (None, ControlKind::RichText) => format!("<{w}richText/>"),
        (None, ControlKind::Picture) => format!("<{w}picture/>"),
        (None, ControlKind::Date { format, lid }) => format!(
            "<{w}date><{w}dateFormat {w}val=\"{}\"/>{}<{w}storeMappedDataAs {w}val=\"dateTime\"/><{w}calendar {w}val=\"gregorian\"/></{w}date>",
            esc(format),
            lid.as_ref().map_or(String::new(), |l| format!("<{w}lid {w}val=\"{}\"/>", esc(l)))
        ),
        (None, ControlKind::Group) => format!("<{w}group/>"),
        (None, ControlKind::RepeatingSection) => format!("<w15:repeatingSection xmlns:w15=\"{w15}\"/>"),
        (None, ControlKind::RepeatingItem) => format!("<w15:repeatingSectionItem xmlns:w15=\"{w15}\"/>"),
        (None, ControlKind::Checkbox { .. } | ControlKind::DropDown | ControlKind::Other) => String::new(),
    };
    format!("<{w}sdt><{w}sdtPr>{pr}{kind}</{w}sdtPr><{w}sdtContent>")
}

/// The end markup matching [`control_open`].
pub(crate) fn control_close(w: &str, c: &Control) -> String {
    if c.as_read() { c.close.clone() } else { format!("</{w}sdtContent></{w}sdt>") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_keeps_common_units() {
        let k = |s: &str| s.chars().map(|c| c as u64).collect::<Vec<_>>();
        let m = diff(&k("hello world"), &k("help the world"));
        let kept: String = m.iter().zip("hello world".chars()).filter(|(m, _)| m.is_some()).map(|(_, c)| c).collect();
        assert_eq!(kept, "hel world");
        assert_eq!(diff(&k("abc"), &k("")), vec![None; 3]);
        assert_eq!(strip_attr(r#"<w:p w14:paraId="1A" w:rsidR="00"><"#, "paraId"), r#"<w:p w:rsidR="00"><"#);
    }

    #[test]
    fn tracking_is_set_where_the_schema_puts_it() {
        let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
        let off = format!(r#"<w:settings {ns}><w:zoom w:percent="100"/><w:revisionView w:markup="0"/><w:defaultTabStop w:val="720"/></w:settings>"#);
        let on = with_track_revisions(&off, true).expect("set");
        assert_eq!(on, off.replace(r#"w:markup="0"/>"#, r#"w:markup="0"/><w:trackRevisions/>"#));
        assert!(tracks_revisions(&on));
        let protected =
            format!(r#"<w:settings {ns}><w:trackRevisions/><w:documentProtection w:edit="trackedChanges" w:enforcement="1"/></w:settings>"#);
        assert_eq!(
            with_track_revisions(&protected, false).expect("removed"),
            format!(r#"<w:settings {ns}><w:documentProtection w:edit="trackedChanges" w:enforcement="1"/></w:settings>"#)
        );
        assert_eq!(
            with_track_revisions(&format!("<w:settings {ns}/>"), true).expect("set"),
            format!("<w:settings {ns}><w:trackRevisions/></w:settings>")
        );
        assert!(!tracks_revisions(&format!(r#"<w:settings {ns}><w:trackRevisions w:val="0"/></w:settings>"#)));
    }
}
