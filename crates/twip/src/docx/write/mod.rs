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
use super::xml::{MC, W, W14, attr, child, is, is_w};
use super::*;

mod comments;
pub(crate) mod drawing;
mod fonts;
mod glossary;
mod numbering;
pub mod props;
mod revisions;
mod styles;

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
    if let Some(xml) = main_xml(doc)? {
        parts.push((src.main.clone(), xml.into_bytes()));
    }
    for (name, (xml, orig)) in &src.stories {
        let now = doc.stories.get(name).map_or(&[][..], Vec::as_slice);
        if let Some(xml) = part_xml(xml, &boxes(&[orig], &[now]), &[(orig, now)], &doc.controls, &gone_controls(doc), &doc.people)? {
            parts.push((name.clone(), xml.into_bytes()));
        }
    }
    glossary::write(doc, src, &mut parts)?;
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
        let made = new_notes_part(&src.package, &src.main, foot, notes, &parts, &doc.controls, &doc.people)?;
        for (n, b) in made {
            match parts.iter_mut().find(|(m, _)| *m == n) {
                Some(p) => p.1 = b,
                None => parts.push((n, b)),
            }
        }
    }
    section_pages(doc, &mut parts)?;
    new_stories(doc, &mut parts)?;
    numbering::numbering(doc, &mut parts)?;
    styles::styles(doc, &mut parts)?;
    fonts::font_renames(doc, &mut parts)?;
    filled_placeholders(doc, &mut parts)?;
    review_parts(doc, &mut parts)?;
    settings_part(doc, &mut parts)?;
    page_background(doc, &mut parts)?;
    add_images(&src.package, &doc.images, &mut parts)?;
    unique_ids(src, &mut parts)?;
    data_parts(doc, &mut parts)?;
    property_parts(doc, &mut parts)?;
    Ok(parts)
}

/// Each `wp:docPr` id of the parts written that another drawing in the package has (a new or copied drawing's, a
/// nested one's, or one an earlier save gave), made one no drawing has: Word repairs a document with duplicates.
/// Bookmark ids too, a part's start and end alike, when another part has them (each part numbers its new ones).
fn unique_ids(src: &Source, parts: &mut [(String, Vec<u8>)]) -> Result<(), String> {
    let content = |n: &str| n.ends_with(".xml") && !n.contains("_rels/");
    let holds = |b: &[u8], tags: IdTags| tags.1.iter().any(|n| b.windows(n.len()).any(|w| w == n.as_bytes()));
    let wanted: Vec<IdTags> =
        [DRAWING_TAGS, BOOKMARK_TAGS].into_iter().filter(|tags| parts.iter().any(|(n, b)| content(n) && holds(b, *tags))).collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let mut taken: Vec<std::collections::HashSet<i64>> = vec![Default::default(); wanted.len()];
    let mut scan = |xml: &str| {
        for (tags, t) in wanted.iter().zip(&mut taken) {
            if holds(xml.as_bytes(), *tags) {
                t.extend(tag_ids(xml, *tags).into_iter().map(|(_, id)| id));
            }
        }
    };
    // The parts kept as read: those the source holds, else from the package.
    let mut held: HashMap<&str, &str> = HashMap::from([(src.main.as_str(), src.xml.as_str())]);
    held.extend(src.stories.iter().map(|(n, (x, _))| (n.as_str(), x.as_str())));
    held.extend(src.notes.iter().map(|(n, x, _)| (n.as_str(), x.as_str())));
    if let Some(g) = &src.glossary {
        held.insert(g.part.as_str(), g.xml.as_str());
    }
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package)).map_err(|e| e.to_string())?;
    let kept: Vec<String> =
        zin.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).filter(|n| content(n) && !parts.iter().any(|(m, _)| m == n)).collect();
    for name in kept {
        match held.get(name.as_str()) {
            Some(x) => scan(x),
            None => {
                let Ok(f) = zin.by_name(&name) else { continue };
                if let Ok(x) = crate::docx::package::text(f, &name) {
                    scan(&x);
                }
            }
        }
    }
    for (tags, taken) in wanted.into_iter().zip(&mut taken) {
        let paired = tags.1 == BOOKMARK_TAGS.1;
        let mut next = 1i64;
        let mut free = |taken: &mut std::collections::HashSet<i64>, mine: &std::collections::HashSet<i64>| -> Result<i64, String> {
            while taken.contains(&next) || mine.contains(&next) {
                next = next.checked_add(1).filter(|n| *n <= i64::from(u32::MAX)).ok_or("no id left")?;
            }
            taken.insert(next);
            Ok(next)
        };
        for (name, bytes) in parts.iter_mut().filter(|(n, b)| content(n) && holds(b, tags)) {
            let xml = std::str::from_utf8(bytes).map_err(|e| format!("{name}: {e}"))?;
            let ids = tag_ids(xml, tags);
            let mut fix = Vec::new();
            if paired {
                // The part's ids another part has, each given one neither has (its start's and end's alike).
                let mine: std::collections::HashSet<i64> = ids.iter().map(|(_, id)| *id).collect();
                let mut map: HashMap<i64, i64> = HashMap::new();
                for (_, id) in &ids {
                    if taken.contains(id) && !map.contains_key(id) {
                        let n = free(taken, &mine)?;
                        map.insert(*id, n);
                    }
                }
                taken.extend(mine.into_iter().filter(|id| !map.contains_key(id)));
                fix.extend(ids.into_iter().filter_map(|(r, id)| Some((r, *map.get(&id)?))));
            } else {
                let none = std::collections::HashSet::new();
                for (r, id) in ids {
                    if !taken.insert(id) {
                        fix.push((r, free(taken, &none)?));
                    }
                }
            }
            if !fix.is_empty() {
                let mut xml = xml.to_string();
                for (r, id) in fix.into_iter().rev() {
                    xml.replace_range(r, &id.to_string());
                }
                *bytes = xml.into_bytes();
            }
        }
    }
    Ok(())
}

/// What [`tag_ids`] looks for: elements of a namespace and local names, and the namespace of their `id` attribute
/// (`None`: unprefixed).
type IdTags = (&'static str, &'static [&'static str], Option<&'static str>);

/// `wp:docPr/@id`.
const DRAWING_TAGS: IdTags = (super::xml::WP, &["docPr"], None);

/// `w:bookmarkStart/@w:id` and `w:bookmarkEnd/@w:id`.
const BOOKMARK_TAGS: IdTags = (W, &["bookmarkStart", "bookmarkEnd"], Some(W));

/// The `id` of each start tag `tags` names in XML, its value read as XML reads it (references resolved), with its value's
/// byte range, in order. Prefixes are resolved by the declarations in scope; one the XML doesn't declare (a fragment's,
/// bound around it) is taken to be the one wanted. Comments, CDATA sections, processing instructions and declarations
/// hold no tags.
fn tag_ids(xml: &str, (ns, names, id_ns): IdTags) -> Vec<(Range<usize>, i64)> {
    use std::borrow::Cow;
    fn decode(v: &str) -> Cow<'_, str> {
        if v.contains('&') { Cow::Owned(crate::docx::binding::unescape(v)) } else { Cow::Borrowed(v) }
    }
    let mut out = Vec::new();
    // The declarations in scope (prefix, URI; "" the default), and where each open element's start.
    let mut decls: Vec<(&str, Cow<str>)> = Vec::new();
    let mut scopes: Vec<usize> = Vec::new();
    let mut attrs: Vec<(&str, &str, usize)> = Vec::new();
    let mut at = 0;
    'tags: while let Some(lt) = xml[at..].find('<').map(|k| at + k) {
        let rest = &xml[lt..];
        for (open, close) in [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>"), ("<!", ">")] {
            if let Some(body) = rest.strip_prefix(open) {
                at = body.find(close).map_or(xml.len(), |e| lt + open.len() + e + close.len());
                continue 'tags;
            }
        }
        if rest.starts_with("</") {
            if let Some(n) = scopes.pop() {
                decls.truncate(n);
            }
            at = lt + 2;
            continue;
        }
        let Some(gt) = crate::docx::binding::tag_end(rest).map(|e| lt + e) else { break };
        let head = &xml[lt..gt];
        let closed = head.ends_with('/');
        let head = head.trim_end_matches('/');
        let qname = &head[1..head.find(|c: char| c.is_ascii_whitespace()).unwrap_or(head.len())];
        attrs.clear();
        let mark = decls.len();
        for (name, value, r) in crate::docx::binding::tag_attrs(head) {
            if let Some(prefix) = name.strip_prefix("xmlns").filter(|p| p.is_empty() || p.starts_with(':')) {
                decls.push((prefix.strip_prefix(':').unwrap_or(prefix), decode(value)));
            } else {
                // The value's start: its quote's after the `=`.
                let v0 = lt + r.end - 1 - value.len();
                attrs.push((name, value, v0));
            }
        }
        let resolves = |prefix: &str, uri: &str| decls.iter().rev().find(|(p, _)| *p == prefix).is_none_or(|(_, u)| u == uri);
        let (prefix, local) = qname.split_once(':').unwrap_or(("", qname));
        if names.contains(&local) && resolves(prefix, ns) {
            for &(name, value, v0) in &attrs {
                let found = match (name.split_once(':'), id_ns) {
                    (None, None) => name == "id",
                    (Some((p, "id")), Some(uri)) => resolves(p, uri),
                    _ => false,
                };
                if found && let Ok(id) = decode(value).trim().parse() {
                    out.push((v0..v0 + value.len(), id));
                }
            }
        }
        if closed {
            decls.truncate(mark);
        } else {
            scopes.push(mark);
        }
        at = gt + 1;
    }
    out
}

/// The document properties parts, when the properties changed or anything else did: an edited document's last saved
/// time and revision follow.
fn property_parts(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use super::properties as p;
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let part = |rel: &str, parts: &[(String, Vec<u8>)]| package::related(&src.package, parts, "", rel);
    let (core, app, custom) = (part(p::CORE, parts)?, part(p::APP, parts)?, part(p::CUSTOM, parts)?);
    let read_of =
        |n: &Option<String>, parts: &[(String, Vec<u8>)]| n.as_ref().map(|n| package::read(&src.package, parts, n)).transpose().map(Option::flatten);
    let (core_x, app_x, custom_x) = (read_of(&core, parts)?, read_of(&app, parts)?, read_of(&custom, parts)?);
    let read = p::read(core_x.as_deref(), app_x.as_deref(), custom_x.as_deref());
    let mut now = doc.properties.clone();
    if !parts.is_empty() && core.is_some() {
        let secs = crate::clock::unix_secs();
        now.set_core("modified", Some(crate::docx::review::Timestamp::from_utc(secs).raw.to_string()));
        let rev = read.core("revision").and_then(|r| r.parse::<u64>().ok()).unwrap_or(0) + 1;
        if now.core("revision") == read.core("revision") {
            now.set_core("revision", Some(rev.to_string()));
        }
    }
    if now.core != read.core {
        let name = match &core {
            Some(n) => n.clone(),
            None => package::free_name(&src.package, parts, "docProps/core.xml")?,
        };
        if core.is_none() {
            package::add_part(&src.package, parts, "", &name, "application/vnd.openxmlformats-package.core-properties+xml", p::CORE)?;
        }
        package::set(parts, &name, p::core_xml(core_x.as_deref(), &now).into_bytes());
    }
    if now.app != read.app {
        let name = match &app {
            Some(n) => n.clone(),
            None => package::free_name(&src.package, parts, "docProps/app.xml")?,
        };
        if app.is_none() {
            package::add_part(&src.package, parts, "", &name, "application/vnd.openxmlformats-officedocument.extended-properties+xml", p::APP)?;
        }
        package::set(parts, &name, p::app_xml(app_x.as_deref(), &now).into_bytes());
    }
    if now.custom != read.custom {
        let name = match &custom {
            Some(n) => n.clone(),
            None => package::free_name(&src.package, parts, "docProps/custom.xml")?,
        };
        if custom.is_none() {
            package::add_part(&src.package, parts, "", &name, "application/vnd.openxmlformats-officedocument.custom-properties+xml", p::CUSTOM)?;
        }
        package::set(parts, &name, p::custom_xml(custom_x.as_deref(), &now).into_bytes());
    }
    Ok(())
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

/// CT_Settings' children from `w:defaultTabStop` on, as far as the settings written go, in order.
const TAB_AND_HYPHENATION: &[&str] = &[
    "defaultTabStop",
    "autoHyphenation",
    "consecutiveHyphenLimit",
    "hyphenationZone",
    "doNotHyphenateCaps",
    "showEnvelope",
    "summaryLength",
    "clickAndTypeStyle",
    "defaultTableStyle",
    "evenAndOddHeaders",
    "bookFoldRevPrinting",
    "bookFoldPrinting",
    "bookFoldPrintingSheets",
    "drawingGridHorizontalSpacing",
    "drawingGridVerticalSpacing",
    "displayHorizontalDrawingGridEvery",
    "displayVerticalDrawingGridEvery",
    "doNotUseMarginsForDrawingGridOrigin",
    "drawingGridHorizontalOrigin",
    "drawingGridVerticalOrigin",
    "doNotShadeFormData",
    "noPunctuationKerning",
    "characterSpacingControl",
    "printTwoOnOne",
    "strictFirstAndLastChars",
    "noLineBreaksAfter",
    "noLineBreaksBefore",
    "savePreviewPicture",
    "doNotValidateAgainstSchema",
    "saveInvalidXml",
    "ignoreMixedContent",
    "alwaysShowPlaceholderText",
    "doNotDemarcateInvalidXml",
    "saveXmlDataOnly",
    "useXSLTWhenSaving",
    "saveThroughXslt",
    "showXMLTags",
    "alwaysMergeEmptyNamespace",
    "updateFields",
    "hdrShapeDefaults",
    "footnotePr",
    "endnotePr",
    "compat",
    "docVars",
];

/// The settings part with the default tab spacing and the hyphenation settings, where they changed since reading; a
/// document without one gets one when a setting is written.
fn settings_part(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let existing = package::related(&src.package, parts, &src.main, SETTINGS)?;
    let xml = match &existing {
        Some(n) => package::read(&src.package, parts, n)?.unwrap_or_default(),
        None => format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:settings xmlns:w="{W}"/>"#
        ),
    };
    if xml.is_empty() {
        return Ok(());
    }
    let (read, now) = (super::reader::read_settings(Some(&xml)), &doc.settings);
    let w = {
        let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
        match dom.root_element().lookup_prefix(W) {
            Some(p) if !p.is_empty() => format!("{p}:"),
            _ => String::new(),
        }
    };
    let mut out = xml.clone();
    let tw = |v: f64| (v * 20.0).round() as i64;
    if (read.default_tab - now.default_tab).abs() >= 0.025 {
        set_setting(&mut out, "defaultTabStop", Some(format!("<{w}defaultTabStop {w}val=\"{}\"/>", tw(now.default_tab))))?;
    }
    if read.auto_hyphenation != now.auto_hyphenation {
        set_setting(&mut out, "autoHyphenation", now.auto_hyphenation.then(|| format!("<{w}autoHyphenation/>")))?;
    }
    if read.consecutive_hyphen_limit != now.consecutive_hyphen_limit {
        let el = (now.consecutive_hyphen_limit > 0).then(|| format!("<{w}consecutiveHyphenLimit {w}val=\"{}\"/>", now.consecutive_hyphen_limit));
        set_setting(&mut out, "consecutiveHyphenLimit", el)?;
    }
    if tw(read.hyphenation_zone) != tw(now.hyphenation_zone) {
        set_setting(&mut out, "hyphenationZone", Some(format!("<{w}hyphenationZone {w}val=\"{}\"/>", tw(now.hyphenation_zone))))?;
    }
    if read.display_background_shape != now.display_background_shape {
        set_setting(&mut out, "displayBackgroundShape", now.display_background_shape.then(|| format!("<{w}displayBackgroundShape/>")))?;
    }
    if read.do_not_hyphenate_caps != now.do_not_hyphenate_caps {
        set_setting(&mut out, "doNotHyphenateCaps", now.do_not_hyphenate_caps.then(|| format!("<{w}doNotHyphenateCaps/>")))?;
    }
    // Endnotes at each section's end: `w:endnotePr/w:pos`, its note list kept.
    if read.endnotes_sect_end != now.endnotes_sect_end {
        let dom = roxmltree::Document::parse(&out).map_err(|e| e.to_string())?;
        let pr = dom.root_element().children().find(|c| is_w(*c, "endnotePr")).map(|n| (n.range(), child(n, "pos").map(|p| p.range())));
        drop(dom);
        let pos = now.endnotes_sect_end.then(|| format!("<{w}pos {w}val=\"sectEnd\"/>"));
        match pr {
            Some((_, Some(p))) => out.replace_range(p, pos.as_deref().unwrap_or("")),
            Some((r, None)) => {
                if let Some(pos) = pos {
                    let x = out[r.clone()].to_string();
                    let new = match x.strip_suffix("/>") {
                        Some(open) => format!("{open}>{pos}</{w}endnotePr>"),
                        None => x.replacen('>', &format!(">{pos}"), 1),
                    };
                    out.replace_range(r, &new);
                }
            }
            None => set_setting(&mut out, "endnotePr", pos.map(|p| format!("<{w}endnotePr>{p}</{w}endnotePr>")))?,
        }
    }
    let same = match (&read.protection, &now.protection) {
        (Some(a), Some(b)) => a.same(b),
        (a, b) => a.is_none() && b.is_none(),
    };
    if !same {
        set_setting(&mut out, "documentProtection", now.protection.as_ref().map(|p| p.xml(&w)))?;
    }
    if read.variables != now.variables {
        let vars: String =
            now.variables.iter().map(|(n, v)| format!("<{w}docVar {w}name=\"{}\" {w}val=\"{}\"/>", props::esc(n), props::esc(v))).collect();
        set_setting(&mut out, "docVars", (!vars.is_empty()).then(|| format!("<{w}docVars>{vars}</{w}docVars>")))?;
    }
    if read.default_table_style != now.default_table_style {
        let el = now.default_table_style.as_ref().map(|s| format!("<{w}defaultTableStyle {w}val=\"{}\"/>", props::esc(s)));
        set_setting(&mut out, "defaultTableStyle", el)?;
    }
    for (name, was, is) in [
        ("evenAndOddHeaders", read.even_and_odd, now.even_and_odd),
        ("mirrorMargins", read.mirror_margins, now.mirror_margins),
        ("gutterAtTop", read.gutter_at_top, now.gutter_at_top),
        ("doNotShadeFormData", read.no_form_shading, now.no_form_shading),
        ("bordersDoNotSurroundHeader", read.borders_do_not_surround_header, now.borders_do_not_surround_header),
        ("bordersDoNotSurroundFooter", read.borders_do_not_surround_footer, now.borders_do_not_surround_footer),
        ("bookFoldPrinting", read.book_fold == Some(false), now.book_fold == Some(false)),
        ("bookFoldRevPrinting", read.book_fold == Some(true), now.book_fold == Some(true)),
    ] {
        if was != is {
            set_setting(&mut out, name, is.then(|| format!("<{w}{name}/>")))?;
        }
    }
    // A new part: tracking too, which the review parts set only in a part there is.
    if existing.is_none() && now.track_revisions {
        out = with_track_revisions(&out, true)?;
    }
    if out == xml {
        return Ok(());
    }
    let name = match existing {
        Some(n) => n,
        None => {
            let name = src.main.rsplit_once('/').map_or_else(|| "settings.xml".into(), |(dir, _)| format!("{dir}/settings.xml"));
            let name = package::free_name(&src.package, parts, &name)?;
            let ct = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
            package::add_part(&src.package, parts, &src.main, &name, ct, SETTINGS)?;
            name
        }
    };
    package::set(parts, &name, out.into_bytes());
    Ok(())
}

/// The main part's `w:background` when the page background changed since reading: the model's colour, and its fill.
/// The fill's markup as read is kept when it still reads as the model's fill under the new colour (a gradient or
/// pattern that names no colour of its own takes the page's); else the fill is written from the model, as Word writes
/// it. An untouched background keeps its markup.
fn page_background(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    if src.page_background.0 == doc.page_colour && src.page_background.1 == doc.page_fill && src.page_theme == doc.page_colour_theme {
        return Ok(());
    }
    let mut xml = match parts.iter().find(|(n, _)| *n == src.main) {
        Some((_, b)) => String::from_utf8(b.clone()).map_err(|e| e.to_string())?,
        None => src.xml.clone(),
    };
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let prefix = |ns: &str| root.lookup_prefix(ns).filter(|p| !p.is_empty()).map(str::to_string);
    let w = prefix(W).map_or(String::new(), |p| format!("{p}:"));
    let old = root.children().find(|c| is_w(*c, "background"));
    // The picture the markup names: the one it was read with.
    let image = match &src.page_background.1 {
        Some(crate::drawing::paint::Fill::Blip(b)) => b.image,
        _ => None,
    };
    let kept = old
        .filter(|b| doc.page_fill.is_some() && crate::drawing::vml::background_fill(*b, doc.page_colour, &mut |_| image) == doc.page_fill)
        .and_then(|b| b.children().find(|c| c.is_element() && c.tag_name().name() == "background" && c.tag_name().namespace() == Some(VML)))
        .map(|v| xml[v.range()].to_string());
    // The prefixes the kept markup has from its `w:background` and not from the root, declared on the new one.
    let mut local: String = match (&kept, old) {
        (Some(_), Some(b)) => b
            .namespaces()
            .filter(|n| root.lookup_namespace_uri(n.name()) != Some(n.uri()))
            .filter_map(|n| match n.name() {
                Some(p) => Some(format!(r#" xmlns:{p}="{}""#, props::esc(n.uri()))),
                None => (!w.is_empty()).then(|| format!(r#" xmlns="{}""#, props::esc(n.uri()))),
            })
            .collect(),
        _ => String::new(),
    };
    // The prefix the new `w:background` takes: one still WordprocessingML's under those declarations.
    let w = match (&kept, old) {
        (Some(_), Some(b)) if !w.is_empty() && b.lookup_namespace_uri(Some(&w[..w.len() - 1])) != Some(W) => {
            match b.namespaces().find_map(|n| n.name().filter(|_| n.uri() == W)) {
                Some(p) => format!("{p}:"),
                None => {
                    let free = (1..).map(|k| format!("w{k}")).find(|p| b.lookup_namespace_uri(Some(p)).is_none()).unwrap_or_default();
                    local.push_str(&format!(r#" xmlns:{free}="{W}""#));
                    format!("{free}:")
                }
            }
        }
        _ => w,
    };
    let at = match old {
        Some(b) => b.range(),
        None => {
            let body = root.children().find(|c| is_w(*c, "body")).ok_or("no body")?;
            body.range().start..body.range().start
        }
    };
    // The VML, Office and relationship prefixes the root declares; those it lacks declared on it, by a prefix it leaves
    // free.
    let mut declare = String::new();
    let mut named = |ns: &str, want: &str| match prefix(ns) {
        Some(p) => p,
        None => {
            let free = std::iter::once(want.to_string())
                .chain((1..).map(|k| format!("{want}{k}")))
                .find(|p| root.lookup_namespace_uri(Some(p)).is_none())
                .unwrap_or_default();
            declare.push_str(&format!(r#" xmlns:{free}="{ns}""#));
            free
        }
    };
    let palette = crate::drawing::paint::Palette { theme: &doc.theme, map: &doc.colour_map };
    // A preset pattern's tile, a picture beside the main part as Word writes one.
    let tile = match (&kept, &doc.page_fill) {
        (None, Some(crate::drawing::paint::Fill::Pattern { preset, .. })) => Some(pattern_tile(src, parts, *preset)?),
        _ => None,
    };
    let fill = match kept {
        Some(k) => Some(k),
        None => match &doc.page_fill {
            Some(f) => {
                let ns = [named(VML, "v"), named(O, "o"), named(R, "r")];
                vml_background(f, palette, &ns, tile.as_deref())
            }
            None => None,
        },
    };
    let root_open = root.range().start + xml[root.range()].find(|c: char| c.is_whitespace() || c == '>').unwrap_or(0);
    drop(dom);
    let el = match (doc.page_colour, fill) {
        (None, None) => String::new(),
        (colour, fill) => {
            let mut c = format!("{w}color=\"{:06X}\"", colour.unwrap_or(0xFF_FFFF));
            if let Some(t) = doc.page_colour_theme
                && let Some(name) = crate::docx::props::theme_name(t.slot)
            {
                c.push_str(&format!(" {w}themeColor=\"{name}\""));
                for (attr, b) in [("themeTint", t.tint), ("themeShade", t.shade)] {
                    if let Some(b) = b {
                        c.push_str(&format!(" {w}{attr}=\"{b:02X}\""));
                    }
                }
            }
            match fill {
                Some(f) => format!("<{w}background {c}{local}>{f}</{w}background>"),
                None => format!("<{w}background {c}/>"),
            }
        }
    };
    xml.replace_range(at, &el);
    xml.insert_str(root_open, &declare);
    package::set(parts, &src.main, xml.into_bytes());
    Ok(())
}

const VML: &str = "urn:schemas-microsoft-com:vml";
const O: &str = "urn:schemas-microsoft-com:office:office";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A preset pattern's tile as a PNG part beside the main part (its set bits black, the rest white), and its
/// relationship's id: what a VML pattern fill names.
fn pattern_tile(
    src: &crate::docx::Source,
    parts: &mut Vec<(String, Vec<u8>)>,
    preset: crate::drawing::paint::PatternPreset,
) -> Result<String, String> {
    let bits = crate::drawing::patterns::tile(preset);
    let rgba: Vec<u8> = (0..64).flat_map(|k| if bits[k / 8] & (0x80 >> (k % 8)) != 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
    let png = crate::render::svg::png(8, 8, &rgba);
    let dir = src.main.rsplit_once('/').map_or(String::new(), |(d, _)| format!("{d}/"));
    let taken = |n: &str| parts.iter().any(|(m, _)| m == n) || package::read(&src.package, &[], n).ok().flatten().is_some();
    let name = (1..).map(|k| format!("{dir}media/pattern{k}.png")).find(|n| !taken(n)).unwrap_or_default();
    let rid = package::add_part(&src.package, parts, &src.main, &name, "image/png", IMAGE_REL)?;
    package::set(parts, &name, png);
    Ok(rid)
}

const IMAGE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// A page fill as a `v:background` (prefixes `[v, o, r]`), as Word writes one: its gradient, its picture tiled,
/// stretched or as a two-colour pattern, or a preset pattern from its tile picture (`tile`, its relationship). `None`
/// for a fill VML has no page form of.
fn vml_background(
    f: &crate::drawing::paint::Fill,
    palette: crate::drawing::paint::Palette,
    [v, o, r]: &[String; 3],
    tile: Option<&str>,
) -> Option<String> {
    use crate::drawing::paint::{BlipAdjust, BlipMode, Fill, GradientKind, TileFlip};
    let hex = |c: &crate::drawing::colour::Colour| {
        let c = c.resolve(palette.theme, palette.map, None);
        (format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b), f64::from(c.a) / 255.0)
    };
    let opacity = |name: &str, a: f64| if a < 1.0 { format!(r#" {name}="{}""#, (a * 1000.0).round() / 1000.0) } else { String::new() };
    let fill = match f {
        Fill::Gradient(g) if g.stops.len() >= 2 => {
            let (radial, angle) = match g.kind {
                GradientKind::Linear { angle, .. } => (false, (270.0 - angle.0 as f64 / 60000.0).rem_euclid(360.0)),
                GradientKind::Path { .. } => (true, 0.0),
            };
            // A radial gradient is read from the outside in.
            let stops: Vec<(f64, String, f64)> = if radial {
                g.stops.iter().rev().map(|(p, c)| (1.0 - p.fraction(), hex(c).0, hex(c).1)).collect()
            } else {
                g.stops.iter().map(|(p, c)| (p.fraction(), hex(c).0, hex(c).1)).collect()
            };
            let (first, last) = (&stops[0], &stops[stops.len() - 1]);
            // Two colours as Word blends them are written as the two; any other stops as a list.
            let two = crate::drawing::vml::is_two_colour(g);
            let colors = if two {
                String::new()
            } else {
                let list: Vec<String> = stops.iter().map(|(p, c, _)| format!("{} {c}", (p * 100000.0).round() / 100000.0)).collect();
                format!(r#" colors="{}""#, list.join(";"))
            };
            let mirrored = g.flip != TileFlip::None && g.tile != [crate::drawing::units::Pct(0); 4];
            let kind = if radial { "gradientRadial" } else { "gradient" };
            let mut focus = if mirrored { r#" focus="50%""#.to_string() } else { String::new() };
            if let GradientKind::Path { focus: at, .. } = g.kind {
                let f = |p: crate::drawing::units::Pct| (p.fraction() * 100_000.0).round() / 100_000.0;
                let size = |a: crate::drawing::units::Pct, b: crate::drawing::units::Pct| {
                    ((1.0 - a.fraction() - b.fraction()).max(0.0) * 100_000.0).round() / 100_000.0
                };
                focus.push_str(&format!(r#" focusposition="{},{}" focussize="{},{}""#, f(at[0]), f(at[1]), size(at[0], at[2]), size(at[1], at[3])));
            }
            let (o1, o2) = (opacity("opacity", first.2), opacity(&format!("{o}:opacity2"), last.2));
            // One colour darkened or lightened is written as the fill's own, as Word writes it.
            let rgb = |c: &crate::drawing::colour::Colour| c.resolve(palette.theme, palette.map, None).to_rgb();
            let color2 = match two.then(|| crate::drawing::vml::shade_of(rgb(&g.stops[0].1), rgb(&g.stops[g.stops.len() - 1].1))).flatten() {
                Some((light, n)) => format!("fill {}({n})", if light { "lighten" } else { "darken" }),
                None => last.1.clone(),
            };
            let angle = (angle * 100_000.0).round() / 100_000.0;
            format!(r#"<{v}:fill type="{kind}" color="{}" color2="{color2}"{colors} angle="{angle}"{focus}{o1}{o2}/>"#, first.1)
        }
        Fill::Blip(b) => {
            let image = b.image?;
            let (kind, colours) = match (&b.mode, b.adjust.iter().find_map(|a| if let BlipAdjust::Duotone(d) = a { Some(d) } else { None })) {
                (BlipMode::Tile { .. }, Some([fg, bg])) => ("pattern", format!(r#" color2="{}" color="{}""#, hex(fg).0, hex(bg).0)),
                (BlipMode::Tile { .. }, None) => ("tile", String::new()),
                (BlipMode::Stretch(_), _) => ("frame", String::new()),
            };
            format!(r#"<{v}:fill {r}:id="{NEW_IMAGE}{image}"{colours} type="{kind}"/>"#)
        }
        Fill::Pattern { preset, fg, bg } => {
            let title = props::esc(crate::drawing::patterns::title(*preset));
            let (fg, bg) = (hex(fg).0, hex(bg).0);
            return Some(format!(
                r#"<{v}:background id="_x0000_s1025" {o}:bwmode="white" fillcolor="{bg}" {o}:targetscreensize="1024,768"><{v}:fill {r}:id="{}" {o}:title="{title}" color2="{fg}" type="pattern"/></{v}:background>"#,
                tile?
            ));
        }
        _ => return None,
    };
    Some(format!(r#"<{v}:background id="_x0000_s1025" {o}:bwmode="white" {o}:targetscreensize="1024,768">{fill}</{v}:background>"#))
}

/// Replaces, removes (`el` of `None`) or adds the settings child `name`, where the schema puts it.
fn set_setting(xml: &mut String, name: &str, el: Option<String>) -> Result<(), String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = dom.root_element();
    let el = el.unwrap_or_default();
    if let Some(c) = root.children().find(|c| is_w(*c, name)) {
        let r = c.range();
        drop(dom);
        xml.replace_range(r, &el);
        return Ok(());
    }
    if el.is_empty() {
        return Ok(());
    }
    let all: Vec<&str> = BEFORE.iter().chain(BEFORE_DEFAULT_TAB).chain(TAB_AND_HYPHENATION).copied().collect();
    let before: Vec<&str> = all.iter().take_while(|n| **n != name).copied().collect();
    let after = root.children().rfind(|c| c.is_element() && before.iter().any(|b| is_w(*c, b))).map(|c| c.range().end);
    let r = root.range();
    let tag = xml[r.clone()].to_string();
    drop(dom);
    if after.is_none() && tag.ends_with("/>") {
        let tag_name = tag[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("").to_string();
        xml.replace_range(r.end - 2..r.end, &format!(">{el}</{tag_name}>"));
    } else {
        xml.insert_str(after.unwrap_or_else(|| r.start + tag.find('>').map_or(0, |e| e + 1)), &el);
    }
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
    let (dir, name) = item.rsplit_once('/').unwrap_or(("", item));
    let rels = if dir.is_empty() { format!("_rels/{name}.rels") } else { format!("{dir}/_rels/{name}.rels") };
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).ok()?;
    let x = crate::docx::package::text(zin.by_name(&rels).ok()?, &rels).ok()?;
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
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package)).map_err(|e| e.to_string())?;
    let names: Vec<String> = zin.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    let mut read = |n: &str, parts: &[(String, Vec<u8>)]| -> Result<String, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map_err(|e| e.to_string());
        }
        crate::docx::package::text(zin.by_name(n).map_err(|e| format!("{n}: {e}"))?, n)
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
            props::esc(&p.item_id)
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

/// The `w:sectPr`s that end a body's sections, in order, as reading takes them: its paragraphs' (those in block-level
/// controls and in the branch of alternate content read too; a cell's or a text box's ends no section), then its own.
fn section_props(dom: &roxmltree::Document) -> Vec<Range<usize>> {
    fn walk(container: Node, out: &mut Vec<Range<usize>>) {
        for n in container.children().filter(|c| c.is_element()) {
            if is_w(n, "p") {
                out.extend(child(n, "pPr").and_then(|p| child(p, "sectPr")).map(|s| s.range()));
            } else if is_w(n, "sdt") {
                if let Some(c) = child(n, "sdtContent") {
                    walk(c, out);
                }
            } else if is_w(n, "customXml") {
                walk(n, out);
            } else if is(n, MC, "AlternateContent")
                && let Some(c) = crate::docx::xml::choose(n)
            {
                walk(c, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Some(body) = child(dom.root_element(), "body") {
        walk(body, &mut out);
        out.extend(child(body, "sectPr").map(|s| s.range()));
    }
    out
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
    let sects = section_props(&dom);
    drop(dom);
    let mut edited: Vec<usize> = doc.edited_sections.iter().copied().filter(|&k| k < sects.len().min(doc.sections.len())).collect();
    edited.sort_unstable_by(|a, b| b.cmp(a));
    for k in edited {
        let r = sects[k].clone();
        let s = &doc.sections[k];
        let new = sect_with_page(&xml[r.clone()], &w, &s.page(), s.read.as_deref(), s.change.as_deref(), &doc.people);
        xml.replace_range(r, &new);
    }
    match parts.iter_mut().find(|(n, _)| *n == src.main) {
        Some(p) => p.1 = xml.into_bytes(),
        None => parts.push((src.main.clone(), xml.into_bytes())),
    }
    Ok(())
}

/// A `w:sectPr` with `p`'s page size, margins, columns and different first page, and `change` as its tracked change.
/// Only those differing from `was` (the page as read) when given.
fn sect_with_page(sect: &str, w: &str, p: &SectionPage, was: Option<&SectionPage>, change: Option<&SectionChange>, people: &People) -> String {
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
        Raw(Some(sect_with_page(&base, w, &c.previous, None, None, people).into()))
    });
    let existing = |s: &str, local: &str| find_tag(s, local, open_end).map(|(a, e)| s[a..e].to_string());
    let changed = |f: &dyn Fn(&SectionPage) -> bool| was.is_none_or(f);
    if changed(&|q| q.page_borders != p.page_borders) {
        put(&mut s, "pgBorders", p.page_borders_xml.clone());
    }
    let mut sz = existing(&s, "pgSz").unwrap_or_else(|| format!("<{w}pgSz/>"));
    sz = set_attr(&sz, &format!("{w}w"), &twips(p.page_w).to_string());
    sz = set_attr(&sz, &format!("{w}h"), &twips(p.page_h).to_string());
    sz = if p.page_w > p.page_h { set_attr(&sz, &format!("{w}orient"), "landscape") } else { set_attr(&sz, &format!("{w}orient"), "portrait") };
    if changed(&|q| (q.page_w, q.page_h) != (p.page_w, p.page_h)) {
        put(&mut s, "pgSz", Some(sz));
    }
    let mut mar = existing(&s, "pgMar").unwrap_or_else(|| format!("<{w}pgMar/>"));
    for (name, v) in
        [("top", p.top), ("right", p.right), ("bottom", p.bottom), ("left", p.left), ("header", p.header), ("footer", p.footer), ("gutter", p.gutter)]
    {
        mar = set_attr(&mar, &format!("{w}{name}"), &twips(v).to_string());
    }
    if changed(&|q| {
        (q.top, q.right, q.bottom, q.left, q.header, q.footer, q.gutter) != (p.top, p.right, p.bottom, p.left, p.header, p.footer, p.gutter)
    }) {
        put(&mut s, "pgMar", Some(mar));
    }
    let c = &p.columns;
    let sep = if c.separator { format!(" {w}sep=\"1\"") } else { String::new() };
    let cols = if !c.equal && c.cols.len() > 1 {
        let each: String = c.cols.iter().map(|(cw, sp)| format!("<{w}col {w}w=\"{}\" {w}space=\"{}\"/>", twips(*cw), twips(*sp))).collect();
        format!("<{w}cols {w}num=\"{}\"{sep} {w}equalWidth=\"0\">{each}</{w}cols>", c.cols.len())
    } else {
        format!("<{w}cols {w}space=\"{}\" {w}num=\"{}\"{sep}/>", twips(c.space), c.num.max(1))
    };
    if changed(&|q| q.columns != p.columns) {
        put(&mut s, "cols", Some(cols));
    }
    if changed(&|q| q.title_pg != p.title_pg) {
        put(&mut s, "titlePg", p.title_pg.then(|| format!("<{w}titlePg/>")));
    }
    // The rest only when it changed since reading (a tracked change's earlier page keeps its source's).
    if let Some(was) = was {
        if was.footnote_columns != p.footnote_columns {
            let w15 = crate::docx::xml::W15;
            let tag = (p.footnote_columns > 0).then(|| format!(r#"<w15:footnoteColumns xmlns:w15="{w15}" {w}val="{}"/>"#, p.footnote_columns));
            put(&mut s, "footnoteColumns", tag);
        }
        if was.break_kind != p.break_kind {
            let v = match p.break_kind {
                SectionBreak::NextPage => "nextPage",
                SectionBreak::Continuous => "continuous",
                SectionBreak::EvenPage => "evenPage",
                SectionBreak::OddPage => "oddPage",
                SectionBreak::NextColumn => "nextColumn",
            };
            put(&mut s, "type", Some(format!("<{w}type {w}val=\"{v}\"/>")));
        }
        if (was.pg_start, &was.pg_fmt, &was.chapter) != (p.pg_start, &p.pg_fmt, &p.chapter) {
            let mut a = String::new();
            if let Some(f) = &p.pg_fmt {
                a += &format!(" {w}fmt=\"{}\"", props::esc(f));
            }
            if let Some(n) = p.pg_start {
                a += &format!(" {w}start=\"{n}\"");
            }
            if let Some((level, sep)) = &p.chapter {
                let sep = match sep.as_str() {
                    "." => "period",
                    ":" => "colon",
                    "\u{2014}" => "emDash",
                    "\u{2013}" => "enDash",
                    _ => "hyphen",
                };
                a += &format!(" {w}chapStyle=\"{level}\" {w}chapSep=\"{sep}\"");
            }
            put(&mut s, "pgNumType", (!a.is_empty()).then(|| format!("<{w}pgNumType{a}/>")));
        }
        if was.line_numbers != p.line_numbers {
            let tag = p.line_numbers.as_ref().map(|l| {
                let distance = if l.distance > 0.0 { format!(" {w}distance=\"{}\"", twips(l.distance)) } else { String::new() };
                format!("<{w}lnNumType {w}countBy=\"{}\" {w}start=\"{}\"{distance} {w}restart=\"{}\"/>", l.count_by, l.start, props::esc(&l.restart))
            });
            put(&mut s, "lnNumType", tag);
        }
        if was.v_align != p.v_align {
            put(&mut s, "vAlign", (p.v_align != "top").then(|| format!("<{w}vAlign {w}val=\"{}\"/>", props::esc(&p.v_align))));
        }
        if was.text_flow != p.text_flow {
            let v = match p.text_flow {
                1 => Some("tbRl"),
                3 => Some("btLr"),
                4 => Some("lrTbV"),
                5 => Some("tbRlV"),
                7 => Some("tbLrV"),
                _ => None,
            };
            put(&mut s, "textDirection", v.map(|v| format!("<{w}textDirection {w}val=\"{v}\"/>")));
        }
        for (k, name) in [(0, "footnotePr"), (1, "endnotePr")] {
            if was.notes[k] != p.notes[k] {
                let n = &p.notes[k];
                let mut x = String::new();
                if k == 0 && n.beneath {
                    x += &format!("<{w}pos {w}val=\"beneathText\"/>");
                }
                if let Some(f) = &n.fmt {
                    x += &format!("<{w}numFmt {w}val=\"{}\"/>", props::esc(f));
                }
                if n.start != 1 {
                    x += &format!("<{w}numStart {w}val=\"{}\"/>", n.start);
                }
                let restart = match n.restart {
                    NoteRestart::EachSect => Some("eachSect"),
                    NoteRestart::EachPage => Some("eachPage"),
                    NoteRestart::Continuous => None,
                };
                if let Some(r) = restart {
                    x += &format!("<{w}numRestart {w}val=\"{r}\"/>");
                }
                put(&mut s, name, (!x.is_empty()).then(|| format!("<{w}{name}>{x}</{w}{name}>")));
            }
        }
        if was.form_protected != p.form_protected {
            put(&mut s, "formProt", (!p.form_protected).then(|| format!("<{w}formProt {w}val=\"0\"/>")));
        }
        if was.no_endnote != p.no_endnote {
            put(&mut s, "noEndnote", p.no_endnote.then(|| format!("<{w}noEndnote/>")));
        }
        // A header or footer it now takes from the section before, or from another part: its reference goes (new ones are
        // `new_stories`').
        for (el, was, now) in [("headerReference", &was.headers, &p.headers), ("footerReference", &was.footers, &p.footers)] {
            for (slot, t) in ["default", "first", "even"].into_iter().enumerate() {
                if was[slot].is_some() && was[slot] != now[slot] {
                    let (open, ty) = (format!("<{w}{el}"), format!("{w}type"));
                    let mut at = open_end;
                    while let Some(k) = s[at..].find(&open).map(|k| at + k) {
                        let name_end = k + open.len();
                        let Some(gt) = crate::docx::binding::tag_end(&s[k..]).map(|e| k + e) else { break };
                        if !s[name_end..].starts_with(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>') {
                            at = name_end;
                            continue;
                        }
                        let head = &s[k..gt];
                        // Its slot: `w:type`, default when it has none.
                        let slot_of = crate::docx::binding::tag_attrs(head.trim_end_matches('/'))
                            .find(|(name, ..)| *name == ty)
                            .map(|(_, v, _)| crate::docx::binding::unescape(v));
                        let end = if head.ends_with('/') {
                            gt + 1
                        } else {
                            s[gt..].find(&format!("</{w}{el}>")).map_or(gt + 1, |e| gt + e + w.len() + el.len() + 3)
                        };
                        if slot_of.as_deref().map_or("default", str::trim) == t {
                            s.replace_range(k..end, "");
                            break;
                        }
                        at = end;
                    }
                }
            }
        }
    }
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
    if doc.filled.is_empty() {
        return Ok(());
    }
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(f) = zin.by_name(n) else { return Ok(None) };
        crate::docx::package::text(f, n).map(Some)
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
                            let patched = Placement { size: Some((w, h)), ..Placement::default() }.apply(&xml[d0..d1]);
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

/// Headers and footers made since reading: each part, its relationship from the main part, its content type, and
/// the section references to it (inserted first in their sections' `w:sectPr`, where CT_SectPr has them). A section
/// whose slot shows another part read than it did gets a reference to that part's relationship too.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn new_stories(doc: &Document, parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let made: Vec<&String> = doc.stories.keys().filter(|n| !src.stories.contains_key(*n)).collect();
    // Each slot that needs a reference written: one showing a part made, or a part read it didn't show as read.
    fn wants<'d>(src: &Source, s: &'d Section, footer: bool, slot: usize) -> Option<&'d String> {
        let now = if footer { &s.footers } else { &s.headers }[slot].as_ref()?;
        match s.read.as_deref() {
            _ if !src.stories.contains_key(now) => Some(now),
            Some(r) => (if footer { &r.footers } else { &r.headers }[slot].as_ref() != Some(now)).then_some(now),
            None => None,
        }
    }
    let relinked: Vec<&String> =
        doc.sections.iter().flat_map(|s| (0..6).filter_map(move |k| wants(src, s, k >= 3, k % 3))).filter(|n| src.stories.contains_key(*n)).collect();
    if made.is_empty() && relinked.is_empty() {
        return Ok(());
    }
    let mut zin = zip::ZipArchive::new(Cursor::new(&src.package[..])).map_err(|e| e.to_string())?;
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<String, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map_err(|e| e.to_string());
        }
        crate::docx::package::text(zin.by_name(n).map_err(|e| format!("{n}: {e}"))?, n)
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
        let body = new_blocks_xml(&shell, &doc.stories[*name], &doc.controls, &doc.people)?;
        let end = shell.rfind("</").ok_or("story shell")?;
        parts.retain(|(n, _)| n != *name);
        parts.push(((*name).clone(), format!("{}{body}{}", &shell[..end], &shell[end..]).into_bytes()));
        let mut k = 1;
        while rels.contains(&format!("Id=\"rId{k}\"")) {
            k += 1;
        }
        // Beside the main part, relative to its folder; elsewhere, from the package root.
        let target = name.strip_prefix(&format!("{dir}/")).filter(|_| !dir.is_empty()).map_or_else(|| format!("/{name}"), str::to_string);
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
    if !relinked.is_empty() {
        let dom = roxmltree::Document::parse(&rels).map_err(|e| e.to_string())?;
        for name in relinked {
            let id = dom.root_element().children().find(|r| r.attribute("Target").is_some_and(|t| crate::docx::package::resolve(&main, t) == *name));
            if let Some(id) = id.and_then(|r| r.attribute("Id")) {
                ids.insert(name, id.to_string());
            }
        }
    }
    // The sections' `w:sectPr`s in document order are the sections.
    let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    let sects: Vec<(usize, bool, Range<usize>)> = section_props(&dom)
        .into_iter()
        .map(|r| {
            let self_closing = xml[r.clone()].ends_with("/>");
            let at = if self_closing { r.end - 2 } else { r.start + xml[r.clone()].find('>').map_or(0, |i| i + 1) };
            (at, self_closing, r)
        })
        .collect();
    drop(dom);
    let mut inserts: Vec<(usize, bool, Range<usize>, String)> = Vec::new();
    for (k, s) in doc.sections.iter().enumerate() {
        let mut refs = String::new();
        for footer in [false, true] {
            for slot in 0..3 {
                if let Some(id) = wants(src, s, footer, slot).and_then(|n| ids.get(n)) {
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
    controls: &HashMap<ControlId, Control>,
    people: &People,
) -> Result<Vec<(String, Vec<u8>)>, String> {
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
        body.push_str(&format!(r#"<w:{tag} w:id="{id}">{}</w:{tag}>"#, new_blocks_xml(&shell, &notes[id], controls, people)?));
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
        crate::docx::package::text(zin.by_name(n).map_err(|e| format!("{n}: {e}"))?, n)
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
        let body = new_blocks_xml(xml, blocks, controls, people)?;
        text.push_str(&format!("<{w}{tag} {w}id=\"{id}\">{body}</{w}{tag}>"));
    }
    if !text.is_empty() {
        let r = root.range();
        if let Some(el) = filled(&xml[r.clone()], &text) {
            // An empty, self-closing part root: opened and closed around the new notes.
            splices.push((r.clone(), el));
        } else {
            let close = xml[..r.end].rfind("</").ok_or("a notes part without its end tag")?;
            splices.push((close..close, text));
        }
    }
    // One pass: the kept notes' edits and these whole-note splices never overlap (removed notes aren't in `lists`).
    part_xml_with(xml, inner, lists, &splices, &SectionEdits::default(), controls, gone, people)
}

/// The controls read that the document no longer has (removed, or emptied of every member).
fn gone_controls(doc: &Document) -> Vec<Control> {
    doc.source.as_ref().map_or(Vec::new(), |s| s.controls.iter().filter(|(id, _)| !doc.controls.contains_key(id)).map(|(_, c)| c.clone()).collect())
}

/// Blocks with no source written from the model, for a part (new notes).
fn new_blocks_xml(xml: &str, blocks: &[Block], controls: &HashMap<ControlId, Control>, people: &People) -> Result<String, String> {
    let dom = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let w = match dom.root_element().lookup_prefix(W) {
        Some(p) if !p.is_empty() => format!("{p}:"),
        _ => String::new(),
    };
    blocks_in(xml, w, blocks, controls, people, Default::default())
}

/// Blocks with no source written from the model, for a part (its WordprocessingML prefix `w`), with bookmark ids
/// shared with what else writes it.
fn blocks_in(
    xml: &str,
    w: String,
    blocks: &[Block],
    controls: &HashMap<ControlId, Control>,
    people: &People,
    bookmarks: Bookmarks,
) -> Result<String, String> {
    let made = HashMap::new();
    let mut s = Saver {
        xml,
        paras: HashMap::new(),
        tables: HashMap::new(),
        w,
        cx: Cx::of_xml(xml),
        splices: Vec::new(),
        nested: Vec::new(),
        float_src: HashMap::new(),
        controls: controls.clone(),
        ids: std::cell::Cell::new(DRAWING_IDS),
        people,
        dropped: &[],
        made: &made,
        bookmarks,
        marks: Default::default(),
    };
    s.blocks_xml(None, blocks, &[])
}

/// The content of each outermost `w:txbxContent` in a drawing's XML (any prefix; a Choice's and its Fallback's both):
/// the range between its tags. One written self-closing has none.
fn box_contents(xml: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (k, _) in xml.match_indices("txbxContent") {
        let Some(lt) = xml[..k].rfind('<') else { continue };
        let name = &xml[lt + 1..k];
        let close = name.starts_with('/');
        let prefix = name.trim_start_matches('/');
        let named = prefix.is_empty()
            || prefix.ends_with(':') && !prefix[..prefix.len() - 1].contains(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.');
        if !named || !matches!(xml.as_bytes().get(k + "txbxContent".len()), Some(b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')) {
            continue;
        }
        let Some(gt) = crate::docx::binding::tag_end(&xml[k..]).map(|g| k + g) else { break };
        if close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                out.push(start..lt);
            }
        } else if !xml[..gt].ends_with('/') {
            if depth == 0 {
                start = gt + 1;
            }
            depth += 1;
        }
    }
    out
}

/// The text boxes in `orig` with what they hold now (by float), innermost first.
fn boxes<'d>(orig: &[&'d [Block]], now: &[&'d [Block]]) -> Vec<(&'d [Block], &'d [Block])> {
    fn walk<'d>(blocks: &'d [Block], out: &mut Vec<(u64, &'d [Block])>) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    for i in &p.inlines {
                        let story = match i {
                            Inline::Float(f, _) => f.text_box.as_ref().map(|tb| (f.id, tb)),
                            Inline::Picture(pic, _) => pic.text.as_ref().map(|t| (t.id, &t.text_box)),
                            _ => None,
                        };
                        if let Some((id, tb)) = story {
                            out.push((id, &tb.blocks));
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

/// Section breaks changed since reading: the `w:sectPr`s of those deleted, and the ones a new section's last paragraph
/// takes, by paragraph id.
#[derive(Default)]
struct SectionEdits {
    dropped: Vec<Range<usize>>,
    made: HashMap<u64, String>,
}

/// Self-closing element `el` opened and closed around `text`; `None` when `el` isn't self-closing.
fn filled(el: &str, text: &str) -> Option<String> {
    let open = el.strip_suffix("/>")?;
    let name = el.get(1..)?.split(|c: char| c.is_whitespace() || c == '/' || c == '>').next()?;
    Some(format!("{open}>{text}</{name}>"))
}

/// Takes the paragraphs the reader made to close their story out of `qs`, with the place of the last
/// ([`ParaSrc::closing`]).
fn take_closing(qs: &mut Vec<&Paragraph>) -> Option<Range<usize>> {
    let mut at = None;
    qs.retain(|q| match q.src.as_deref() {
        Some(s) if s.closing => {
            at = s.ranges.first().cloned();
            false
        }
        _ => true,
    });
    at
}

/// The main part's XML after the edits; `None` when nothing changed.
pub fn main_xml(doc: &Document) -> Result<Option<String>, String> {
    use std::borrow::Cow;
    let src = doc.source.as_ref().ok_or("the document was not read for editing")?;
    let sources = if doc.section_sources.is_empty() { vec![1; doc.sections.len()] } else { doc.section_sources.clone() };
    if sources.len() != doc.sections.len() || sources.iter().sum::<usize>() != src.blocks.len() || sources.last() == Some(&0) {
        return Err("the sections don't match the source's".into());
    }
    // The source's `w:sectPr`s, when a break was deleted or a section made.
    let (sects, w): (Vec<Range<usize>>, String) = if sources.iter().all(|&n| n == 1) {
        (Vec::new(), String::new())
    } else {
        let dom = roxmltree::Document::parse(&src.xml).map_err(|e| e.to_string())?;
        let w = dom.root_element().lookup_prefix(W).filter(|p| !p.is_empty()).map_or(String::new(), |p| format!("{p}:"));
        (section_props(&dom), w)
    };
    let sect = |k: usize| sects.get(k).cloned().ok_or("a section without its sectPr");
    // A section holding several as read (their breaks deleted) is compared with their blocks together, and the
    // `w:sectPr`s of the breaks deleted go. A section made since reading is compared with the one after it, its last
    // paragraph taking a `w:sectPr` made from that one's as read.
    let mut orig: Vec<Cow<[Block]>> = Vec::new();
    let mut now: Vec<Cow<[Block]>> = Vec::new();
    let mut edits = SectionEdits::default();
    let (mut at, mut made) = (0, Vec::new());
    for (k, &n) in sources.iter().enumerate() {
        if n == 0 {
            made.push(k);
            continue;
        }
        orig.push(match n {
            1 => Cow::Borrowed(src.blocks[at].as_slice()),
            _ => Cow::Owned(src.blocks[at..at + n].concat()),
        });
        for g in at..at + n - 1 {
            edits.dropped.push(sect(g)?);
        }
        let s = &doc.sections[k];
        if made.is_empty() {
            now.push(Cow::Borrowed(s.blocks.as_slice()));
        } else {
            let base = &src.xml[sect(at + n - 1)?];
            let mut blocks = Vec::new();
            for j in made.drain(..) {
                let new = &doc.sections[j];
                let Some(Block::Para(last)) = new.blocks.last() else { return Err("a new section that doesn't end in a paragraph".into()) };
                edits.made.insert(last.id, sect_with_page(base, &w, &new.page(), s.read.as_deref(), None, &doc.people));
                blocks.extend(new.blocks.iter().cloned());
            }
            blocks.extend(s.blocks.iter().cloned());
            now.push(Cow::Owned(blocks));
        }
        at += n;
    }
    let lists: Vec<(&[Block], &[Block])> = orig.iter().zip(&now).map(|(a, b)| (a.as_ref(), b.as_ref())).collect();
    let (o, n): (Vec<&[Block]>, Vec<&[Block]>) = lists.iter().copied().unzip();
    part_xml_with(&src.xml, &boxes(&o, &n), &lists, &[], &edits, &doc.controls, &gone_controls(doc), &doc.people)
}

/// A part's XML with its block lists (as read, now) written back; `None` when none changed.
fn part_xml(
    xml: &str,
    inner: &[(&[Block], &[Block])],
    lists: &[(&[Block], &[Block])],
    controls: &HashMap<ControlId, Control>,
    gone: &[Control],
    people: &People,
) -> Result<Option<String>, String> {
    part_xml_with(xml, inner, lists, &[], &SectionEdits::default(), controls, gone, people)
}

/// [`part_xml`] with `extra` splices of the original part added (whole notes in and out), and the section breaks
/// `dropped` (their `w:sectPr`s) taken out.
#[allow(clippy::too_many_arguments, reason = "the part, its block lists and the document state its rewrite reads")]
fn part_xml_with<'a>(
    xml: &'a str,
    inner: &[(&[Block], &[Block])],
    lists: &[(&[Block], &[Block])],
    extra: &[Splice],
    sections: &'a SectionEdits,
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
        splices: Vec::new(),
        nested: Vec::new(),
        float_src,
        controls: controls.clone(),
        ids: std::cell::Cell::new(DRAWING_IDS),
        people,
        dropped: &sections.dropped,
        made: &sections.made,
        bookmarks: Default::default(),
        marks: Default::default(),
    };
    let mut now = Vec::new();
    for (_, cur) in lists.iter().chain(inner) {
        all_paras(cur, &mut now);
    }
    s.marks = now.iter().flat_map(|p| p.inlines.iter()).filter_map(|i| s.mark_key(i)).collect();
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
    // A dropped break in a paragraph written again went with it; else it goes from the paragraph as read.
    let rewritten = |r: &Range<usize>| s.splices.iter().any(|(o, _)| o.start <= r.start && r.end <= o.end);
    let unwritten: Vec<Splice> = sections.dropped.iter().filter(|r| !rewritten(r)).map(|r| (r.clone(), String::new())).collect();
    s.splices.extend(unwritten);
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
    /// The `w:sectPr`s of section breaks deleted: a paragraph written again doesn't keep one.
    dropped: &'a [Range<usize>],
    /// The `w:sectPr`s new sections' last paragraphs take, by paragraph id.
    made: &'a HashMap<u64, String>,
    /// Bookmark ids by name: the part's, and those given to bookmarks written new.
    bookmarks: Bookmarks,
    /// The markers ([`Saver::mark_key`]) the part's content holds now.
    marks: std::collections::HashSet<(u8, i64)>,
}

/// A part's bookmark ids by name, read when first needed, shared by what writes the part's content.
type Bookmarks = Rc<std::cell::RefCell<Option<HashMap<String, i64>>>>;

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
            // A floating picture control is written without the fallback copy beside it.
            Inline::Float(f, _) => f.picture_control(&self.controls).is_some() && f.src.clone().is_some_and(|r| self.alternate_content(r).is_some()),
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

    /// The table's own default cell margins, as its properties hold them.
    fn direct_cell_margins(&self, t: &Table) -> Option<String> {
        Props::parse(t.direct.0.as_deref(), "tblPr", &self.cx).get("tblCellMar").map(str::to_string)
    }

    /// A table's own `w:shd` as written in its `w:tblPr` (not its style's).
    fn direct_shd(&self, t: &Table) -> Option<String> {
        Props::parse(t.direct.0.as_deref(), "tblPr", &self.cx).get("shd").map(str::to_string)
    }

    /// A table's own band sizes as written in its `w:tblPr` (not its style's).
    fn direct_bands(&self, t: &Table) -> [Option<String>; 2] {
        let p = Props::parse(t.direct.0.as_deref(), "tblPr", &self.cx);
        ["tblStyleRowBandSize", "tblStyleColBandSize"].map(|n| p.get(n).map(str::to_string))
    }

    fn table(&mut self, a: &Table, b: &Table) -> Result<(), String> {
        let same_rows = a.rows.len() == b.rows.len()
            && a.rows.iter().zip(&b.rows).all(|(x, y)| {
                x.cells.len() == y.cells.len()
                    && x.controls == y.controls
                    && x.cells.iter().zip(&y.cells).all(|(c, d)| c.controls == d.controls)
                    && same_src(&x.src, &y.src)
                    && x.height == y.height
                    && (x.header, x.cant_split, x.hidden, x.jc) == (y.header, y.cant_split, y.hidden, y.jc)
                    && (&x.direct.0, &x.exceptions.0) == (&y.direct.0, &y.exceptions.0)
                    && (x.grid_before, x.grid_after) == (y.grid_before, y.grid_after)
                    && x.rev == y.rev
                    && x.cells.iter().zip(&y.cells).all(|(c, d)| {
                        c.width == d.width
                            && c.span == d.span
                            && c.direct.0 == d.direct.0
                            && c.v_merge == d.v_merge
                            && c.rev == d.rev
                            && c.ids == d.ids
                    })
            })
            && a.grid == b.grid
            && (&a.change, &a.grid_change) == (&b.change, &b.grid_change)
            && a.width == b.width
            && (a.cell_spacing, &a.alt) == (b.cell_spacing, &b.alt)
            && self.direct_shd(a) == self.direct_shd(b)
            && self.direct_cell_margins(a) == self.direct_cell_margins(b)
            && (&a.style_id, a.look, a.jc, a.ind, a.ind_pct, a.floating) == (&b.style_id, b.look, b.jc, b.ind, b.ind_pct, b.floating)
            && (a.fixed, a.cell_spacing, &a.alt) == (b.fixed, b.cell_spacing, &b.alt);
        // Tables read as one (nothing between them) are written as one.
        let joined = a.src.as_ref().is_some_and(|r| self.tables.get(&r.start).is_some_and(|n| n.range().end != r.end));
        if !same_rows || joined {
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
            self.gone_marks(&qs, p);
            let closing = take_closing(&mut qs);
            return self.with_new_tables(&qs, p, at, closing);
        }
        let ps: Vec<&Paragraph> = p.iter().filter_map(|b| if let Block::Para(x) = b { Some(x) } else { None }).collect();
        if qs.is_empty() && ps.is_empty() {
            return Ok(());
        }
        if qs.len() == 1
            && ps.len() == 1
            && same_para(qs[0], ps[0])
            && qs[0].controls == ps[0].controls
            && !self.changed_inline(ps[0])
            && !self.made.contains_key(&ps[0].id)
        {
            return Ok(());
        }
        self.gone_marks(&qs, p);
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
        let closing = take_closing(&mut qs);
        let srcs: Vec<&ParaSrc> = qs.iter().map(|x| x.src.as_deref().ok_or("a paragraph without its source")).collect::<Result<_, _>>()?;
        if ps.is_empty() {
            // Every paragraph of the group went (with the blocks around them).
            for r in srcs.iter().flat_map(|s| s.ranges.iter()) {
                self.splices.push((r.clone(), String::new()));
            }
            return Ok(());
        }
        let mut text = Emit::group(self, &qs, &srcs, &ps)?;
        // Frames made since reading (a drop cap): their paragraphs, each with its `w:framePr`, go before the paragraph
        // they anchor at.
        let made: String = ps
            .iter()
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| match i {
                Inline::Float(f, _) if f.frame_src.is_none() && f.is_frame() => f.text_box.as_ref(),
                _ => None,
            })
            .map(|tb| self.nested_blocks(&tb.blocks))
            .collect::<Result<_, _>>()?;
        text.insert_str(0, &made);
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
            None => self.put(closing, at, text, "nowhere to put new paragraphs")?,
        }
        Ok(())
    }

    /// The markers read beside a group's paragraphs in their container (those before a paragraph, and after its
    /// container's last one, are the reader's for it) that none of the group's paragraphs holds now: taken out.
    /// Writing the group leaves those it holds where they were; one that moved to another group is written with it.
    fn gone_marks(&mut self, qs: &[&Paragraph], p: &[Block]) {
        let carriers = self.carriers(qs.iter().copied());
        if carriers.is_empty() {
            return;
        }
        let held: std::collections::HashSet<(u8, i64)> = p
            .iter()
            .filter_map(|b| if let Block::Para(x) = b { Some(x) } else { None })
            .flat_map(|x| x.inlines.iter())
            .filter_map(|i| self.mark_key(i))
            .collect();
        let block = |n: &Node| is_w(*n, "p") || is_w(*n, "tbl") || is_w(*n, "sdt") || is_w(*n, "customXml") || is(*n, MC, "AlternateContent");
        let mut gone = Vec::new();
        for s in qs.iter().filter_map(|q| q.src.as_ref()) {
            let first = s.ranges.first().and_then(|r| self.paras.get(&r.start)).copied();
            let last = s.ranges.last().and_then(|r| self.paras.get(&r.start)).copied();
            let before = first.into_iter().flat_map(|n| n.prev_siblings().skip(1).filter(Node::is_element).take_while(|n| !block(n)));
            let mut after: Vec<Node> = Vec::new();
            for n in last.into_iter().flat_map(|n| n.next_siblings().skip(1).filter(Node::is_element)) {
                if block(&n) {
                    after.clear();
                    break;
                }
                after.push(n);
            }
            for n in before.chain(after) {
                if self.element_key(n).is_some_and(|k| carriers.contains_key(&k) && !self.keeps_mark(k, held.contains(&k))) {
                    gone.push((n.range(), String::new()));
                }
            }
        }
        self.splices.extend(gone);
    }

    /// `text` where a group without source paragraphs goes: in place of its closing paragraph, else at `at`.
    fn put(&mut self, closing: Option<Range<usize>>, at: Option<usize>, text: String, none: &str) -> Result<(), String> {
        let splice = match closing {
            Some(r) if !r.is_empty() => {
                let el = &self.xml[r.clone()];
                (r, filled(el, &text).ok_or("a closing paragraph's story without its tags")?)
            }
            Some(r) => (r, text),
            None => {
                let at = at.ok_or(none)?;
                (at..at, text)
            }
        };
        self.splices.push(splice);
        Ok(())
    }

    /// A paragraph group with new tables among its paragraphs: each run of paragraphs is written with the source
    /// paragraphs it continues (deleted ones with the first run), each table from the model.
    // shortcut: a section break in the group goes with the run its source paragraph went to, which may not be the last.
    fn with_new_tables(&mut self, qs: &[&Paragraph], p: &[Block], at: Option<usize>, closing: Option<Range<usize>>) -> Result<(), String> {
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
            None => self.put(closing, at, text, "nowhere to put new blocks")?,
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
                        let mut fix: Vec<(&str, String, &[&str])> = Vec::new();
                        if a.style_id != t.style_id {
                            let tag = t.style_id.as_ref().map_or(String::new(), |id| format!("<{w}tblStyle {w}val=\"{}\"/>", props::esc(id)));
                            fix.push(("tblStyle", tag, before("tblStyle")));
                        }
                        if a.floating != t.floating {
                            let (pos, overlap) = tblp_pr(&w, t.floating.as_ref());
                            fix.extend([("tblpPr", pos, before("tblpPr")), ("tblOverlap", overlap, before("tblOverlap"))]);
                        }
                        if a.width != t.width {
                            fix.push(("tblW", width(&w, "tblW", &t.width), before("tblW")));
                        }
                        if a.jc != t.jc {
                            let v = match t.jc {
                                Jc::Center => "center",
                                Jc::Right => "right",
                                _ => "left",
                            };
                            fix.push(("jc", format!("<{w}jc {w}val=\"{v}\"/>"), before("jc")));
                        }
                        if a.fixed != t.fixed {
                            let v = if t.fixed { "fixed" } else { "autofit" };
                            fix.push(("tblLayout", format!("<{w}tblLayout {w}type=\"{v}\"/>"), before("tblLayout")));
                        }
                        if a.cell_spacing != t.cell_spacing {
                            let tag = width(&w, "tblCellSpacing", &Width::Dxa(t.cell_spacing));
                            fix.push(("tblCellSpacing", tag, before("tblCellSpacing")));
                        }
                        // The table's own shading; its style's stays the style's.
                        let own = self.direct_shd(t);
                        if self.direct_shd(a) != own {
                            fix.push(("shd", own.unwrap_or_default(), before("shd")));
                        }
                        let (was, now) = (self.direct_bands(a), self.direct_bands(t));
                        for (k, name) in ["tblStyleRowBandSize", "tblStyleColBandSize"].into_iter().enumerate() {
                            if was[k] != now[k] {
                                fix.push((name, now[k].clone().unwrap_or_default(), before(name)));
                            }
                        }
                        // The table's own default cell margins, as its properties hold them.
                        let (was, now) = (self.direct_cell_margins(a), self.direct_cell_margins(t));
                        if was != now {
                            fix.push(("tblCellMar", now.unwrap_or_default(), before("tblCellMar")));
                        }
                        if a.alt != t.alt {
                            let (title, description) = t.alt.as_deref().map_or(("", ""), |a| (a.title.as_str(), a.description.as_str()));
                            let tag = |name: &str, v: &str| if v.is_empty() { String::new() } else { format!("<{w}{name} {w}val=\"{}\"/>", props::esc(v)) };
                            fix.extend([
                                ("tblCaption", tag("tblCaption", title), before("tblCaption")),
                                ("tblDescription", tag("tblDescription", description), before("tblDescription")),
                            ]);
                        }
                        if (a.ind, a.ind_pct) != (t.ind, t.ind_pct) {
                            fix.push(("tblInd", format!("<{w}tblInd {w}w=\"{}\" {w}type=\"dxa\"/>", twips(t.ind)), before("tblInd")));
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
                            fix.push(("tblLook", tag, before("tblLook")));
                        }
                        if a.change != t.change {
                            let tag = t.change.as_ref().map_or(String::new(), |c| revisions::table_change(&w, "tblPrChange", "tblPr", c, self.people));
                            fix.push(("tblPrChange", tag, before("tblPrChange")));
                        }
                        // One rewrite of them all: a self-closing `w:tblPr` is rewritten whole, in schema order.
                        fix.sort_by_key(|(name, _, _)| before(name).len());
                        let fix = self.set_children(p, fix);
                        if fix.is_empty() { self.xml[p.range()].to_string() } else { self.spliced(p.range(), fix)? }
                    }
                    (Some(p), None) => self.xml[p.range()].to_string(),
                    (None, _) => tbl_pr(&w, t, self.direct_shd(t).as_deref(), &self.direct_bands(t), self.direct_cell_margins(t).as_deref(), self.people),
                });
                close
            }
            None => {
                out.push_str(&format!("<{w}tbl>"));
                let shd = self.direct_shd(t);
                out.push_str(&tbl_pr(&w, t, shd.as_deref(), &self.direct_bands(t), self.direct_cell_margins(t).as_deref(), self.people));
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
        let first = node.map(|n| (n, self.direct_shd(t).is_some()));
        // What the source tables (those read as one) hold between their rows besides rows: bookmarks, permissions and
        // the like, written again before the kept row they came before.
        let mut loose: Vec<Range<usize>> = Vec::new();
        if let (Some(n), Some(end)) = (node, orig.and_then(|a| a.src.as_ref()).map(|r| r.end)) {
            let tables = std::iter::successors(Some(n), |t| t.next_sibling_element()).take_while(|t| is_w(*t, "tbl") && t.range().end <= end);
            for tbl in tables {
                self.loose(tbl, &["tblPr", "tblGrid", "tr"], &|_| true, &mut loose);
            }
        }
        let mut loose = loose.into_iter().peekable();
        // The markers the cells' paragraphs were read with (those read between paragraphs are written again only
        // while their paragraph holds them).
        let mut was = Vec::new();
        for c in orig.iter().flat_map(|a| a.rows.iter()).flat_map(|r| r.cells.iter()) {
            all_paras(&c.blocks, &mut was);
        }
        let carriers = self.carriers(was.into_iter());
        let (mut rows_seen, mut cells_seen) = (std::collections::HashSet::new(), std::collections::HashSet::new());
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
            let kept = row.src.as_ref().and_then(|r| old_rows.iter().find(|o| o.src.as_ref().is_some_and(|s| Rc::ptr_eq(s, r))).copied());
            // The source row it was read as (by its cells' paragraphs when a structural edit made it again), its
            // markup written once.
            let from = kept
                .and_then(|k| k.src.as_ref())
                .and_then(|r| self.tables.get(&r.start).copied())
                .or_else(|| row.cells.iter().find_map(|c| self.cell_node(c)).and_then(|tc| tc.ancestors().skip(1).find(|a| is_w(*a, "tr"))));
            let from = from.filter(|tr| rows_seen.insert(tr.range().start));
            if let Some(at) = from.map(|tr| tr.range().start) {
                while let Some(r) = loose.next_if(|r| r.start < at) {
                    wrap(&mut out, &self.loose_within(&r, &open, &row.controls), &mut open, self);
                    out.push_str(&self.src_text(r));
                }
            }
            wrap(&mut out, &row.controls, &mut open, self);
            let row_node = kept.and_then(|k| k.src.as_ref()).and_then(|r| self.tables.get(&r.start).copied());
            match (kept, row_node) {
                (Some(k), Some(n)) if k.cells.len() == row.cells.len() => {
                    let outer = std::mem::take(&mut self.splices);
                    for (c, d) in k.cells.iter().zip(&row.cells) {
                        self.container(&c.blocks, &d.blocks)?;
                    }
                    let mut inner = std::mem::replace(&mut self.splices, outer);
                    inner.extend(self.row_fixes(n, k, row, first));
                    out.push_str(&self.spliced(n.range(), inner)?);
                }
                (_, n) => {
                    let (open, close) = match n {
                        Some(n) => self.tags(n),
                        None => (format!("<{w}tr>"), format!("</{w}tr>")),
                    };
                    out.push_str(&open);
                    out.push_str(&self.row_props(n, kept, row, first));
                    // What the source row holds besides its cells, before the cell it came before.
                    let mut row_loose = Vec::new();
                    if let Some(n) = from {
                        self.loose(n, &["trPr", "tblPrEx", "tc"], &|_| true, &mut row_loose);
                    }
                    let mut row_loose = row_loose.into_iter().peekable();
                    let mut cells_open = Vec::new();
                    for c in &row.cells {
                        // A source cell's markup is written once (a split cell's other part has none).
                        let tc = self.cell_node(c).filter(|tc| cells_seen.insert(tc.range().start));
                        if let Some(at) = tc.map(|tc| tc.range().start) {
                            while let Some(r) = row_loose.next_if(|r| r.start < at) {
                                wrap(&mut out, &self.loose_within(&r, &cells_open, &c.controls), &mut cells_open, self);
                                out.push_str(&self.src_text(r));
                            }
                        }
                        wrap(&mut out, &c.controls, &mut cells_open, self);
                        let id = c.ids.as_ref().and_then(|i| i.id.as_deref()).map(|id| format!(" {w}id=\"{}\"", props::esc(id))).unwrap_or_default();
                        out.push_str(&format!("<{w}tc{id}>{}", tc_pr(&w, c, self.people, &self.cx)));
                        let mut cell_loose = Vec::new();
                        if let Some(tc) = tc {
                            let mut paras = Vec::new();
                            all_paras(&c.blocks, &mut paras);
                            let held = self.held(&carriers, paras.into_iter());
                            let keep =
                                |n: Node| self.element_key(n).is_none_or(|k| !carriers.contains_key(&k) || self.keeps_mark(k, held.contains(&k)));
                            self.loose(tc, &["tcPr", "p", "tbl"], &keep, &mut cell_loose);
                        }
                        out.push_str(&self.blocks_xml(orig, &c.blocks, &cell_loose)?);
                        out.push_str(&format!("</{w}tc>"));
                    }
                    for r in row_loose {
                        wrap(&mut out, &self.loose_within(&r, &cells_open, &[]), &mut cells_open, self);
                        out.push_str(&self.src_text(r));
                    }
                    wrap(&mut out, &[], &mut cells_open, self);
                    out.push_str(&close);
                }
            }
        }
        for r in loose {
            wrap(&mut out, &self.loose_within(&r, &open, &[]), &mut open, self);
            out.push_str(&self.src_text(r));
        }
        wrap(&mut out, &[], &mut open, self);
        out.push_str(&close);
        Ok(out)
    }

    /// The row or cell controls markup read at `r` between rows (or cells) is written in: of those `open` (after the
    /// last written) or `next` (around the next) holds, the ones read around it, else the ones both share.
    fn loose_within(&self, r: &Range<usize>, open: &[ControlId], next: &[ControlId]) -> Vec<ControlId> {
        let shared = open.iter().zip(next).take_while(|(a, b)| a == b).count();
        let (last, coming) = (self.read_within(open, r), self.read_within(next, r).max(shared));
        if last > coming { open[..last].to_vec() } else { next[..coming].to_vec() }
    }

    /// How many of `controls` (outermost first) were read around source range `r` in this part.
    fn read_within(&self, controls: &[ControlId], r: &Range<usize>) -> usize {
        controls
            .iter()
            .take_while(|id| {
                self.controls.get(id).is_some_and(|c| {
                    c.at.is_some_and(|(a, b)| a < r.start && r.end <= b && self.xml.get(a..a + c.open.len()) == Some(c.open.as_str()))
                })
            })
            .count()
    }

    /// The source cell a cell's paragraphs were read from.
    fn cell_node(&self, c: &Cell) -> Option<Node<'a, 'i>> {
        fn first(blocks: &[Block]) -> Option<usize> {
            blocks.iter().find_map(|b| match b {
                Block::Para(p) => p.src.as_ref().map(|s| s.ranges[0].start),
                Block::Table(t) => t.src.as_ref().map(|r| r.start),
            })
        }
        let at = first(&c.blocks)?;
        let n = self.paras.get(&at).or_else(|| self.tables.get(&at))?;
        n.ancestors().skip(1).find(|a| is_w(*a, "tc"))
    }

    /// The children of `container` (a table, row or cell as read) that aren't `own` nor wrap them (a content
    /// control's or custom XML's content is looked in): its markup between them, in order.
    fn loose(&self, container: Node, own: &[&str], keep: &dyn Fn(Node) -> bool, out: &mut Vec<Range<usize>>) {
        for n in container.children().filter(|c| c.is_element()) {
            if is_w(n, "sdt") {
                if let Some(c) = child(n, "sdtContent") {
                    self.loose(c, own, keep, out);
                }
            } else if is_w(n, "customXml") {
                self.loose(n, own, keep, out);
            } else if !(n.tag_name().namespace() == Some(W) && own.contains(&n.tag_name().name())
                || is_w(n, "customXmlPr")
                || is(n, MC, "AlternateContent"))
                && keep(n)
            {
                out.push(n.range());
            }
        }
    }

    /// A marker's key: a bookmark's start (0) or end (1) by its id in the part, a comment's or move's range by its
    /// kind and id. A start's and its end's differ in the lowest bit only.
    fn mark_key(&self, i: &Inline) -> Option<(u8, i64)> {
        match i {
            Inline::Bookmark(n) => {
                let (name, end) = n.strip_prefix('/').map_or((n.as_str(), false), |n| (n, true));
                Some((u8::from(end), self.bookmark_id(name)))
            }
            Inline::Range(r) => Some((2 + r.kind as u8, r.id)),
            _ => None,
        }
    }

    /// [`Saver::mark_key`] of a marker element as read.
    fn element_key(&self, n: Node) -> Option<(u8, i64)> {
        const KINDS: [RangeKind; 6] = [
            RangeKind::CommentStart,
            RangeKind::CommentEnd,
            RangeKind::MoveFromStart,
            RangeKind::MoveFromEnd,
            RangeKind::MoveToStart,
            RangeKind::MoveToEnd,
        ];
        let id = || n.attribute((W, "id")).and_then(|v| v.trim().parse::<i64>().ok());
        if is_w(n, "bookmarkStart") {
            return Some((0, self.bookmark_id(n.attribute((W, "name"))?)));
        }
        if is_w(n, "bookmarkEnd") {
            return Some((1, id()?));
        }
        let k = KINDS.into_iter().find(|k| is_w(n, range_element(*k)))?;
        Some((2 + k as u8, id()?))
    }

    /// Whether a marker read between paragraphs is written again from its markup there: while a paragraph it was read
    /// with holds it (`held`), and when the content lost it but holds its other end (an edit that dropped one end
    /// keeps the pair).
    fn keeps_mark(&self, k: (u8, i64), held: bool) -> bool {
        held || !self.marks.contains(&k) && self.marks.contains(&(k.0 ^ 1, k.1))
    }

    /// Each marker the paragraphs as read hold, with the source of the paragraph holding it.
    fn carriers<'p>(&self, paras: impl Iterator<Item = &'p Paragraph>) -> HashMap<(u8, i64), *const ParaSrc> {
        let mut out = HashMap::new();
        for q in paras {
            let Some(s) = &q.src else { continue };
            for k in q.inlines.iter().filter_map(|i| self.mark_key(i)) {
                out.insert(k, Rc::as_ptr(s));
            }
        }
        out
    }

    /// The markers of `carriers` that `paras` hold in the paragraph they were read with: written again from their
    /// markup read between paragraphs, which a paragraph written from the model leaves where it was. A marker that
    /// went is not, nor one that moved (written with its new paragraph).
    fn held<'p>(
        &self,
        carriers: &HashMap<(u8, i64), *const ParaSrc>,
        paras: impl Iterator<Item = &'p Paragraph>,
    ) -> std::collections::HashSet<(u8, i64)> {
        let mut out = std::collections::HashSet::new();
        for p in paras {
            let Some(s) = &p.src else { continue };
            out.extend(p.inlines.iter().filter_map(|i| self.mark_key(i)).filter(|k| carriers.get(k) == Some(&Rc::as_ptr(s))));
        }
        out
    }

    /// Blocks written from the model: paragraphs from their source paragraph in `orig` when they have one, tables
    /// as [`Saver::table_xml`]; each in the block-level controls read around it (tables in their new ones too), and
    /// `loose` (the source markup between the blocks, in order) before the block it came before.
    fn blocks_xml(&mut self, orig: Option<&Table>, blocks: &[Block], loose: &[Range<usize>]) -> Result<String, String> {
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
        let scope = orig.map_or(&[][..], |a| std::slice::from_ref(&a.rows));
        let cells = || scope.iter().flatten().flat_map(|r| r.cells.iter());
        let mut out = String::new();
        let mut new: Vec<&Paragraph> = Vec::new();
        let mut loose = loose.iter().peekable();
        // The block-level controls open: those read around paragraphs (new ones are their group's), every one
        // around tables.
        let mut open: Vec<ControlId> = Vec::new();
        for b in blocks {
            let (controls, at) = match b {
                Block::Para(p) => (&p.controls, p.src.as_ref().map(|s| s.ranges[0].start)),
                Block::Table(t) => (&t.controls, t.src.as_ref().map(|r| r.start)),
            };
            let want: Vec<ControlId> = match b {
                Block::Para(_) => controls.iter().copied().filter(|c| self.controls.get(c).is_some_and(|k| !k.open.is_empty())).collect(),
                Block::Table(_) => controls.clone(),
            };
            let keep = open.iter().zip(&want).take_while(|(a, b)| a == b).count();
            let before = at.map_or(0, |at| loose.clone().take_while(|r| r.start < at).count());
            if keep < open.len() || keep < want.len() || before > 0 {
                out.push_str(&emit(self, &[], &[], &std::mem::take(&mut new))?);
            }
            while open.len() > keep {
                out.push_str(&open.pop().and_then(|c| self.controls.get(&c)).map(|k| control_close(&self.w, k)).unwrap_or_default());
            }
            for r in loose.by_ref().take(before) {
                out.push_str(&self.src_text(r.clone()));
            }
            for c in &want[keep..] {
                if let Some(k) = self.controls.get(c) {
                    out.push_str(&control_open(&self.w, *c, k));
                }
                open.push(*c);
            }
            match b {
                Block::Para(p) => match p.src.as_ref().and_then(|s| cells().find_map(|c| find(&c.blocks, s))) {
                    Some(q) => {
                        out.push_str(&emit(self, &[], &[], &std::mem::take(&mut new))?);
                        let src = q.src.as_deref().ok_or("a paragraph without its source")?;
                        out.push_str(&emit(self, &[q], &[src], &[p])?);
                    }
                    None => new.push(p),
                },
                Block::Table(t) => {
                    out.push_str(&emit(self, &[], &[], &std::mem::take(&mut new))?);
                    let o = t.src.as_ref().and_then(|s| cells().find_map(|c| find_table(&c.blocks, s))).cloned();
                    out.push_str(&self.table_xml(o.as_ref(), t)?);
                }
            }
        }
        out.push_str(&emit(self, &[], &[], &new)?);
        for c in open.into_iter().rev() {
            out.push_str(&self.controls.get(&c).map(|k| control_close(&self.w, k)).unwrap_or_default());
        }
        for r in loose {
            out.push_str(&self.src_text(r.clone()));
        }
        Ok(out)
    }

    /// A rewritten row's `w:tblPrEx` and `w:trPr`: its source's (`n`, read as `was`) with its properties and revisions
    /// as the model has them now, else written from the model.
    fn row_props(&self, n: Option<Node>, was: Option<&Row>, row: &Row, table: Option<(Node, bool)>) -> String {
        let w = &self.w;
        let same = was.is_some_and(|k| k.rev == row.rev);
        let later = n.map(|n| self.later_table_ex(n, row, table)).unwrap_or_default();
        // Exceptions the API set are the row's, whole.
        let set_ex = was.is_some_and(|k| k.exceptions.0 != row.exceptions.0);
        // A row rewritten from the model (its cells changed) patches the properties it was read with.
        let src = |name: &str| {
            if name == "tblPrEx" && set_ex {
                return row.exceptions.0.as_deref();
            }
            n.and_then(|n| child(n, name)).map(|p| &self.xml[p.range()]).or(if name == "trPr" {
                row.direct.0.as_deref()
            } else {
                row.exceptions.0.as_deref()
            })
        };
        let mut out = String::new();
        match src("tblPrEx") {
            Some(x) if same && !set_ex && later.is_empty() => out.push_str(x),
            x => {
                let mut p = Props::parse(x, "tblPrEx", &self.cx);
                p.reorder(revisions::TBLPREX);
                for (name, tag, _) in later {
                    if p.get(name).is_none() {
                        p.set(name, tag);
                    }
                }
                p.remove("tblPrExChange");
                let change = revisions::ex_change(w, &row.rev, self.people);
                if !change.is_empty() {
                    p.set("tblPrExChange", change);
                }
                out.push_str(&p.xml().unwrap_or_default());
            }
        }
        let gaps = n.map(|n| grid_gaps(child(n, "trPr")));
        let fixes = tr_changes(w, gaps, was, row);
        match src("trPr") {
            Some(x) if same && fixes.is_empty() => out.push_str(x),
            Some(x) => {
                let mut p = Props::parse(Some(x), "trPr", &self.cx);
                p.reorder(revisions::TRPR);
                for (name, tag) in fixes {
                    p.set(name, tag);
                }
                for (name, tag) in revisions::row_marks(w, &row.rev, self.people).into_iter().filter(|_| !same) {
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

    /// The exceptions a row of a later table read as one with the first (`table`, the one written, and whether it is
    /// written with shading of its own) takes from its own table, as (name, XML, the names before it) in schema order:
    /// its alignment and cell margins, and its shading while the model keeps it (its own, else its style's when the
    /// first has its own: written, or none written as auto).
    fn later_table_ex(&self, n: Node, now: &Row, table: Option<(Node, bool)>) -> Vec<(&'static str, String, &'static [&'static str])> {
        let Some((first, first_own)) = table else { return Vec::new() };
        let Some(own) = n.ancestors().skip(1).find(|a| is_w(*a, "tbl")).filter(|t| first != *t) else { return Vec::new() };
        let pr = child(own, "tblPr");
        let mut out: Vec<(&'static str, String, &'static [&'static str])> = ["jc", "tblCellMar"]
            .into_iter()
            .filter_map(|name| {
                Some((name, self.xml[pr.and_then(|p| child(p, name))?.range()].to_string(), revisions::before(revisions::TBLPREX, name)))
            })
            .collect();
        if let Some((fill, hatch)) = now.table_shading {
            let tag = match pr.and_then(|p| child(p, "shd")) {
                Some(c) => Some(self.xml[c.range()].to_string()),
                None if first_own => {
                    let w = &self.w;
                    let hex = |c: Option<u32>| c.map_or("auto".to_string(), |c| format!("{c:06X}"));
                    let pattern = hatch.map_or("clear", |h| h.stripe.val());
                    Some(format!("<{w}shd {w}val=\"{pattern}\" {w}color=\"{}\" {w}fill=\"{}\"/>", hex(hatch.map(|h| h.colour)), hex(fill)))
                }
                None => None,
            };
            out.extend(tag.map(|t| ("shd", t, revisions::before(revisions::TBLPREX, "shd"))));
        }
        out.sort_by_key(|(_, _, before)| before.len());
        out
    }

    /// A kept row's height and its cells' widths changed by a border drag, and its and its cells' revisions, patched
    /// into its source properties.
    /// `table` is the table written (the first of tables read as one) and whether it is written with shading of its own.
    fn row_fixes(&self, n: Node, was: &Row, now: &Row, table: Option<(Node, bool)>) -> Vec<Splice> {
        let w = self.w.clone();
        let mut out = Vec::new();
        // A row of a later table read as one with the first: its own table's alignment and cell margins become its
        // exceptions (the row's own win).
        let ex = child(n, "tblPrEx");
        let mut later = self.later_table_ex(n, now, table);
        later.retain(|(name, _, _)| ex.is_none_or(|e| child(e, name).is_none()));
        // The table exceptions' tracked change.
        let change = if was.rev.ex_change != now.rev.ex_change { Some(revisions::ex_change(&w, &now.rev, self.people)) } else { None };
        match ex {
            // Exceptions the API set: the row's, whole.
            _ if was.exceptions.0 != now.exceptions.0 => {
                let mut p = Props::parse(now.exceptions.0.as_deref(), "tblPrEx", &self.cx);
                p.reorder(revisions::TBLPREX);
                for (name, tag, _) in later {
                    if p.get(name).is_none() {
                        p.set(name, tag);
                    }
                }
                p.remove("tblPrExChange");
                let change = revisions::ex_change(&w, &now.rev, self.people);
                if !change.is_empty() {
                    p.set("tblPrExChange", change);
                }
                let xml = p.xml().unwrap_or_default();
                match ex {
                    Some(e) => out.push((e.range(), xml)),
                    None => {
                        let at = self.open_end(n);
                        out.push((at..at, xml));
                    }
                }
            }
            Some(p) => {
                later.extend(change.map(|tag| ("tblPrExChange", tag, revisions::before(revisions::TBLPREX, "tblPrExChange"))));
                out.extend(self.set_children(p, later));
            }
            None => {
                let kids: String = later.into_iter().map(|(_, t, _)| t).chain(change).collect();
                if !kids.is_empty() {
                    let at = self.open_end(n);
                    out.push((at..at, format!("<{w}tblPrEx>{kids}</{w}tblPrEx>")));
                }
            }
        }
        // Its base properties: grid gaps (a border moved in some rows only), breaking, header, hidden and height.
        const BASE: &[&str] = &["cnfStyle", "divId"];
        let int_of = |n: Option<Node>, name: &str| n.and_then(|p| child(p, name)).and_then(|c| attr(c, "val")).and_then(|v| v.parse::<usize>().ok());
        let mut tr: Vec<(&str, String, &[&str])> =
            tr_changes(&w, Some(grid_gaps(child(n, "trPr"))), Some(was), now).into_iter().map(|(name, tag)| (name, tag, BASE)).collect();
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
                if int_of(child(*tc, "tcPr"), "gridSpan").unwrap_or(1).max(1) != b.span.max(1) {
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
                for name in ["tcBorders", "shd", "noWrap", "tcMar", "textDirection", "tcFitText", "vAlign", "hideMark"] {
                    if da.get(name) != db.get(name) {
                        fix.push((name, db.get(name).unwrap_or("").to_string(), revisions::before(revisions::TCPR, name)));
                    }
                }
                // Its header cells, and its own id (`w:tc/@w:id`).
                fn headers(c: &Cell) -> &[String] {
                    c.ids.as_ref().map_or(&[][..], |i| i.headers.as_slice())
                }
                if headers(a) != headers(b) {
                    fix.push(("headers", header_refs(&w, b).unwrap_or_default(), revisions::before(revisions::TCPR, "headers")));
                }
                let id = |c: &Cell| c.ids.as_ref().and_then(|i| i.id.clone());
                if id(a) != id(b) {
                    let (start, at) = (tc.range().start, self.open_end(*tc));
                    let open = self.without_attrs(*tc, &self.xml[start..at], &[(Some(W), "id"), (None, "id")]);
                    let attr = id(b).map(|v| format!(" {w}id=\"{}\"", props::esc(&v))).unwrap_or_default();
                    out.push((start..at, format!("{}{attr}>", open.trim_end_matches('>'))));
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
    /// Also floats whose insertion or deletion changed (a shape inserted or deleted with tracking on): in their
    /// revision wrappers.
    fn moved_in(&self, p: &Paragraph, qs: &[&Paragraph]) -> String {
        let here =
            |id: u64, run: &Run| qs.iter().flat_map(|q| q.inlines.iter()).any(|i| matches!(i, Inline::Float(f, r) if f.id == id && same_rev(r, run)));
        let mut out = String::new();
        for i in &p.inlines {
            let Inline::Float(f, run) = i else { continue };
            // A new frame is written as its paragraphs, before this one.
            if here(f.id, run) || (f.frame_src.is_none() && f.is_frame()) {
                continue;
            }
            let (open, close) = revisions::wrappers(&self.w, run.rev.ins.as_deref(), run.rev.del.as_deref(), self.people);
            let mut push = |x: &str| out.push_str(&format!("{open}<{w}r>{x}</{w}r>{close}", w = self.w));
            // A pasted copy (a new id) is found by where it was read from, and gets drawing ids of its own.
            let copied = f.src.as_ref().and_then(|s| self.float_src.values().find(|(r, _)| r == s)).filter(|_| !self.float_src.contains_key(&f.id));
            let Some((r, was)) = self.float_src.get(&f.id).or(copied) else {
                if let Some(x) = self.new_float_xml(f, run) {
                    push(&x);
                }
                continue;
            };
            // A copy: the source as read (not the rewrites of the float it came from), its text box holding its own text.
            // A group's parts keep the text they were read with: their text boxes are not stories, so no edit reaches
            // them.
            let own = copied.and(f.text_box.as_ref()).and_then(|tb| self.nested_blocks(&tb.blocks).ok());
            let mut src = if copied.is_some() { self.xml[r.clone()].to_string() } else { self.src_text(r.clone()) };
            let text: Vec<drawing::Edit> = own.map_or(Vec::new(), |x| box_contents(&src).into_iter().map(|b| (b, x.clone())).collect());
            if drawing::is_vml(&src) {
                // What VML can't say is written as DrawingML.
                match drawing::vml_patch(&drawing::apply(&src, text), was, f).or_else(|| self.new_float_xml(f, run)) {
                    Some(x) => push(&x),
                    None => push(&src),
                }
                continue;
            }
            // The shape's own properties first (their ranges are in the source as read), then its placement.
            let mut edits = drawing::changed(was, f, &src).unwrap_or_default();
            edits.extend(text);
            if !edits.is_empty() {
                src = drawing::apply(&src, edits);
            }
            let d = match Placement::changed(was, f) {
                Some(pl) => pl.apply(&src),
                None => src,
            };
            push(&if copied.is_some() { self.new_doc_pr_ids(&d) } else { d });
        }
        out
    }

    /// The source element at exactly `r`, inside one of the part's paragraphs.
    /// A marker's element in the group's source paragraphs: a range or comment reference by its name and id, opaque
    /// content by its text.
    fn marker_range(&self, m: &Marker, srcs: &[&ParaSrc]) -> Option<Range<usize>> {
        let mut nodes = srcs.iter().flat_map(|x| x.ranges.iter()).filter_map(|r| self.paras.get(&r.start));
        match m {
            Marker::Bookmark(n) => {
                let (name, start) = n.strip_prefix('/').map_or((n.as_str(), true), |n| (n, false));
                let id = self.bookmark_id(name);
                let (el, by) = if start { ("bookmarkStart", None) } else { ("bookmarkEnd", Some(id.to_string())) };
                nodes.find_map(|p| {
                    p.descendants()
                        .find(|d| {
                            is_w(*d, el)
                                && match &by {
                                    None => d.attribute((W, "name")) == Some(name),
                                    Some(id) => d.attribute((W, "id")).map(str::trim) == Some(id.as_str()),
                                }
                        })
                        .map(|d| d.range())
                })
            }
            Marker::Opaque(o) => {
                let raw = o.xml.0.as_deref()?;
                srcs.iter().flat_map(|x| x.ranges.iter()).find_map(|r| self.xml[r.clone()].find(raw).map(|k| r.start + k..r.start + k + raw.len()))
            }
            _ => {
                let (name, id) = m.element()?;
                nodes.find_map(|n| {
                    n.descendants()
                        .find(|d| is_w(*d, name) && d.attribute((W, "id")).and_then(|v| v.trim().parse::<i64>().ok()) == Some(id))
                        .map(|d| d.range())
                })
            }
        }
    }

    /// `xml` with each `wp:docPr` given a new id (a copied drawing's must be unique).
    fn new_doc_pr_ids(&self, xml: &str) -> String {
        let ids: Vec<(Range<usize>, u32)> = tag_ids(xml, DRAWING_TAGS)
            .into_iter()
            .map(|(r, _)| {
                let id = self.ids.get();
                self.ids.set(id + 1);
                (r, id)
            })
            .collect();
        let mut out = xml.to_string();
        for (r, id) in ids.into_iter().rev() {
            out.replace_range(r, &id.to_string());
        }
        out
    }

    /// Content with no source (a new text box's) written from the model in this part: its bookmarks' ids are the
    /// part's.
    fn nested_blocks(&self, blocks: &[Block]) -> Result<String, String> {
        blocks_in(self.xml, self.w.clone(), blocks, &self.controls, self.people, self.bookmarks.clone())
    }

    /// A bookmark's id: the part's for its name, else a new one, the same for its start and end.
    fn bookmark_id(&self, name: &str) -> i64 {
        let mut map = self.bookmarks.borrow_mut();
        let map = map.get_or_insert_with(|| {
            let ids = |root: Node| -> HashMap<String, i64> {
                root.descendants()
                    .filter(|d| is_w(*d, "bookmarkStart"))
                    .filter_map(|d| Some((d.attribute((W, "name"))?.to_string(), d.attribute((W, "id"))?.trim().parse().ok()?)))
                    .collect()
            };
            match self.paras.values().next() {
                Some(n) => ids(n.document().root_element()),
                None => roxmltree::Document::parse(self.xml).map(|d| ids(d.root_element())).unwrap_or_default(),
            }
        });
        if let Some(&id) = map.get(name) {
            return id;
        }
        let id = map.values().max().map_or(0, |m| m + 1);
        map.insert(name.to_string(), id);
        id
    }

    /// Whether `xml` (content of a part's body) parses, with the part's namespaces.
    fn well_formed(&self, xml: &str) -> bool {
        let Some(root) = self.paras.values().next().map(|n| n.document().root_element()) else { return true };
        let decl: String =
            root.namespaces().map(|n| format!(" xmlns{}=\"{}\"", n.name().map_or(String::new(), |p| format!(":{p}")), props::esc(n.uri()))).collect();
        roxmltree::Document::parse(&format!("<x{decl}>{xml}</x>")).is_ok()
    }

    fn node_at(&self, r: Range<usize>) -> Option<Node<'a, 'i>> {
        self.paras.values().filter(|p| p.range().start <= r.start && r.end <= p.range().end).find_map(|p| p.descendants().find(|n| n.range() == r))
    }

    /// The `mc:AlternateContent` around the drawing at `r`, when it is its Choice's.
    fn alternate_content(&self, r: Range<usize>) -> Option<Range<usize>> {
        let ch = self.node_at(r)?.parent().filter(|c| is(*c, MC, "Choice"))?;
        ch.parent().filter(|a| is(*a, MC, "AlternateContent")).map(|a| a.range())
    }

    /// The Choice's `w:drawing` of the `mc:AlternateContent` at `r`.
    fn choice_drawing(&self, r: Range<usize>) -> Option<Range<usize>> {
        let ac = self.node_at(r).filter(|a| is(*a, MC, "AlternateContent"))?;
        let ch = ac.children().find(|c| is(*c, MC, "Choice"))?;
        ch.children().find(|d| is_w(*d, "drawing")).map(|d| d.range())
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

    /// `open`, element `n`'s start tag as written (or as [`Saver::tags`] opens it), without its attributes `names`
    /// (namespace and local name), each with the space before it.
    fn without_attrs(&self, n: Node, open: &str, names: &[(Option<&str>, &str)]) -> String {
        let base = n.range().start;
        let mut out = open.to_string();
        let gone: Vec<Range<usize>> = n.attributes().filter(|a| names.contains(&(a.namespace(), a.name()))).map(|a| a.range()).collect();
        for r in gone.iter().rev() {
            let from = self.xml[..r.start].trim_end().len();
            if r.end - base <= out.len() {
                out.replace_range(from - base..r.end - base, "");
            }
        }
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
    /// Content kept as read ([`Inline::Opaque`]).
    Opaque(Rc<super::model::Opaque>),
    /// A bookmark's start, or (`/` and its name) end.
    Bookmark(String),
}

impl Marker {
    /// The element it is in the source, and the id that names it there.
    fn element(&self) -> Option<(&'static str, i64)> {
        match self {
            Marker::Control(..) | Marker::Opaque(_) | Marker::Bookmark(_) => None,
            Marker::Range(r) => Some((range_element(r.kind), r.id)),
            Marker::Comment(id, _) => Some(("commentReference", id.0)),
        }
    }
}

impl Marker {
    /// What names it, whatever its formatting: two markers with the same key are the same marker.
    fn key(&self) -> (u8, i64) {
        match self {
            Marker::Control(id, start) => (u8::from(*start), id.0 as i64),
            Marker::Range(r) => (2 + r.kind as u8, r.id),
            Marker::Comment(id, _) => (u8::MAX, id.0),
            Marker::Opaque(o) => (u8::MAX - 1, Rc::as_ptr(o) as usize as i64),
            Marker::Bookmark(n) => {
                use std::hash::{Hash, Hasher};
                let mut h = std::hash::DefaultHasher::new();
                n.hash(&mut h);
                (u8::MAX - 2, h.finish() as i64)
            }
        }
    }
}

/// The paragraphs of `blocks`, those in tables too.
fn all_paras<'b>(blocks: &'b [Block], out: &mut Vec<&'b Paragraph>) {
    for b in blocks {
        match b {
            Block::Para(p) => out.push(p),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| all_paras(&c.blocks, out)),
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
        Inline::Opaque(o) => pend.push(Marker::Opaque(o.clone())),
        Inline::Bookmark(n) => pend.push(Marker::Bookmark(n.clone())),
        _ => {}
    }
}

fn same_inline(x: &Inline, y: &Inline) -> bool {
    match (x, y) {
        (Inline::Text(s, r), Inline::Text(t, q)) => s == t && Rc::ptr_eq(r, q),
        (Inline::Bookmark(a), Inline::Bookmark(b)) => a == b,
        (Inline::Opaque(a), Inline::Opaque(b)) => Rc::ptr_eq(a, b),
        (Inline::NoteRef { kind: k, id: a, custom: c, run: r }, Inline::NoteRef { kind: l, id: b, custom: d, run: q }) => {
            (k, a, c) == (l, b, d) && Rc::ptr_eq(r, q)
        }
        (Inline::Range(a), Inline::Range(b)) => a == b,
        (Inline::CommentRef { id: a, run: r }, Inline::CommentRef { id: b, run: q }) => a == b && Rc::ptr_eq(r, q),
        (Inline::ControlStart(a), Inline::ControlStart(b)) | (Inline::ControlEnd(a), Inline::ControlEnd(b)) => a == b,
        // A toggled checkbox keeps its run and changes its face.
        (Inline::Sym { ch: a, font: f, run: r, .. }, Inline::Sym { ch: b, font: g, run: q, .. }) => a == b && f == g && Rc::ptr_eq(r, q),
        // Moved or resized floats are written patched (their text boxes' content is its own container).
        (Inline::Float(a, r), Inline::Float(b, q)) => {
            a.id == b.id && Placement::changed(a, b).is_none() && !drawing::differs(a, b) && Rc::ptr_eq(r, q)
        }
        (Inline::FieldBegin { form: a, run: r, .. }, Inline::FieldBegin { form: b, run: q, .. }) => {
            a.as_ref().map(|f| &f.data) == b.as_ref().map(|f| &f.data) && Rc::ptr_eq(r, q)
        }
        // A resized picture is written patched.
        (Inline::Picture(a, r), Inline::Picture(b, q)) => a.image == b.image && a.w == b.w && a.h == b.h && a.alt == b.alt && Rc::ptr_eq(r, q),
        // A ruby whose text was formatted is written anew.
        (Inline::Ruby(a, r), Inline::Ruby(b, q)) => (Rc::ptr_eq(a, b) || a.xml.0.as_deref() == b.xml.0.as_deref()) && Rc::ptr_eq(r, q),
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
        // Characters XML can't hold are left out (a control character written as text would come back as a break).
        Inline::Text(s, _) => out.extend(s.chars().filter(|&c| crate::docx::xml::is_char(c)).map(|c| (c as u64, c))),
        Inline::Tab(_) => out.push(('\t' as u64, '\t')),
        Inline::Break(BreakKind::Line, _) => out.push((0xB, '\u{b}')),
        Inline::Break(BreakKind::ClearLeft, _) => out.push((ATOM | BreakKind::ClearLeft as u64, '\u{1}')),
        Inline::Break(BreakKind::ClearRight, _) => out.push((ATOM | BreakKind::ClearRight as u64, '\u{2}')),
        Inline::Break(BreakKind::ClearAll, _) => out.push((ATOM | BreakKind::ClearAll as u64, '\u{3}')),
        Inline::Break(BreakKind::Column, _) => out.push((ATOM | BreakKind::Column as u64, '\u{e}')),
        Inline::Break(k, _) => out.push((ATOM | *k as u64, '\u{c}')),
        Inline::Bookmark(_)
        | Inline::Opaque(_)
        | Inline::Range(_)
        | Inline::CommentRef { .. }
        | Inline::Float(..)
        | Inline::ControlStart(_)
        | Inline::ControlEnd(_) => {}
        other => {
            let mut h = std::hash::DefaultHasher::new();
            // By what the object is, not its formatting: reformatting keeps it.
            std::mem::discriminant(other).hash(&mut h);
            match other {
                // By its drawing as read, else its image: a resized or refilled picture is kept, patched (`Emit::seg`).
                Inline::Picture(p, _) => match &p.src.0 {
                    Some(src) => (Rc::as_ptr(src) as *const u8 as usize).hash(&mut h),
                    None => p.image.hash(&mut h),
                },
                Inline::NoteRef { kind, id, .. } => (*kind as u8, id).hash(&mut h),
                // A checkbox by its control: toggling keeps it (its face is patched, `Emit::seg`).
                Inline::Sym { control: Some(c), .. } => c.hash(&mut h),
                Inline::Sym { ch, font, .. } => (ch, font).hash(&mut h),
                Inline::PTab(t, _) => t.hash(&mut h),
                // A ruby by its XML: formatting its text writes it anew.
                Inline::Ruby(r, _) => r.xml.0.as_deref().hash(&mut h),
                // A field's markers by their field: editing its code keeps them; a form field's changed data is
                // written anew.
                Inline::FieldBegin { id, form, .. } => (id, form.as_ref().is_some_and(|f| f.data != f.read)).hash(&mut h),
                Inline::FieldSep { id, .. } | Inline::FieldEnd { id, .. } => id.hash(&mut h),
                _ => {}
            }
            // A checkbox's face writes as its character; other objects (a symbol too) as U+FFFC.
            let ch = if let Inline::Sym { ch, control: Some(_), .. } = other { *ch } else { '\u{FFFC}' };
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
    /// An inline picture's size, alternative text and shape tree (to see them changed).
    size: Option<PicState>,
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
    size: Option<PicState>,
    /// The markers right before it (inline controls, comment and move ranges, comment references, in order): written
    /// when the group is written from the model.
    ctl: Vec<Marker>,
    /// Those of `ctl` the group's source doesn't have, when the source is walked: written where they are now.
    fresh: Vec<Marker>,
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

/// A source revision wrapper: whether it deletes (`w:del`, `w:moveFrom`) or inserts, whether it moves, and its `w:id`.
#[derive(Clone, Copy)]
struct Wrapper {
    del: bool,
    moved: bool,
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
        let moved = is_w(n, "moveFrom") || is_w(n, "moveTo");
        Some(Wrapper { del, moved, id: n.attribute((W, "id")).and_then(|v| v.trim().parse().ok()) })
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
    /// Bookmarks (`/` and its name for an end) and kept content the group's paragraphs were read with that sit
    /// outside them: kept there too.
    marks_outside: (Vec<String>, Vec<*const super::model::Opaque>),
    /// The move marks of the group's runs, for new move ranges' authors and dates.
    moves: Vec<Rc<RevisionMark>>,
    /// Gaps whose insertions split by the markers in them: how many go before the unit before's end (see `split`).
    cut: HashMap<usize, usize>,
    /// Cur units written inside an empty content control, by its content's start.
    fills: HashMap<usize, Vec<usize>>,
    /// Simple fields and hyperlinks walked into, by start: their begin, separator and end units.
    fields: HashMap<usize, (usize, usize, usize)>,
    /// Content controls gone since reading, by start: their content is walked without their markup.
    unwrap: Vec<usize>,
    /// Changed controls' `w:sdtPr` ranges, with the properties written there.
    prs: Vec<(Range<usize>, String)>,
    /// Fresh markers written so far ([`Cur::fresh`]).
    fresh_written: usize,
}

impl<'s, 'a, 'i> Emit<'s, 'a, 'i> {
    /// A group's XML: its source walked, with markers added since reading written where they are, when that comes out
    /// whole (well-formed, every added marker written); else as [`Emit::group_as`] without them.
    fn group(s: &'s Saver<'a, 'i>, qs: &[&Paragraph], srcs: &[&ParaSrc], ps: &[&Paragraph]) -> Result<String, String> {
        match Emit::group_as(s, qs, srcs, ps, true)? {
            Some(x) => Ok(x),
            None => Emit::group_as(s, qs, srcs, ps, false).map(|x| x.unwrap_or_default()),
        }
    }

    /// With `fresh`, `None` when the source can't be walked with the markers added since reading, or what that wrote
    /// isn't whole.
    fn group_as(s: &'s Saver<'a, 'i>, qs: &[&Paragraph], srcs: &[&ParaSrc], ps: &[&Paragraph], fresh: bool) -> Result<Option<String>, String> {
        // The source text can be walked when every unit has its element and no element holds another's.
        let mut segs = Vec::new();
        let mut orig = Vec::new();
        // The markers before each orig unit (see `Cur::ctl`).
        let mut orig_ctl: Vec<Vec<Marker>> = Vec::new();
        let mut mapped = srcs.iter().all(|x| !x.lossy);
        let mut seps = Vec::new();
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
                if matches!(i, Inline::FieldSep { .. }) {
                    seps.push(orig.len() - 1);
                }
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
                    fresh: Vec::new(),
                    pr: pr.clone(),
                }));
            }
            cur.push(Cur { key: MARK, ch: '\n', code: false, run: None, obj: None, size: None, ctl: pend, fresh: Vec::new(), pr: None });
        }
        let keys_of = |orig: &[Orig]| orig.iter().map(|o| o.key).collect::<Vec<_>>();
        let cur_keys: Vec<u64> = cur.iter().map(|c| c.key).collect();
        let mut kept = diff(&keys_of(&orig), &cur_keys);
        // The source's inline controls, comment and move ranges and comment references are walked as they were: right
        // when they come in the same order against the kept text (those of content controls gone since reading left
        // out: their markup is unwrapped), each control among them changed since reading is a content control of the
        // group's source (its properties patched in), and every insertion next to them has a place among them.
        // Otherwise the group is written from the model, its controls from their properties.
        let mut sdts: HashMap<usize, (Option<usize>, Option<Range<usize>>)> = HashMap::new();
        for n in srcs.iter().flat_map(|x| x.ranges.iter()).filter_map(|r| s.paras.get(&r.start)) {
            for d in n.descendants().filter(|d| is_w(*d, "sdt")) {
                sdts.insert(d.range().start, (child(d, "sdtContent").map(|c| c.range().start), child(d, "sdtPr").map(|p| p.range())));
            }
        }
        let sdt_of = |id: &ControlId| s.controls.get(id).and_then(|k| k.at).map(|a| a.0).filter(|a| sdts.contains_key(a));
        let mut now_ids = Vec::new();
        for m in cur.iter().flat_map(|c| c.ctl.iter()) {
            if let Marker::Control(id, _) = m
                && !now_ids.contains(id)
            {
                now_ids.push(*id);
            }
        }
        let mut gone = Vec::new();
        for m in orig_ctl.iter().flatten() {
            if let Marker::Control(id, _) = m
                && !now_ids.contains(id)
                && !gone.contains(id)
                && sdt_of(id).is_some()
            {
                gone.push(*id);
            }
        }
        let live: Vec<Vec<Marker>> =
            orig_ctl.iter().map(|ms| ms.iter().filter(|m| !matches!(m, Marker::Control(id, _) if gone.contains(id))).cloned().collect()).collect();
        // Markers the source doesn't have, set aside to be written where they are (a new comment's range, a new
        // control's markup).
        let mut fresh_ids = Vec::new();
        if fresh {
            let had: std::collections::HashSet<(u8, i64)> = orig_ctl.iter().flatten().map(Marker::key).collect();
            for c in &mut cur {
                let (old, new): (Vec<Marker>, Vec<Marker>) = std::mem::take(&mut c.ctl).into_iter().partition(|m| had.contains(&m.key()));
                c.ctl = old;
                fresh_ids.extend(new.iter().filter_map(|m| if let Marker::Control(id, _) = m { Some(*id) } else { None }));
                c.fresh = new;
            }
        }
        let fresh_count: usize = cur.iter().map(|c| c.fresh.len()).sum();
        now_ids.retain(|id| !fresh_ids.contains(id));
        // Markers gone since reading (a deleted comment's range and reference, deleted opaque content), their source
        // elements left out of the walk; controls go as `gone`.
        let mut removed = Vec::new();
        if fresh {
            let now: std::collections::HashSet<(u8, i64)> = cur.iter().flat_map(|c| c.ctl.iter().chain(&c.fresh)).map(Marker::key).collect();
            for m in orig_ctl.iter().flatten().filter(|m| !matches!(m, Marker::Control(..)) && !now.contains(&m.key())) {
                match s.marker_range(m, srcs) {
                    Some(r) => removed.push((m.key(), r)),
                    None => return Ok(None),
                }
            }
        }
        let live: Vec<Vec<Marker>> =
            live.into_iter().map(|ms| ms.into_iter().filter(|m| !removed.iter().any(|(k, _)| *k == m.key())).collect()).collect();
        let faithful = |kept: &[Option<usize>]| {
            let mut taken = vec![false; cur.len()];
            kept.iter().flatten().for_each(|&j| taken[j] = true);
            let a = live.iter().zip(kept).flat_map(|(ms, k)| ms.iter().map(Err).chain(k.map(Ok)));
            let b = cur.iter().zip(&taken).enumerate().flat_map(|(j, (c, t))| c.ctl.iter().map(Err).chain(t.then_some(Ok(j))));
            a.eq(b) && now_ids.iter().all(|id| s.controls.get(id).is_some_and(|k| k.as_read() || sdt_of(id).is_some()))
        };
        mapped &= faithful(&kept);
        let mut ins = gaps(&kept, cur.len());
        let (mut cut, mut fills) = (HashMap::new(), HashMap::new());
        let content = |id: ControlId| sdt_of(&id).and_then(|a| sdts[&a].0);
        mapped = mapped && place(&orig, &live, &cur, &kept, &mut ins, &mut cut, &mut fills, &content);
        if !mapped && (fresh_count > 0 || !removed.is_empty()) {
            return Ok(None);
        }
        if !mapped {
            // shortcut: written from the model (an unmappable source such as a field across runs of different parents,
            // or inline markers added, removed or moved), so the group's content the model doesn't hold (bookmarks,
            // hyperlinks) is lost. Revisions, comment and move ranges and comment references are written from the model.
            orig.retain(|o| o.key == MARK);
            segs.clear();
            kept = diff(&keys_of(&orig), &cur_keys);
            ins = gaps(&kept, cur.len());
            cut.clear();
            fills.clear();
        }
        let unwrap: Vec<usize> = if mapped { gone.iter().filter_map(&sdt_of).collect() } else { Vec::new() };
        let prs: Vec<(Range<usize>, String)> = now_ids
            .iter()
            .filter(|_| mapped)
            .filter_map(|id| {
                let k = s.controls.get(id).filter(|k| !k.as_read())?;
                let r = sdts.get(&sdt_of(id)?)?.1.clone()?;
                Some((r, sdt_pr(&control_open(&s.w, *id, k), &s.w).to_string()))
            })
            .filter(|(_, x)| !x.is_empty())
            .collect();
        // Simple fields and hyperlinks whose code and markers are kept are walked into, their result's elements segs of
        // their own.
        let mut fields = HashMap::new();
        if mapped {
            let inner: HashMap<usize, &[(Range<usize>, usize)]> =
                srcs.iter().flat_map(|x| x.simple.iter()).map(|(k, v)| (*k, v.as_slice())).collect();
            let mut k = 0;
            while k < segs.len() {
                let (start, first, n) = (segs[k].range.start, segs[k].first, segs[k].n);
                let walkable = inner.get(&start).filter(|_| n >= 3).and_then(|v| {
                    let end = first + n - 1;
                    let sep = seps.iter().copied().find(|&i| first < i && i < end)?;
                    let code = (first..=sep).all(|i| kept[i].is_some_and(|j| i == first || kept[i - 1] == Some(j - 1)));
                    (code && kept[end].is_some() && v.iter().map(|x| x.1).sum::<usize>() == end - sep - 1).then_some((sep, end, *v))
                });
                let Some((sep, end, v)) = walkable else {
                    k += 1;
                    continue;
                };
                fields.insert(start, (first, sep, end));
                let mut at = sep + 1;
                let parts: Vec<Seg> = v
                    .iter()
                    .map(|(r, n)| {
                        at += n;
                        Seg { range: r.clone(), first: at - n, n: *n }
                    })
                    .collect();
                segs.splice(k..=k, parts);
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
            .filter(|n| !s.dropped.contains(&n.range()))
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
                let sect = s.made.get(&p.id).map(String::as_str).or(sect.filter(|_| last));
                match by_style.or(by_src).or((!qs.is_empty()).then_some(0)) {
                    Some(k) => {
                        let (mut open, close) = s.tags(nodes[k]);
                        if std::mem::replace(&mut used[k], true) {
                            open = s.without_attrs(nodes[k], &open, &[(Some(W14), "paraId"), (Some(W14), "textId")]);
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
        let floats = |ps: &[&Paragraph]| -> Vec<(u64, Option<Range<usize>>, Run)> {
            ps.iter()
                .flat_map(|p| p.inlines.iter())
                .filter_map(|i| if let Inline::Float(f, r) = i { Some((f.id, f.src.clone(), r.clone())) } else { None })
                .collect()
        };
        // A float whose insertion or deletion changed is written again, in its wrappers (`moved_in`).
        let now = floats(ps);
        let dropped = floats(qs)
            .into_iter()
            .filter(|(id, _, r)| !now.iter().any(|(n, _, q)| n == id && same_rev(r, q)))
            .filter_map(|(_, src, _)| src)
            .chain(removed.iter().map(|(_, r)| r.clone()))
            .collect();
        let all = |ps: &[&Paragraph]| -> Vec<Float> {
            ps.iter().flat_map(|p| p.inlines.iter()).filter_map(|i| if let Inline::Float(f, _) = i { Some((**f).clone()) } else { None }).collect()
        };
        let (was, is) = (all(qs), all(ps));
        let patched = was
            .iter()
            .filter_map(|a| {
                let b = is.iter().find(|b| b.id == a.id)?;
                let src = a.src.clone()?;
                // A VML float's edits patch its VML (`style`, fill and stroke attributes).
                let text = s.src_text(src.clone());
                if drawing::is_vml(&text) {
                    // What VML can't say is written as DrawingML.
                    let now = drawing::vml_patch(&text, a, b).or_else(|| {
                        let run = ps.iter().flat_map(|p| p.inlines.iter()).find_map(|i| match i {
                            Inline::Float(f, r) if f.id == b.id => Some(r),
                            _ => None,
                        })?;
                        s.new_float_xml(b, run)
                    })?;
                    return (now != text).then(|| (src, Placement { replace: Some(now), ..Placement::default() }));
                }
                // The shape's own properties (their ranges are in the source as read), then its placement.
                let mut shape = drawing::changed(a, b, &text);
                // A story added by typing into the shape: its `wps:txbx`, written from the model.
                if let (Some(at), Some(tb)) = (drawing::story_added(a, b), b.text_box.as_ref())
                    && let Ok(blocks) = s.nested_blocks(&tb.blocks)
                {
                    let txbx = format!("{}<{w}txbxContent>{blocks}</{w}txbxContent></wps:txbx>", drawing::txbx_open(b.shape.as_deref()), w = s.w);
                    shape.get_or_insert_with(Vec::new).push((at..at, txbx));
                }
                // A floating picture control drops the fallback copy beside it: its bare Choice drawing.
                let bare = b.picture_control(&s.controls).is_some() && s.alternate_content(src.clone()).is_some();
                let edited = shape.is_some() || bare;
                let mut p = match Placement::changed(a, b) {
                    Some(p) => Placement { shape, ..p },
                    None if bare => Placement { shape, ..Placement::default() },
                    None => Placement { shape: Some(shape?), ..Placement::default() },
                };
                // an edited shape inside `mc:AlternateContent` is written as its bare Choice drawing.
                match s.alternate_content(src.clone()).filter(|_| edited) {
                    Some(ac) => {
                        p.inner = Some(src.start - ac.start..src.end - ac.start);
                        Some((ac, p))
                    }
                    None => Some((src, p)),
                }
            })
            .collect();
        let seg_at = segs.iter().enumerate().map(|(k, g)| (g.range.start, k)).collect();
        let seg_starts = segs.iter().map(|g| g.range.start).collect();
        let (mut marker_src, mut marker_outside, mut marks_outside, mut moves) = (HashMap::new(), Vec::new(), (Vec::new(), Vec::new()), Vec::new());
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
            for m in read.iter().filter(|m| s.marker_range(m, srcs).is_none()) {
                match m {
                    Marker::Bookmark(n) => marks_outside.0.push(n.clone()),
                    Marker::Opaque(o) => marks_outside.1.push(Rc::as_ptr(o)),
                    _ => {}
                }
            }
        }
        if !mapped || fresh_count > 0 {
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
            marks_outside,
            moves,
            cut,
            fills,
            fields,
            unwrap,
            prs,
            fresh_written: 0,
        };
        // Hidden marks are kept only while no paragraph mark comes or goes (they would mark the wrong paragraph).
        let stable = e.ins.iter().flatten().all(|&j| e.cur[j].run.is_some()) && e.orig.iter().zip(&e.kept).all(|(o, k)| o.key != MARK || k.is_some());
        let mut mark = 0;
        for (q, src) in qs.iter().zip(srcs) {
            let start = mark;
            mark += if mapped { q.inlines.iter().map(Inline::units).sum::<usize>() } else { 0 };
            if mapped {
                e.insert(start, None, false);
                for (k, r) in src.ranges.iter().enumerate() {
                    let node = s.paras[&r.start];
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
        if fresh_count > 0 && (e.fresh_written != fresh_count || !s.well_formed(&e.out)) {
            return Ok(None);
        }
        Ok(Some(e.out))
    }

    /// Where insertions at gap `g` split between its neighbours: those in the next unit's run go before it (the
    /// tail), the rest after the unit before (the head); after a paragraph mark or at the start, all go before.
    fn split(&self, g: usize) -> usize {
        if let Some(&c) = self.cut.get(&g) {
            return c;
        }
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
            if let Some(&(begin, sep, end)) = self.fields.get(&r.start) {
                self.insert(begin, None, true);
                self.flush();
                let (open, close) = self.s.tags(c);
                self.stack.push(Open { open, close, run: false, link_end: None, rev: None });
                self.open_to(self.stack.len());
                self.insert(sep + 1, None, false);
                self.walk(c);
                self.insert(end, None, true);
                self.pop();
                self.fmt_run = None;
                self.insert_after(end + 1, None, r.end);
                continue;
            }
            if self.unwrap.contains(&r.start) && is_w(c, "sdt") {
                if let Some(content) = child(c, "sdtContent") {
                    self.walk(content);
                }
                continue;
            }
            let inside = self.seg_starts.partition_point(|&x| x <= r.start);
            let fill = self.fills.keys().any(|&f| r.start <= f && f < r.end);
            if c.is_element() && (fill || self.seg_starts.get(inside).is_some_and(|&x| x < r.end)) {
                let (mut open, close) = self.s.tags(c);
                let run = is_w(c, "r");
                if let Some(rpr) = child(c, "rPr").filter(|_| run) {
                    open.push_str(&self.s.xml[rpr.range()]);
                }
                self.flush();
                let link_end = is_w(c, "hyperlink").then_some(r.end);
                self.stack.push(Open { open, close, run, link_end, rev: Wrapper::of(c) });
                self.walk(c);
                for j in self.fills.remove(&r.start).unwrap_or_default() {
                    self.emit(j, None);
                }
                self.pop();
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

    /// Closes the innermost walked element.
    fn pop(&mut self) {
        self.flush();
        if self.depth == self.stack.len() {
            self.close_wrap();
            self.depth -= 1;
            self.out.push_str(&self.stack[self.depth].close);
        }
        self.stack.pop();
        self.depth = self.depth.min(self.stack.len());
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
        }) && (first + 1..first + n).all(|g| self.ins[g].is_empty() && self.kept[g].is_none_or(|j| self.cur[j].fresh.is_empty()));
        // A checkbox toggled, or its control's properties changed: its control written with the new
        // properties, state and face.
        let checkbox = (n == 1 && !in_run && self.s.xml[range.clone()].contains("checkbox")).then(|| self.kept[first]).flatten();
        let toggled = checkbox.map(|j| (self.cur[j].ch, self.cur[j].pr.clone())).filter(|(ch, pr)| *ch != self.orig[first].ch || pr.is_some());
        if let Some((ch, pr)) = toggled {
            self.insert(first, None, true);
            self.fresh_before(first);
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
            self.fresh_before(first);
            if in_run {
                self.add(To::Src, None, Some(range.clone()));
            } else {
                self.verbatim(range.clone());
            }
            self.fmt_run = None;
            self.insert_after(first + n, ctx(self, first + n - 1), range.end);
            return;
        }
        if n > 1 && (!in_run || !text) {
            // A changed element holding several units (a simple field, a run child such as a fallback's text):
            // written from the model, as what each unit is now (a simple field becomes a complex one).
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
            self.fresh_before(i);
            if let Some(j) = self.kept[i] {
                let run = self.cur[j].run.clone().expect("a unit in a run");
                if !in_run {
                    // A whole field (its runs) or a simple field: reformatted, every run of it takes the formatting.
                    // A content control around a resized picture: its drawing with the new extents.
                    if let Some(size) = resized(self, i).filter(|_| Rc::as_ptr(&run) == self.orig[i].run) {
                        let was = self.orig[i].size.as_ref();
                        let p = Placement { decorative: None, nv: None, ..size.placement(was) };
                        let src = self.src(range.clone());
                        let x = self.s.picture_patched(&src, p, was, &size);
                        self.flush();
                        self.open_to(self.stack.len());
                        self.out.push_str(&x);
                        self.fmt_run = None;
                    } else if Rc::as_ptr(&run) == self.orig[i].run {
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
                        // A resized or reformatted inline picture: its drawing with the new extents and properties.
                        self.add(to, None, None);
                        self.flush_text();
                        let was = self.orig[i].size.as_ref();
                        let mut p = size.placement(was);
                        if let (Some(a), Some(b)) = (was.and_then(|s| s.0.shape.as_deref()), size.0.shape.as_deref()) {
                            let d = self.s.choice_drawing(range.clone()).unwrap_or(range.clone());
                            p.shape = drawing::shape_changed(a, b, d.start, &self.s.src_text(d.clone()));
                            // an edited shape drops its VML fallback.
                            if p.shape.is_some() && d != range {
                                p.inner = Some(d.start - range.start..d.end - range.start);
                            }
                        }
                        let src = self.s.src_text(range.clone());
                        let x = self.s.picture_patched(&src, p, self.orig[i].size.as_ref(), &size);
                        self.pend.push_str(&x);
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

    /// The markers added before orig unit `i`'s unit now, when it is kept.
    fn fresh_before(&mut self, i: usize) {
        if let Some(j) = self.kept[i].filter(|&j| !self.cur[j].fresh.is_empty()) {
            self.markers(j);
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
        let w = self.s.w.clone();
        let ms = if self.model { std::mem::take(&mut self.cur[j].ctl) } else { std::mem::take(&mut self.cur[j].fresh) };
        if !self.model {
            self.fresh_written += ms.len();
        }
        for m in ms {
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
            match &m {
                Marker::Bookmark(n) if self.marks_outside.0.contains(n) => continue,
                Marker::Opaque(o) if self.marks_outside.1.contains(&Rc::as_ptr(o)) => continue,
                _ => {}
            }
            match m {
                Marker::Control(id, start) => {
                    let Some(c) = self.s.controls.get(&id) else { continue };
                    self.flush();
                    let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
                    self.open_to(level);
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
                Marker::Bookmark(n) => {
                    self.flush();
                    let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
                    self.open_to(level);
                    let (name, start) = n.strip_prefix('/').map_or((n.as_str(), true), |n| (n, false));
                    let id = self.s.bookmark_id(name);
                    let x = if start {
                        format!("<{w}bookmarkStart {w}id=\"{id}\" {w}name=\"{}\"/>", props::esc(name))
                    } else {
                        format!("<{w}bookmarkEnd {w}id=\"{id}\"/>")
                    };
                    self.out.push_str(&x);
                }
                Marker::Opaque(o) => {
                    self.flush();
                    let level = self.stack.iter().rposition(|o| o.run).unwrap_or(self.stack.len());
                    self.open_to(level);
                    self.out.push_str(o.xml.0.as_deref().unwrap_or(""));
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
            (Some(c @ '\u{1}'..='\u{3}'), _) => {
                self.flush_text();
                let clear = ["left", "right", "all"][c as usize - 1];
                self.pend.push_str(&format!("<{w}br {w}clear=\"{clear}\"/>"));
            }
            (Some(c), _) => self.text.push(c),
            (None, Some(r)) => {
                self.flush_text();
                // With what is nested in it rewritten (an inline shape's text) and its floats patched.
                let x = self.src(r);
                self.pend.push_str(&x);
            }
            (None, None) => {}
        }
    }

    /// Pending text as run content: deleted text (a deleted run's, or a source run's in a deletion) in `w:delText` or
    /// `w:delInstrText`. Text moved away stays `w:t`, as Word writes it: Word refuses a `w:delText` in a `w:moveFrom`.
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let w = &self.s.w;
            let t = props::esc_text(&self.text);
            let deleted = match &self.to {
                Some(To::Own(run)) => run.rev.del.as_ref().is_some_and(|m| m.kind != crate::docx::review::MarkKind::MoveFrom),
                _ => self.stack.iter().any(|o| o.rev.is_some_and(|r| r.del && !r.moved)),
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

    /// Source bytes, with moved or resized floats and changed controls' properties in them patched.
    fn src(&self, r: Range<usize>) -> String {
        // Floats patched in place, the rest with its nested rewrites (text boxes) applied.
        let floats = self.patched.iter().map(|(d, p)| (d.clone(), Ok(p)));
        let mut inside: Vec<(Range<usize>, Result<&Placement, &str>)> =
            floats.chain(self.prs.iter().map(|(d, x)| (d.clone(), Err(x.as_str())))).filter(|(d, _)| r.start <= d.start && d.end <= r.end).collect();
        inside.sort_by_key(|(d, _)| d.start);
        let mut out = String::new();
        let mut at = r.start;
        for (d, p) in inside {
            if d.start < at {
                continue;
            }
            out.push_str(&self.s.src_text(at..d.start));
            match p {
                Ok(p) => out.push_str(&p.apply(&self.s.src_text(d.clone()))),
                Err(x) => out.push_str(x),
            }
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

/// The cur units inserted before each orig unit (`[kept.len()]`: at the end): after the last unit kept before them.
fn gaps(kept: &[Option<usize>], n: usize) -> Vec<Vec<usize>> {
    let mut ins = vec![Vec::new(); kept.len() + 1];
    let mut taken = vec![false; n];
    for j in kept.iter().flatten() {
        taken[*j] = true;
    }
    let mut gap = 0;
    let mut ki = 0;
    for j in 0..n {
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
    ins
}

/// A place for insertions in a gap: after orig unit `x - 1`, before unit `x` (after the markers before it), or inside
/// an empty content control (by its content's start).
enum Place {
    Head(usize),
    Tail(usize),
    In(usize),
}

/// Insertions in gaps with markers, placed against them: each at the first place, from the one before's on, with as
/// many of the gap's markers before it as before it now. False when one has no such place.
#[allow(clippy::too_many_arguments)]
fn place(
    orig: &[Orig],
    marks: &[Vec<Marker>],
    cur: &[Cur],
    kept: &[Option<usize>],
    ins: &mut [Vec<usize>],
    cut: &mut HashMap<usize, usize>,
    fills: &mut HashMap<usize, Vec<usize>>,
    content: &dyn Fn(ControlId) -> Option<usize>,
) -> bool {
    let at = |x: usize| marks.get(x).map_or(&[][..], Vec::as_slice);
    for g in 0..ins.len() {
        if ins[g].is_empty() {
            continue;
        }
        let h = (g..orig.len()).find(|&x| kept[x].is_some()).unwrap_or(orig.len());
        if (g..=h).all(|x| at(x).is_empty()) {
            continue;
        }
        let mut places = Vec::new();
        let mut n = 0;
        for x in g..=h {
            if x < orig.len() {
                places.push((n, Place::Head(x)));
            }
            for (k, w) in at(x).windows(2).enumerate() {
                if let [Marker::Control(a, true), Marker::Control(b, false)] = w
                    && a == b
                    && let Some(c) = content(*a)
                {
                    places.push((n + k + 1, Place::In(c)));
                }
            }
            n += at(x).len();
            places.push((n, Place::Tail(x)));
        }
        let mut chosen = Vec::new();
        let (mut p, mut before) = (0, 0);
        for &j in &ins[g] {
            before += cur[j].ctl.len();
            while places.get(p).is_some_and(|q| q.0 < before) {
                p += 1;
            }
            if places.get(p).is_none_or(|q| q.0 != before) {
                return false;
            }
            chosen.push((p, j));
        }
        ins[g].clear();
        let mut heads = vec![Vec::new(); h - g + 1];
        let mut tails = vec![Vec::new(); h - g + 1];
        for (p, j) in chosen {
            match places[p].1 {
                Place::Head(x) => heads[x - g].push(j),
                Place::Tail(x) => tails[x - g].push(j),
                Place::In(c) => fills.entry(c).or_default().push(j),
            }
        }
        for (k, (head, tail)) in heads.into_iter().zip(tails).enumerate() {
            cut.insert(g + k, head.len());
            ins[g + k] = [head, tail].concat();
        }
    }
    true
}

/// [`Emit::group`], or nothing for no paragraphs (the sources' text is deleted).
fn emit(s: &Saver, qs: &[&Paragraph], srcs: &[&ParaSrc], ps: &[&Paragraph]) -> Result<String, String> {
    if ps.is_empty() {
        return Ok(String::new());
    }
    // Frames made since reading: their paragraphs, each with its `w:framePr`, go before the paragraph they anchor at.
    let mut out: String = ps
        .iter()
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            Inline::Float(f, _) if f.frame_src.is_none() && f.is_frame() => f.text_box.as_ref(),
            _ => None,
        })
        .map(|tb| s.nested_blocks(&tb.blocks))
        .collect::<Result<_, _>>()?;
    out.push_str(&Emit::group(s, qs, srcs, ps)?);
    Ok(out)
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
    crate::docx::xml::set_checkbox_checked(&mut s, checked);
    // The face: the content's first text (the first `t` start tag, any prefix).
    let start = s.match_indices(":t").map(|(k, _)| k).find(|&k| {
        let lt = s[..k].rfind('<');
        lt.is_some_and(|lt| !s[lt + 1..k].contains('/') && !s[lt + 1..k].contains(' ')) && matches!(s.as_bytes().get(k + 2), Some(b'>' | b' '))
    });
    if let Some(t) = start.and_then(|k| s[k..].find('>').map(|g| k + g + 1))
        && let Some(e) = s[t..].find('<').map(|e| t + e)
    {
        s.replace_range(t..e, &props::esc_text(face.encode_utf8(&mut [0; 4])));
    }
    s
}

/// An inline picture or shape as it is, compared but for its text's blocks (written as its own container).
#[derive(Clone, Debug)]
struct PicState(Picture);

impl PartialEq for PicState {
    fn eq(&self, o: &PicState) -> bool {
        self.0.same_but_story(&o.0)
    }
}

fn picture_size(i: &Inline) -> Option<PicState> {
    if let Inline::Picture(p, _) = i { Some(PicState(p.clone())) } else { None }
}

impl PicState {
    /// The placement patch a kept picture's drawing takes to be this from `was`.
    fn placement(&self, was: Option<&PicState>) -> Placement {
        let p = &self.0;
        let a = was.map(|w| &w.0);
        Placement {
            size: Some((p.w, p.h)),
            alt: Some(p.alt.to_string()),
            decorative: Some(decorative(&p.shape)),
            effect: Some(p.effect),
            nv: nv_changed(a.and_then(|a| a.shape.as_deref()), p.shape.as_deref()),
            lock: a.filter(|a| a.lock_aspect != p.lock_aspect).map(|_| p.lock_aspect),
            ..Placement::default()
        }
    }
}

impl Saver<'_, '_> {
    /// A kept picture's drawing (`src`) as it is now: its extents, alternative text and, when it shows another image
    /// than it was read with, a blip naming that image. A VML picture edited past what VML says is written as
    /// DrawingML.
    fn picture_patched(&self, src: &str, p: Placement, was: Option<&PicState>, now: &PicState) -> String {
        if drawing::is_vml(src) {
            let a = was.and_then(|w| w.0.shape.as_deref());
            match drawing::vml_picture_patch(src, a, now.0.shape.as_deref(), now.0.w, now.0.h) {
                Some(x) => return picture_relinked(x, was.and_then(|w| w.0.image), now.0.image),
                None => {
                    if let Some(x) = self.new_picture_xml(&now.0) {
                        return x;
                    }
                }
            }
        }
        picture_patched(src, p, was.and_then(|w| w.0.image), now.0.image)
    }
}

/// A VML picture's text naming image `now` when it shows another than `was`.
fn picture_relinked(x: String, was: Option<usize>, now: Option<usize>) -> String {
    let image = now.filter(|_| now != was);
    match (image, find_tag(&x, "imagedata", 0)) {
        // Its `r:id`, else the `o:relid` read when it has none.
        (Some(k), Some((start, end))) => {
            match x[start..end].find(":id=\"").map(|a| a + 5).or_else(|| x[start..end].find(":relid=\"").map(|a| a + 8)) {
                Some(a) => {
                    let v0 = start + a;
                    let v1 = x[v0..end].find('"').map_or(end, |e| v0 + e);
                    format!("{}{NEW_IMAGE}{k}{}", &x[..v0], &x[v1..])
                }
                None => x,
            }
        }
        _ => x,
    }
}

/// A kept picture's drawing (`src`) as it is now: its extents, alternative text and, when it shows another image
/// than it was read with, a blip naming that image.
fn picture_patched(src: &str, p: Placement, was: Option<usize>, now: Option<usize>) -> String {
    let image = now.filter(|_| now != was);
    if drawing::is_vml(src) {
        let (w, h) = p.size.unwrap_or_default();
        let x = drawing::vml_size(src, w, h);
        return match (image, find_tag(&x, "imagedata", 0)) {
            // Its `r:id`, else the `o:relid` read when it has none.
            (Some(k), Some((start, end))) => {
                match x[start..end].find(":id=\"").map(|a| a + 5).or_else(|| x[start..end].find(":relid=\"").map(|a| a + 8)) {
                    Some(a) => {
                        let v0 = start + a;
                        let v1 = x[v0..end].find('"').map_or(end, |e| v0 + e);
                        format!("{}{NEW_IMAGE}{k}{}", &x[..v0], &x[v1..])
                    }
                    None => x,
                }
            }
            _ => x,
        };
    }
    Placement { blip: image, ..p }.apply(src)
}

/// Marks [`Saver::object_xml`] content written between runs, not in one.
const PARA_LEVEL: &str = "\u{0}P";

/// Where new drawings' `wp:docPr/@id`s start: above what Word gives a document's own drawings.
const DRAWING_IDS: u32 = 0x4345_0000;

/// What a new picture's image is embedded by until saving: this and its index in [`Document::images`].
/// [`add_images`] gives each part that names one the image, a relationship of its own and that relationship's id. It
/// holds a colon, which a relationship id (an `xsd:ID`) can't: no id a package was read with is taken for one.
pub(crate) const NEW_IMAGE: &str = "twip:image:";

/// `wp:effectExtent`'s attributes for extents of `e` points (left, top, right, bottom).
fn effect_extent(e: [f64; 4]) -> String {
    let [l, t, r, b] = e.map(|v| (v * 12700.0).round() as i64);
    format!(r#"l="{l}" t="{t}" r="{r}" b="{b}""#)
}

impl Saver<'_, '_> {
    /// A float with no source (an inserted text box, or a picture float): its `w:drawing`, from the model. `None` for
    /// what the model can't write (a text box whose content can't be).
    fn new_float_xml(&self, f: &Float, run: &Run) -> Option<String> {
        // A watermark is written as Word writes one, so Word's Watermark dialog finds it.
        if let Some(m) = super::watermark::of(f) {
            let spid = f.shape.as_ref().map_or(0, |s| s.nv.id).max(2049);
            let image = f.image.map_or(String::new(), |i| format!("{NEW_IMAGE}{i}"));
            return Some(super::watermark::vml(&m, &self.w, [f.w, f.h], spid, &image, &run.lang));
        }
        let f = &*unswapped(f);
        const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
        const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
        const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
        const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
        const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let emu = |pt: f64| (pt * 12700.0).round() as i64;
        let id = self.ids.get();
        self.ids.set(id + 1);
        let (cx, cy) = (emu(f.w), emu(f.h));
        const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
        let pos = |name: &str, rel: &str, align: &Option<String>, off: f64| {
            let (k, el) = if name == "positionH" { (0, "pctPosHOffset") } else { (1, "pctPosVOffset") };
            match (align, f.relative.pos[k]) {
                (Some(a), _) => format!("<wp:{name} relativeFrom=\"{rel}\"><wp:align>{}</wp:align></wp:{name}>", props::esc(a)),
                (None, Some(t)) => {
                    format!(r#"<wp:{name} relativeFrom="{rel}"><wp14:{el} xmlns:wp14="{WP14}">{}</wp14:{el}></wp:{name}>"#, t.saturating_mul(100))
                }
                (None, None) => format!("<wp:{name} relativeFrom=\"{rel}\"><wp:posOffset>{}</wp:posOffset></wp:{name}>", emu(off)),
            }
        };
        let sizes: String = f
            .relative
            .size
            .iter()
            .zip([("sizeRelH", "pctWidth"), ("sizeRelV", "pctHeight")])
            .filter_map(|(v, (el, pct))| {
                let (from, t) = v.as_ref()?;
                Some(format!(
                    r#"<wp14:{el} xmlns:wp14="{WP14}" relativeFrom="{}"><wp14:{pct}>{}</wp14:{pct}></wp14:{el}>"#,
                    props::esc(from),
                    t.saturating_mul(100)
                ))
            })
            .collect();
        // A float made with a shape tree writes it (its extent the float's), its story in `wps:txbx`.
        let made = match f.shape.as_deref() {
            Some(shape) => {
                let mut shape = shape.clone();
                shape.xfrm.ext = [crate::drawing::units::Emu(cx), crate::drawing::units::Emu(cy)];
                // Each shape's story: the float's own, or its group part's.
                let txbx = |path: &[u16]| {
                    let tb =
                        if path.is_empty() { f.text_box.as_ref() } else { f.parts.iter().find(|p| p.path == path).and_then(|p| p.text_box.as_ref()) };
                    self.nested_blocks(&tb?.blocks).ok()
                };
                Some(drawing::graphic_data(&shape, &txbx, &self.w))
            }
            None => None,
        };
        let graphic = match (&f.text_box, f.image) {
            _ if made.is_some() => made.clone().unwrap_or_default(),
            (Some(tb), _) => {
                let blocks = self.nested_blocks(&tb.blocks).ok()?;
                let [l, t, r, b] = tb.insets.map(emu);
                let line = if tb.outline > 0.0 {
                    format!(r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#, emu(tb.outline))
                } else {
                    "<a:ln><a:noFill/></a:ln>".to_string()
                };
                let fit = if tb.auto_fit { "<a:spAutoFit/>" } else { "<a:noAutofit/>" };
                // A floating picture control's box has no fill.
                let fill = if f.picture_control(&self.controls).is_some() {
                    "<a:noFill/>"
                } else {
                    r#"<a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill>"#
                };
                format!(
                    concat!(
                        r#"<a:graphicData uri="{wps}"><wps:wsp xmlns:wps="{wps}"><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/>"#,
                        r#"<a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}"#,
                        r#"{line}</wps:spPr><wps:txbx><{w}txbxContent>{blocks}</{w}txbxContent></wps:txbx><wps:bodyPr rot="0" vert="horz" "#,
                        r#"wrap="square" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}" anchor="t" anchorCtr="0">{fit}</wps:bodyPr></wps:wsp></a:graphicData>"#
                    ),
                    wps = WPS,
                    cx = cx,
                    cy = cy,
                    fill = fill,
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
                r#"<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent {ee}/>{wrap}<wp:docPr id="{id}" name="{name}"{title}{descr}/>"#,
                r#"<wp:cNvGraphicFramePr>{locks}</wp:cNvGraphicFramePr><a:graphic xmlns:a="{a}">{graphic}</a:graphic>{sizes}</wp:anchor></{w}drawing>"#
            ),
            w = self.w,
            wp = WP,
            a = A,
            ee = effect_extent(f.effect),
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
            wrap = wrap_xml("wp:", &f.wrap, &f.wrap_text, f.poly.as_deref(), f.poly_edited),
            name = match f.shape.as_deref().map(|s| s.nv.name.as_str()).filter(|n| !n.is_empty()) {
                Some(n) => props::esc(n),
                None => format!("{} {id}", if f.text_box.is_some() { "Text Box" } else { "Picture" }),
            },
            title = f.shape.as_deref().map_or(String::new(), |s| title_attr(&s.nv)),
            descr = descr_attr(&f.alt),
            locks = if f.lock_aspect { format!(r#"<a:graphicFrameLocks xmlns:a="{A}" noChangeAspect="1"/>"#) } else { String::new() },
            graphic = graphic,
            sizes = sizes,
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
            let esc = props::esc;
            let rpr = run.direct.0.as_deref().unwrap_or("");
            let on = if ch == checked { 1 } else { 0 };
            let (cs, us, f) = (hex(*checked), hex(*unchecked), esc(font));
            // From its properties (alias, tag, lock and the rest), with the state it shows now.
            let mut ctl = self.controls[c].clone();
            ctl.open.clear();
            // Its own states (their fonts) when it has them, checked as it shows.
            let own = ctl.props.kind_xml.as_deref().filter(|x| x.contains("checkedState")).map(|x| {
                let mut x = x.to_string();
                crate::docx::xml::set_checkbox_checked(&mut x, on == 1);
                x
            });
            ctl.props.kind_xml = own.or(Some(format!(
                concat!(
                    "<w14:checkbox xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\">",
                    "<w14:checked w14:val=\"{on}\"/><w14:checkedState w14:val=\"{cs}\" w14:font=\"{f}\"/><w14:uncheckedState w14:val=\"{us}\" w14:font=\"{f}\"/>",
                    "</w14:checkbox>"
                ),
                on = on,
                cs = cs,
                us = us,
                f = f,
            )));
            return Some(format!(
                "{PARA_LEVEL}{}<{w}r>{rpr}<{w}t>{}</{w}t></{w}r></{w}sdtContent></{w}sdt>",
                control_open(w, *c, &ctl),
                esc(ch.encode_utf8(&mut [0; 4]))
            ));
        }
        // A copy pasted into the part it came from: its source XML (its relationships are this part's), at its size now,
        // with drawing ids of its own.
        if let Inline::Picture(p, _) = i
            && let Some(x) = p.src.0.as_deref().filter(|x| self.xml.contains(*x))
        {
            // Its text box holding its own text.
            let own = p.text.as_ref().and_then(|t| self.nested_blocks(&t.text_box.blocks).ok());
            let x = &match own {
                Some(t) => drawing::apply(x, box_contents(x).into_iter().map(|b| (b, t.clone())).collect()),
                None => x.to_string(),
            };
            let x = if drawing::is_vml(x) {
                drawing::vml_size(x, p.w, p.h)
            } else {
                Placement { size: Some((p.w, p.h)), alt: Some(p.alt.to_string()), effect: Some(p.effect), ..Placement::default() }.apply(x)
            };
            return Some(self.new_doc_pr_ids(&x));
        }
        if let Inline::Picture(p, _) = i {
            return self.new_picture_xml(p);
        }
        object_xml(&self.w, i)
    }
}

impl Saver<'_, '_> {
    /// A picture or shape written from the model, as a new inline drawing.
    fn new_picture_xml(&self, p: &Picture) -> Option<String> {
        // A shape put in line with text: its tree in a `wp:inline`.
        if let Some(shape) = p.shape.as_deref().filter(|s| p.image.is_none() || s.pic().is_none_or(|q| q.blip.image.is_some())) {
            let id = self.ids.get();
            self.ids.set(id + 1);
            let emu = |pt: f64| (pt * 12700.0).round() as i64;
            let (cx, cy) = (emu(p.w), emu(p.h));
            let mut shape = shape.clone();
            shape.xfrm.ext = [crate::drawing::units::Emu(cx), crate::drawing::units::Emu(cy)];
            const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
            const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
            let name = if shape.nv.name.is_empty() { format!("Shape {id}") } else { props::esc(&shape.nv.name) };
            return Some(format!(
                concat!(
                    r#"<{w}drawing><wp:inline xmlns:wp="{wp}" distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>"#,
                    r#"<wp:effectExtent {ee}/><wp:docPr id="{id}" name="{name}"{title}{descr}/><wp:cNvGraphicFramePr>{locks}"#,
                    r#"</wp:cNvGraphicFramePr><a:graphic xmlns:a="{a}">{graphic}</a:graphic></wp:inline></{w}drawing>"#
                ),
                locks = if p.lock_aspect { format!(r#"<a:graphicFrameLocks xmlns:a="{A}" noChangeAspect="1"/>"#) } else { String::new() },
                ee = effect_extent(p.effect),
                w = self.w,
                wp = WP,
                a = A,
                cx = cx,
                cy = cy,
                id = id,
                name = name,
                descr = descr_attr(&p.alt),
                title = title_attr(&shape.nv),
                graphic = drawing::graphic_data(
                    &shape,
                    &|path: &[u16]| self.nested_blocks(&p.text.as_ref().filter(|_| path.is_empty())?.text_box.blocks).ok(),
                    &self.w
                ),
            ));
        }
        // A picture without its shape tree.
        let image = p.image?;
        let id = self.ids.get();
        self.ids.set(id + 1);
        let emu = |pt: f64| (pt * 12700.0).round() as i64;
        let (cx, cy) = (emu(p.w), emu(p.h));
        const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
        const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
        const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
        const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        Some(format!(
            concat!(
                r#"<{w}drawing><wp:inline xmlns:wp="{wp}" distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>"#,
                r#"<wp:effectExtent {ee}/><wp:docPr id="{id}" name="Picture {id}"{descr}/><wp:cNvGraphicFramePr>"#,
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
            descr = descr_attr(&p.alt),
            ee = effect_extent(p.effect),
        ))
    }
}

/// A new drawing's `wp:docPr` title and hidden mark: none when it has neither.
fn title_attr(nv: &crate::drawing::NonVisual) -> String {
    let title = if nv.title.is_empty() { String::new() } else { format!(r#" title="{}""#, props::esc(&nv.title)) };
    if nv.hidden { format!(r#"{title} hidden="1""#) } else { title }
}

/// A new drawing's `wp:docPr/@descr`, its alternative text: none when it has none.
fn descr_attr(alt: &str) -> String {
    if alt.is_empty() { String::new() } else { format!(r#" descr="{}""#, props::esc(alt)) }
}

/// Each made part that embeds a new picture's image ([`NEW_IMAGE`]): the image as a media part beside it, its
/// relationship, and the content type of its extension.
// shortcut: its relationship and content-type edits are its own; they move to `package::add_part` when next changed.
fn add_images(package: &[u8], images: &[Vec<u8>], parts: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let mut zin = zip::ZipArchive::new(Cursor::new(package)).map_err(|e| e.to_string())?;
    // Media part names the package and the parts written have: a new image's is free in both.
    let mut taken: std::collections::HashSet<String> =
        zin.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).chain(parts.iter().map(|(n, _)| n.clone())).collect();
    let mut read = |parts: &[(String, Vec<u8>)], n: &str| -> Result<Option<String>, String> {
        if let Some((_, b)) = parts.iter().find(|(m, _)| m == n) {
            return String::from_utf8(b.clone()).map(Some).map_err(|e| e.to_string());
        }
        let Ok(f) = zin.by_name(n) else { return Ok(None) };
        crate::docx::package::text(f, n).map(Some)
    };
    let names: Vec<String> = parts
        .iter()
        .filter(|(n, b)| n.ends_with(".xml") && !n.contains("_rels") && b.windows(NEW_IMAGE.len()).any(|w| w == NEW_IMAGE.as_bytes()))
        .map(|(n, _)| n.clone())
        .collect();
    // A new image's media part in a folder, shared by the parts there that show it.
    let mut placed: HashMap<(String, String), String> = HashMap::new();
    for name in names {
        let xml = String::from_utf8(parts.iter().find(|(n, _)| *n == name).map(|p| p.1.clone()).unwrap_or_default()).map_err(|e| e.to_string())?;
        // The images its relationship attributes name (`r:embed`, `r:id`, `o:relid`), not text that looks like one.
        let mut used: Vec<usize> = {
            let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
            dom.descendants()
                .flat_map(|n| n.attributes())
                .filter(|a| a.namespace() == Some(R) || a.namespace() == Some(O) && a.name() == "relid")
                .filter_map(|a| a.value().strip_prefix(NEW_IMAGE)?.parse().ok())
                .collect()
        };
        used.sort_unstable();
        used.dedup();
        let (dir, file) = name.rsplit_once('/').unwrap_or(("", &name));
        let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let mut rels = read(parts, &rels_name)?.unwrap_or_else(|| {
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_string()
        });
        let mut types = read(parts, "[Content_Types].xml")?.ok_or("no content types")?;
        let mut xml = xml;
        let mut media: Vec<(String, Vec<u8>)> = Vec::new();
        for k in used {
            let data = images.get(k).ok_or("a new picture without its image")?;
            media.push((format!("{k}"), data.clone()));
            // An SVG is written as Word writes one: the blip shows a PNG of it, its `asvg:svgBlip` the SVG.
            if super::image::is_svg(data)
                && let Some((png, patched)) = svg_blips(&xml, k, data)
            {
                xml = patched;
                media.push((format!("{k}f"), png));
            }
        }
        // Relationship ids the part has: each new image takes one it doesn't.
        let mut ids: std::collections::HashSet<String> = roxmltree::Document::parse(&rels)
            .map_err(|e| e.to_string())?
            .descendants()
            .filter_map(|n| n.attribute("Id"))
            .map(str::to_string)
            .collect();
        let mut given: HashMap<String, String> = HashMap::new();
        let mut n = 0usize;
        for (id, data) in media {
            let (mime, ext) = super::image::image_type(&data).ok_or("a new picture of an unknown image type")?;
            let rid = loop {
                n += 1;
                let rid = format!("rIdCeImg{n}");
                if !ids.contains(&rid) {
                    break rid;
                }
            };
            ids.insert(rid.clone());
            let target = match placed.get(&(dir.to_string(), id.clone())) {
                Some(t) => t.clone(),
                None => {
                    // An SVG's PNG fallback is named for it.
                    let fallback = if id.ends_with('f') { "f" } else { "" };
                    let target = (1..)
                        .map(|m| format!("media/ceimage{m}{fallback}.{ext}"))
                        .find(|t| !taken.contains(&if dir.is_empty() { t.clone() } else { format!("{dir}/{t}") }))
                        .ok_or("no free media name")?;
                    let full = if dir.is_empty() { target.clone() } else { format!("{dir}/{target}") };
                    taken.insert(full.clone());
                    parts.push((full, data));
                    placed.insert((dir.to_string(), id.clone()), target.clone());
                    target
                }
            };
            let close = rels.rfind("</Relationships>").ok_or("relationships without their end tag")?;
            rels.insert_str(
                close,
                &format!(
                    r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{target}"/>"#
                ),
            );
            given.insert(format!("{NEW_IMAGE}{id}"), rid);
            if !types.to_ascii_lowercase().contains(&format!("extension=\"{ext}\"")) {
                let close = types.rfind("</Types>").ok_or("content types without their end tag")?;
                types.insert_str(close, &format!(r#"<Default Extension="{ext}" ContentType="{mime}"/>"#));
            }
        }
        // Each new image's name replaced by its relationship's id.
        let mut fix: Vec<(Range<usize>, &str)> = {
            let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
            dom.descendants()
                .flat_map(|n| n.attributes())
                .filter(|a| a.namespace() == Some(R) || a.namespace() == Some(O) && a.name() == "relid")
                .filter_map(|a| Some((a.range_value(), given.get(a.value())?.as_str())))
                .collect()
        };
        fix.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
        for (r, rid) in fix {
            xml.replace_range(r, rid);
        }
        for (n, b) in [(name, xml), (rels_name, rels), ("[Content_Types].xml".to_string(), types)] {
            match parts.iter_mut().find(|(m, _)| *m == n) {
                Some(p) => p.1 = b.into_bytes(),
                None => parts.push((n, b.into_bytes())),
            }
        }
    }
    Ok(())
}

/// A part's XML with each new blip of SVG image `k` (`a:blip r:embed="rIdCeImg{k}"`) showing its PNG fallback
/// (`rIdCeImg{k}f`) and naming the SVG in an `asvg:svgBlip` extension, and that PNG: drawn at the first picture's
/// size at 96 dpi. `None` when the SVG can't be drawn.
fn svg_blips(xml: &str, k: usize, svg: &[u8]) -> Option<(Vec<u8>, String)> {
    let attr = format!("r:embed=\"{NEW_IMAGE}{k}\"");
    let first = xml.match_indices(&attr).map(|(at, _)| at).find(|&at| xml[..at].rfind('<').is_some_and(|t| xml[t..].starts_with("<a:blip")))?;
    // The drawing's extent, the last before the blip.
    let extent = xml[..first].rfind("<wp:extent ").map(|e| &xml[e..]).and_then(|t| {
        let num = |name: &str| t.split(&format!("{name}=\"")).nth(1)?.split('"').next()?.parse::<f64>().ok();
        Some((num("cx")?, num("cy")?))
    });
    let (w, h) = extent.map_or((256, 256), |(cx, cy)| (((cx / 9525.0).round() as i32).clamp(1, 2048), ((cy / 9525.0).round() as i32).clamp(1, 2048)));
    let png = crate::render::svg::fallback_png(svg, w, h)?;
    let ext = format!(
        r#"<a:extLst><a:ext uri="{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" {attr}/></a:ext></a:extLst>"#
    );
    let mut out = String::with_capacity(xml.len() + 256);
    let mut rest = xml;
    while let Some(at) = rest.find(&attr) {
        if !rest[..at].rfind('<').is_some_and(|t| rest[t..].starts_with("<a:blip")) {
            out.push_str(&rest[..at + attr.len()]);
            rest = &rest[at + attr.len()..];
            continue;
        }
        out.push_str(&rest[..at]);
        out.push_str(&format!("r:embed=\"{NEW_IMAGE}{k}f\""));
        rest = &rest[at + attr.len()..];
        let end = rest.find('>')?;
        if rest[..end].ends_with('/') {
            out.push_str(&rest[..end - 1]);
            out.push('>');
            out.push_str(&ext);
            out.push_str("</a:blip>");
        } else {
            let close = rest.find("</a:blip>")?;
            out.push_str(&rest[..close]);
            out.push_str(&ext);
            out.push_str("</a:blip>");
            rest = &rest[close + "</a:blip>".len()..];
            continue;
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some((png, out))
}

/// The run content a new object writes as, for those the model can write without a source.
fn object_xml(w: &str, i: &Inline) -> Option<String> {
    match i {
        Inline::NoteRef { kind, id, custom, .. } => {
            let el = if *kind == NoteKind::Foot { "footnoteReference" } else { "endnoteReference" };
            let mark = if *custom { format!(" {w}customMarkFollows=\"1\"") } else { String::new() };
            Some(format!("<{w}{el}{mark} {w}id=\"{id}\"/>"))
        }
        // A character past U+FFFF, which `w:char` can't hold: Word 2016's symbol, with the character as its fallback.
        Inline::Sym { ch, font, control: None, .. } if *ch as u32 > 0xFFFF => Some(format!(
            concat!(
                "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\">",
                "<mc:Choice xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\" Requires=\"w16se\">",
                "<w16se:symEx w16se:font=\"{f}\" w16se:char=\"{c:X}\"/></mc:Choice><mc:Fallback><{w}t>{ch}</{w}t></mc:Fallback>",
                "</mc:AlternateContent>"
            ),
            f = props::esc(font),
            c = *ch as u32,
            ch = ch,
            w = w
        )),
        Inline::Sym { ch, font, control: None, .. } => Some(format!("<{w}sym {w}font=\"{}\" {w}char=\"{:04X}\"/>", props::esc(font), *ch as u32)),
        Inline::NoteMark(NoteKind::Foot, ..) => Some(format!("<{w}footnoteRef/>")),
        Inline::NoteMark(NoteKind::End, ..) => Some(format!("<{w}endnoteRef/>")),
        // each marker is its `fldChar`, in a run of the field's formatting.
        Inline::FieldBegin { lock, dirty, form, .. } => {
            let lock = if *lock { format!(" {w}fldLock=\"1\"") } else { String::new() };
            let dirty = if *dirty { format!(" {w}dirty=\"1\"") } else { String::new() };
            let ff = form.as_ref().map(|f| match f.xml.0.as_deref() {
                Some(x) if f.data == f.read => x.to_string(),
                _ => f.data.xml(w),
            });
            Some(match ff {
                Some(ff) => format!("<{w}fldChar {w}fldCharType=\"begin\"{lock}{dirty}>{ff}</{w}fldChar>"),
                None => format!("<{w}fldChar {w}fldCharType=\"begin\"{lock}{dirty}/>"),
            })
        }
        Inline::Ruby(r, _) => r.xml.0.as_deref().map(str::to_string),
        Inline::DateBlock(name, ..) => Some(format!("<{w}{name}/>")),
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
                Leader::Underscore | Leader::Heavy => "underscore",
                Leader::MiddleDot => "middleDot",
                Leader::None => "none",
            };
            let to = if t.margin { "margin" } else { "indent" };
            Some(format!("<{w}ptab {w}relativeTo=\"{to}\" {w}alignment=\"{align}\" {w}leader=\"{leader}\"/>"))
        }
        _ => None,
    }
}

/// A float's new position, size and options, each only when changed: patched into its `w:drawing` in place.
#[derive(Clone, Debug, Default)]
struct Placement {
    /// A position's reference, alignment (or else offset), when any of them changed.
    h: Option<(String, Option<String>, f64)>,
    v: Option<(String, Option<String>, f64)>,
    size: Option<(f64, f64)>,
    /// Wrapping (`wrap`, `wrap_text`, behind the text, the wrap polygon and whether it was edited), when changed.
    wrap: Option<(String, String, bool, Option<Rc<[(f64, f64)]>>, bool)>,
    locked: Option<bool>,
    z: Option<u32>,
    /// The alternative text (`wp:docPr/@descr`).
    alt: Option<String>,
    /// The decorative mark (`adec:decorative` in `wp:docPr`'s extension list).
    decorative: Option<bool>,
    /// The name, title and hidden mark (`wp:docPr`'s `name`, `title` and `hidden`).
    nv: Option<(String, String, bool)>,
    /// Lock aspect ratio (`a:graphicFrameLocks/@noChangeAspect`).
    lock: Option<bool>,
    /// A new picture (an image of [`Document::images`]): the blip names it ([`NEW_IMAGE`]).
    blip: Option<usize>,
    /// The distance from the text (`distT`, `distB`, `distL`, `distR`), `allowOverlap` and `layoutInCell`.
    dist: Option<[f64; 4]>,
    /// An inline picture's effect extents (`wp:effectExtent`: left, top, right, bottom).
    effect: Option<[f64; 4]>,
    overlap: Option<bool>,
    in_cell: Option<bool>,
    /// The shape's own properties, as edits in the drawing's text: applied first.
    shape: Option<Vec<drawing::Edit>>,
    /// The drawing's place in the patched text, when only it is written (an `mc:AlternateContent` around it is
    /// dropped).
    inner: Option<Range<usize>>,
    /// The whole text written instead (a VML object patched as VML).
    replace: Option<String>,
    /// Relative size and position (`wp14:sizeRelH`, `wp14:pctPosHOffset`, ...), with the offsets a position left
    /// relative goes back to.
    relative: Option<(crate::docx::Relative, [f64; 2])>,
}

/// A float as its XML gives it: a quarter-turned float's box and offsets turned back.
fn unswapped(f: &Float) -> std::borrow::Cow<'_, Float> {
    if !f.rot_swap {
        return std::borrow::Cow::Borrowed(f);
    }
    let mut f = f.clone();
    let (w, h) = (f.w, f.h);
    if f.h_align.is_none() {
        f.h_offset -= (h - w) / 2.0;
    }
    if f.v_align.is_none() {
        f.v_offset -= (w - h) / 2.0;
    }
    (f.w, f.h) = (h, w);
    std::borrow::Cow::Owned(f)
}

impl Placement {
    fn changed(a: &Float, b: &Float) -> Option<Placement> {
        let ne = |x: f64, y: f64| (x - y).abs() > 1e-6;
        let (a, b) = (&*unswapped(a), &*unswapped(b));
        // An aligned position that became an offset is written as one.
        let pos = |(ra, aa, oa): (&String, &Option<String>, f64), (rb, ab, ob): (&String, &Option<String>, f64)| {
            (ra != rb || aa != ab || ab.is_none() && ne(oa, ob)).then(|| (rb.clone(), ab.clone(), ob))
        };
        let p = Placement {
            h: pos((&a.h_relative, &a.h_align, a.h_offset), (&b.h_relative, &b.h_align, b.h_offset)),
            v: pos((&a.v_relative, &a.v_align, a.v_offset), (&b.v_relative, &b.v_align, b.v_offset)),
            size: (ne(a.w, b.w) || ne(a.h, b.h)).then_some((b.w, b.h)),
            wrap: (a.wrap != b.wrap || a.wrap_text != b.wrap_text || a.behind != b.behind || a.poly != b.poly || a.poly_edited != b.poly_edited)
                .then(|| (b.wrap.clone(), b.wrap_text.clone(), b.behind, b.poly.clone(), b.poly_edited)),
            locked: (a.locked != b.locked).then_some(b.locked),
            z: (a.z != b.z).then_some(b.z),
            alt: (a.alt != b.alt).then(|| b.alt.to_string()),
            decorative: (decorative(&a.shape) != decorative(&b.shape)).then(|| decorative(&b.shape)),
            nv: nv_changed(a.shape.as_deref(), b.shape.as_deref()),
            lock: (a.lock_aspect != b.lock_aspect).then_some(b.lock_aspect),
            blip: b.image.filter(|_| a.image != b.image),
            dist: a.dist.iter().zip(&b.dist).any(|(x, y)| ne(*x, *y)).then_some(b.dist),
            effect: None,
            overlap: (a.allow_overlap != b.allow_overlap).then_some(b.allow_overlap),
            in_cell: (a.layout_in_cell != b.layout_in_cell).then_some(b.layout_in_cell),
            shape: None,
            inner: None,
            replace: None,
            relative: (a.relative.size != b.relative.size || a.relative.pos != b.relative.pos)
                .then(|| (b.relative.clone(), [b.h_offset, b.v_offset])),
        };
        let any = p.h.is_some()
            || p.relative.is_some()
            || p.v.is_some()
            || p.size.is_some()
            || p.wrap.is_some()
            || p.locked.is_some()
            || p.alt.is_some()
            || p.decorative.is_some()
            || p.nv.is_some()
            || p.lock.is_some()
            || p.blip.is_some()
            || p.z.is_some();
        (any || p.dist.is_some() || p.overlap.is_some() || p.in_cell.is_some()).then_some(p)
    }

    fn apply(&self, d: &str) -> String {
        let emu = |pt: f64| ((pt * 12700.0).round() as i64).to_string();
        if let Some(r) = &self.replace {
            return r.clone();
        }
        let d = self.inner.clone().and_then(|r| d.get(r)).unwrap_or(d);
        let mut s = match &self.shape {
            Some(e) => drawing::apply(d, e.clone()),
            None => d.to_string(),
        };
        for (name, pos) in [("positionH", &self.h), ("positionV", &self.v)] {
            let Some((rel, align, off)) = pos else { continue };
            if let Some((start, end)) = find_tag(&s, name, 0) {
                let tag = set_attr(&s[start..end], "relativeFrom", &props::esc(rel));
                s.replace_range(start..end, &tag);
            }
            set_content(&mut s, name, |p| match align {
                Some(a) => format!("<{p}align>{}</{p}align>", props::esc(a)),
                None => format!("<{p}posOffset>{}</{p}posOffset>", emu(*off)),
            });
        }
        if let Some((r, offsets)) = &self.relative {
            const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
            for (k, (name, el)) in [("positionH", "pctPosHOffset"), ("positionV", "pctPosVOffset")].into_iter().enumerate() {
                match r.pos[k] {
                    Some(t) => set_content(&mut s, name, |_| format!(r#"<wp14:{el} xmlns:wp14="{WP14}">{}</wp14:{el}>"#, t.saturating_mul(100))),
                    None if s.contains(el) => set_content(&mut s, name, |p| format!("<{p}posOffset>{}</{p}posOffset>", emu(offsets[k]))),
                    None => {}
                }
            }
            for el in ["sizeRelH", "sizeRelV"] {
                while let Some(at) = element(&s, el) {
                    s.replace_range(at, "");
                }
            }
            let sizes: String = r
                .size
                .iter()
                .zip([("sizeRelH", "pctWidth"), ("sizeRelV", "pctHeight")])
                .filter_map(|(v, (el, pct))| {
                    let (from, t) = v.as_ref()?;
                    Some(format!(
                        r#"<wp14:{el} xmlns:wp14="{WP14}" relativeFrom="{}"><wp14:{pct}>{}</wp14:{pct}></wp14:{el}>"#,
                        props::esc(from),
                        t.saturating_mul(100)
                    ))
                })
                .collect();
            if let Some(end) = s.rfind(":anchor>").and_then(|k| s[..k].rfind("</")) {
                s.insert_str(end, &sizes);
            }
        }
        if let Some((start, end)) = find_tag(&s, "anchor", 0) {
            let mut tag = s[start..end].to_string();
            if let Some((_, _, behind, ..)) = &self.wrap {
                tag = set_attr(&tag, "behindDoc", if *behind { "1" } else { "0" });
            }
            if let Some(z) = self.z {
                tag = set_attr(&tag, "relativeHeight", &z.to_string());
            }
            if let Some(l) = self.locked {
                tag = set_attr(&tag, "locked", if l { "1" } else { "0" });
            }
            let on = |b: bool| if b { "1" } else { "0" };
            if let Some(o) = self.overlap {
                tag = set_attr(&tag, "allowOverlap", on(o));
            }
            if let Some(c) = self.in_cell {
                tag = set_attr(&tag, "layoutInCell", on(c));
            }
            if let Some(dist) = self.dist {
                for (name, v) in ["distT", "distB", "distL", "distR"].into_iter().zip(dist) {
                    tag = set_attr(&tag, name, &emu(v));
                }
            }
            let p = tag[1..]
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .rsplit_once(':')
                .map_or(String::new(), |(p, _)| format!("{p}:"));
            s.replace_range(start..end, &tag);
            if let Some((wrap, text, _, poly, edited)) = &self.wrap {
                let x = wrap_xml(&p, wrap, text, poly.as_deref(), *edited);
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
        if let Some(e) = self.effect {
            let ee = effect_extent(e);
            match find_tag(&s, "effectExtent", 0) {
                Some((start, end)) => {
                    let p = &s[start + 1..end].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("").to_string();
                    s.replace_range(start..end, &format!("<{p} {ee}/>"));
                }
                // None: after the extent (the schema's order: extent, effectExtent, docPr, ...).
                None if e != [0.0; 4] => {
                    if let Some((at, p)) = find_tag(&s, "extent", 0).map(|(start, end)| {
                        let name = s[start + 1..end].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
                        (end, name.rsplit_once(':').map_or(String::new(), |(p, _)| format!("{p}:")))
                    }) {
                        s.insert_str(at, &format!("<{p}effectExtent {ee}/>"));
                    }
                }
                None => {}
            }
        }
        if let Some(alt) = &self.alt
            && let Some((start, end)) = find_tag(&s, "docPr", 0)
        {
            let tag = set_attr(&s[start..end], "descr", &props::esc(alt));
            s.replace_range(start..end, &tag);
        }
        if let Some((name, title, hidden)) = &self.nv
            && let Some((start, end)) = find_tag(&s, "docPr", 0)
        {
            let mut tag = set_attr(&s[start..end], "name", &props::esc(name));
            if !title.is_empty() || tag.contains(" title=\"") {
                tag = set_attr(&tag, "title", &props::esc(title));
            }
            if *hidden || tag.contains(" hidden=\"") {
                tag = set_attr(&tag, "hidden", if *hidden { "1" } else { "0" });
            }
            s.replace_range(start..end, &tag);
        }
        if let Some(on) = self.decorative {
            s = set_decorative(&s, on);
        }
        if let Some(on) = self.lock {
            s = set_lock_aspect(&s, on);
        }
        // The first blip (the picture's) replaced by one naming the new image; its effects go with the old picture.
        if let Some(k) = self.blip
            && let Some((start, end)) = find_tag(&s, "blip", 0)
        {
            let prefix = s[start + 1..end].split("blip").next().unwrap_or("").to_string();
            let close = format!("</{prefix}blip>");
            let stop = if s[start..end].ends_with("/>") { Some(end) } else { s[end..].find(&close).map(|c| end + c + close.len()) };
            if let Some(stop) = stop {
                // Its own relationship namespace: the old blip may have declared it.
                s.replace_range(start..stop, &format!(r#"<{prefix}blip xmlns:r="{R}" r:embed="{NEW_IMAGE}{k}"/>"#));
            }
        }
        if let Some((w, h)) = self.size {
            // The anchor's extent and the top shape's own size: the first `a:ext` with cx (extension lists' have none;
            // a group's children keep theirs, scaled by its child frame).
            for name in ["extent", "ext"] {
                let mut from = 0;
                while let Some((start, end)) = find_tag(&s, name, from) {
                    let tag = s[start..end].to_string();
                    if tag.contains(" cx=") {
                        let t = set_attr(&set_attr(&tag, "cx", &emu(w)), "cy", &emu(h));
                        s.replace_range(start..end, &t);
                        break;
                    }
                    from = end;
                }
            }
        }
        s
    }
}

/// Whether two runs carry the same insertion and deletion.
fn same_rev(a: &RunStyle, b: &RunStyle) -> bool {
    a.rev.ins == b.rev.ins && a.rev.del == b.rev.del
}

/// The first start tag named `local` (any prefix) at or after `from`: its byte range.
/// The decorative mark's extension (`a:ext/@uri`).
const DECORATIVE: &str = "{C183D7F6-B498-43B3-948B-1728B52AA6E4}";

/// A drawing's XML with its lock aspect ratio on or off: `noChangeAspect` on its frame's locks (`a:graphicFrameLocks`
/// in `wp:cNvGraphicFramePr`, made when it has none).
fn set_lock_aspect(s: &str, on: bool) -> String {
    const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
    let mut s = s.to_string();
    let v = if on { "1" } else { "0" };
    // A picture's own locks say it too: off there as well.
    if !on {
        let mut from = 0;
        while let Some((start, end)) = find_tag(&s, "picLocks", from) {
            let tag = set_attr(&s[start..end], "noChangeAspect", "0");
            s.replace_range(start..end, &tag);
            from = start + tag.len();
        }
    }
    if let Some((start, end)) = find_tag(&s, "graphicFrameLocks", 0) {
        let tag = set_attr(&s[start..end], "noChangeAspect", v);
        s.replace_range(start..end, &tag);
        return s;
    }
    let locks = format!(r#"<a:graphicFrameLocks xmlns:a="{A}" noChangeAspect="{v}"/>"#);
    match find_tag(&s, "cNvGraphicFramePr", 0) {
        Some((start, end)) if s[start..end].ends_with("/>") => {
            let open = s[start..end - 2].trim_end().to_string();
            let name = open[1..].split_whitespace().next().unwrap_or("wp:cNvGraphicFramePr").to_string();
            s.replace_range(start..end, &format!("{open}>{locks}</{name}>"));
        }
        Some((_, end)) => s.insert_str(end, &locks),
        // None: after the `wp:docPr` (the schema's order: docPr, cNvGraphicFramePr, graphic).
        None => {
            if let Some(at) = element(&s, "docPr").map(|r| r.end) {
                let prefix = find_tag(&s, "docPr", 0).and_then(|(a, b)| {
                    s[a + 1..b].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next()?.rsplit_once(':').map(|(p, _)| p.to_string())
                });
                let p = prefix.map_or(String::new(), |p| format!("{p}:"));
                s.insert_str(at, &format!("<{p}cNvGraphicFramePr>{locks}</{p}cNvGraphicFramePr>"));
            }
        }
    }
    s
}

/// A drawing's new name, title and hidden mark, when they changed.
fn nv_changed(a: Option<&crate::drawing::Shape>, b: Option<&crate::drawing::Shape>) -> Option<(String, String, bool)> {
    let (a, b) = (&a?.nv, &b?.nv);
    ((&a.name, &a.title, a.hidden) != (&b.name, &b.title, b.hidden)).then(|| (b.name.clone(), b.title.to_string(), b.hidden))
}

fn decorative(shape: &Option<Box<crate::drawing::Shape>>) -> bool {
    shape.as_ref().is_some_and(|s| s.nv.decorative)
}

/// A drawing's XML with the decorative mark in its `wp:docPr` (`on`) or not: added as an extension (in its extension
/// list, or a new one), or that extension removed.
fn set_decorative(s: &str, on: bool) -> String {
    let Some((start, end)) = find_tag(s, "docPr", 0) else { return s.to_string() };
    let mut s = s.to_string();
    let close = s[start..].find("docPr>").map(|i| start + i + "docPr>".len()).filter(|_| !s[start..end].ends_with("/>"));
    let within = close.unwrap_or(end);
    let have = s[start..within].find(DECORATIVE).map(|i| start + i);
    match (on, have) {
        (true, Some(_)) | (false, None) => {}
        (false, Some(at)) => {
            // The whole `<a:ext uri=...>...</a:ext>`.
            if let (Some(open), Some(shut)) = (s[..at].rfind('<'), s[at..].find("ext>").map(|i| at + i + "ext>".len())) {
                s.replace_range(open..shut, "");
            }
        }
        (true, None) => {
            const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
            const ADEC: &str = "http://schemas.microsoft.com/office/drawing/2017/decorative";
            let ext = format!(r#"<a:ext uri="{DECORATIVE}"><adec:decorative xmlns:adec="{ADEC}" val="1"/></a:ext>"#);
            let prefix =
                s[start + 1..].split(['>', ' ', '/']).next().and_then(|t| t.rsplit_once(':')).map_or(String::new(), |(p, _)| format!("{p}:"));
            match close {
                // An extension list already: the mark joins it.
                Some(c) if s[start..c].contains("extLst>") => {
                    // Before the list's closing tag: the last `extLst>` in the `wp:docPr` is it.
                    if let Some(at) = s[start..c].rfind("extLst>").map(|i| start + i)
                        && let Some(shut) = s[..at].rfind("</")
                    {
                        s.insert_str(shut, &ext);
                    }
                }
                Some(c) => {
                    let shut = s[..c].rfind("</").unwrap_or(c);
                    s.insert_str(shut, &format!(r#"<a:extLst xmlns:a="{A}">{ext}</a:extLst>"#));
                }
                None => {
                    let open = s[start..end - 2].trim_end().to_string();
                    s.replace_range(start..end, &format!(r#"{open}><a:extLst xmlns:a="{A}">{ext}</a:extLst></{prefix}docPr>"#));
                }
            }
        }
    }
    s
}

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

/// A drawing's wrap element (`p`: the drawing prefix with its colon): a tight or through wrap's polygon is `poly` (box
/// fractions), or the object's box; `edited` marks one the user changed.
fn wrap_xml(p: &str, wrap: &str, text: &str, poly: Option<&[(f64, f64)]>, edited: bool) -> String {
    match wrap {
        "square" => format!("<{p}wrapSquare wrapText=\"{text}\"/>"),
        "tight" | "through" => {
            use std::fmt::Write as _;
            // 21600 is the whole of each side.
            const BOX: [(f64, f64); 5] = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)];
            let n = if wrap == "tight" { "wrapTight" } else { "wrapThrough" };
            let (points, edited) = match poly.filter(|q| q.len() >= 2) {
                Some(q) => (q, edited),
                None => (&BOX[..], false),
            };
            let mut s = format!("<{p}{n} wrapText=\"{text}\"><{p}wrapPolygon edited=\"{}\">", if edited { 1 } else { 0 });
            for (k, &(x, y)) in points.iter().enumerate() {
                let el = if k == 0 { "start" } else { "lineTo" };
                let c = |v: f64| (v * 21600.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i64;
                let _ = write!(s, "<{p}{el} x=\"{}\" y=\"{}\"/>", c(x), c(y));
            }
            let _ = write!(s, "</{p}wrapPolygon></{p}{n}>");
            s
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

/// A table position's `w:tblpPr` and its `w:tblOverlap` (empty when the table may overlap others); both empty without
/// a position. Against the text an alignment is written as the offset the reader takes it for.
fn tblp_pr(w: &str, p: Option<&super::props::TablePos>) -> (String, String) {
    use super::props::TableAnchor as A;
    let Some(p) = p else { return (String::new(), String::new()) };
    let [top, bottom, left, right] = p.dist.map(twips);
    let vert = match p.v_anchor {
        A::Text => "text",
        A::Margin => "margin",
        A::Page => "page",
    };
    let horz = match (p.h_page, p.h_text) {
        (true, _) => "page",
        (false, true) => "text",
        (false, false) => "margin",
    };
    let spec = |name: &str, j: Option<Jc>, ends: [&str; 2]| match j {
        Some(Jc::Center) => format!(" {w}{name}=\"center\""),
        Some(Jc::Right) => format!(" {w}{name}=\"{}\"", ends[1]),
        Some(_) => format!(" {w}{name}=\"{}\"", ends[0]),
        None => String::new(),
    };
    let x = match p.x_spec {
        Some(_) => spec("tblpXSpec", p.x_spec, ["left", "right"]),
        None => format!(" {w}tblpX=\"{}\"", twips(p.x)),
    };
    let y = match (p.v_anchor, p.y_spec) {
        (A::Text, _) | (_, None) => format!(" {w}tblpY=\"{}\"", twips(p.y)),
        (_, Some(_)) => spec("tblpYSpec", p.y_spec, ["top", "bottom"]),
    };
    let pos = format!(
        "<{w}tblpPr {w}leftFromText=\"{left}\" {w}rightFromText=\"{right}\" {w}topFromText=\"{top}\" {w}bottomFromText=\"{bottom}\" {w}vertAnchor=\"{vert}\" {w}horzAnchor=\"{horz}\"{x}{y}/>"
    );
    let overlap = if p.never_overlap { format!("<{w}tblOverlap {w}val=\"never\"/>") } else { String::new() };
    (pos, overlap)
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
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\" {w}color=\"{color}\"{}/>",
        b.style,
        (b.width * 8.0).round() as i64,
        b.space.round() as i64,
        if b.shadow { format!(" {w}shadow=\"1\"") } else { String::new() }
    )
}

fn shading(w: &str, fill: Option<u32>) -> String {
    fill.map_or(String::new(), |f| format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{f:06X}\"/>"))
}

/// A table's `w:tblPr` from the model (its resolved properties written as direct ones, its tracked change last), in
/// CT_TblPr order.
fn tbl_pr(w: &str, t: &Table, shd: Option<&str>, bands: &[Option<String>; 2], cell_mar: Option<&str>, people: &People) -> String {
    let mut s = format!("<{w}tblPr>");
    if let Some(id) = &t.style_id {
        s.push_str(&format!("<{w}tblStyle {w}val=\"{}\"/>", props::esc(id)));
    }
    let (pos, overlap) = tblp_pr(w, t.floating.as_ref());
    s.push_str(&pos);
    s.push_str(&overlap);
    if t.bidi_visual {
        s.push_str(&format!("<{w}bidiVisual/>"));
    }
    for b in bands.iter().flatten() {
        s.push_str(b);
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
    if let Some(p) = t.ind_pct {
        s.push_str(&width(w, "tblInd", &Width::Pct(p)));
    } else if t.ind != 0.0 {
        s.push_str(&width(w, "tblInd", &Width::Dxa(t.ind)));
    }
    if t.borders.iter().any(Option::is_some) {
        s.push_str(&format!("<{w}tblBorders>"));
        for (b, name) in t.borders.iter().zip(["top", "left", "bottom", "right", "insideH", "insideV"]) {
            s.push_str(&border(w, name, b));
        }
        s.push_str(&format!("</{w}tblBorders>"));
    }
    if let Some(shd) = shd {
        s.push_str(shd);
    }
    if t.fixed {
        s.push_str(&format!("<{w}tblLayout {w}type=\"fixed\"/>"));
    }
    if let Some(x) = cell_mar {
        s.push_str(x);
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
    s.push_str(&alt_xml(w, t.alt.as_deref()));
    if let Some(c) = &t.change {
        s.push_str(&revisions::table_change(w, "tblPrChange", "tblPr", c, people));
    }
    s.push_str(&format!("</{w}tblPr>"));
    s
}

/// A table's `w:tblCaption` and `w:tblDescription`.
fn alt_xml(w: &str, alt: Option<&crate::docx::TableAlt>) -> String {
    let Some(a) = alt else { return String::new() };
    let tag = |name: &str, v: &str| if v.is_empty() { String::new() } else { format!("<{w}{name} {w}val=\"{}\"/>", props::esc(v)) };
    tag("tblCaption", &a.title) + &tag("tblDescription", &a.description)
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
    if r.hidden {
        s.push_str(&format!("<{w}hidden/>"));
    }
    s.push_str(&row_jc(w, r.jc));
    for (_, tag) in revisions::row_marks(w, &r.rev, people) {
        s.push_str(&tag);
    }
    if s.is_empty() { s } else { format!("<{w}trPr>{s}</{w}trPr>") }
}

/// A row's `w:jc` (empty for none).
fn row_jc(w: &str, jc: Option<Jc>) -> String {
    let Some(jc) = jc else { return String::new() };
    let v = match jc {
        Jc::Center => "center",
        Jc::Right => "right",
        _ => "left",
    };
    format!("<{w}jc {w}val=\"{v}\"/>")
}

/// A row's grid gaps before and after its cells, as its `w:trPr` has them.
fn grid_gaps(tr_pr: Option<Node>) -> [usize; 2] {
    ["gridBefore", "gridAfter"].map(|name| tr_pr.and_then(|p| child(p, name)).and_then(|c| attr(c, "val")).and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// The `w:trPr` children the model owns that changed since reading, as (name, XML; empty to remove it): against the
/// row as read (`was`) and its source's grid gaps (a row of tables read as one has them over their merged grid);
/// every one for a row with neither.
fn tr_changes(w: &str, gaps: Option<[usize; 2]>, was: Option<&Row>, now: &Row) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (k, name) in ["gridBefore", "gridAfter"].into_iter().enumerate() {
        let b = [now.grid_before, now.grid_after][k];
        if gaps.is_none_or(|g| g[k] != b) {
            out.push((name, if b > 0 { format!("<{w}{name} {w}val=\"{b}\"/>") } else { String::new() }));
        }
    }
    // Turned off: written off (over a table style's), else left out.
    let flag = |name: &str, on: bool| match (on, was) {
        (true, _) => format!("<{w}{name}/>"),
        (false, Some(_)) => format!("<{w}{name} {w}val=\"0\"/>"),
        (false, None) => String::new(),
    };
    if was.is_none_or(|x| x.cant_split != now.cant_split) {
        out.push(("cantSplit", flag("cantSplit", now.cant_split)));
    }
    if was.is_none_or(|x| x.header != now.header) {
        out.push(("tblHeader", flag("tblHeader", now.header)));
    }
    if was.is_none_or(|x| x.hidden != now.hidden) {
        out.push(("hidden", if now.hidden { format!("<{w}hidden/>") } else { String::new() }));
    }
    if was.is_none_or(|x| x.height != now.height) {
        let tag = match &now.height {
            Some((h, rule)) => {
                let rule = if rule == "atLeast" { String::new() } else { format!(" {w}hRule=\"{rule}\"") };
                format!("<{w}trHeight {w}val=\"{}\"{rule}/>", twips(*h))
            }
            None => String::new(),
        };
        out.push(("trHeight", tag));
    }
    if was.is_none_or(|x| x.jc != now.jc) {
        out.push(("jc", row_jc(w, now.jc)));
    }
    out
}

/// A cell's `w:tcPr` from the model, in CT_TcPr order.
/// A cell's `w:tcPr`: the properties it was read with (which table formatting keeps current), its width, span,
/// vertical merge and revisions as the model has them now; else written from the model.
fn tc_pr(w: &str, c: &Cell, people: &People, cx: &Cx) -> String {
    if let Some(raw) = c.direct.0.as_deref() {
        let mut p = Props::parse(Some(raw), "tcPr", cx);
        p.reorder(props::TCPR);
        match &c.width {
            Some(x) => p.set("tcW", width(w, "tcW", x)),
            None => p.remove("tcW"),
        }
        if c.span > 1 {
            p.set("gridSpan", format!("<{w}gridSpan {w}val=\"{}\"/>", c.span));
        } else {
            p.remove("gridSpan");
        }
        // A Word 97 horizontal merge is read as a span.
        p.remove("hMerge");
        // The header cells as read, unless the model's differ from what the reader took of them.
        let read = p.get("headers").and_then(|x| cx.parse(x, |_, el| crate::docx::header_list(el))).unwrap_or_default();
        if read != c.ids.as_ref().map_or(&[][..], |i| i.headers.as_slice()) {
            match header_refs(w, c) {
                Some(x) => p.set("headers", x),
                None => p.remove("headers"),
            }
        }
        match c.v_merge {
            VMerge::Restart => p.set("vMerge", format!("<{w}vMerge {w}val=\"restart\"/>")),
            VMerge::Continue => p.set("vMerge", format!("<{w}vMerge/>")),
            VMerge::None => p.remove("vMerge"),
        }
        for (name, tag) in revisions::cell_marks(w, &c.rev, people) {
            p.remove(name);
            if !tag.is_empty() {
                p.set(name, tag);
            }
        }
        return p.xml().unwrap_or_default();
    }
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
    let [tl2br, tr2bl] = c.diagonals.as_deref().cloned().unwrap_or_default();
    if c.top.is_some() || c.left.is_some() || c.bottom.is_some() || c.right.is_some() || tl2br.is_some() || tr2bl.is_some() {
        s.push_str(&format!("<{w}tcBorders>"));
        s.push_str(&border(w, "top", &c.top));
        s.push_str(&border(w, "left", &c.left));
        s.push_str(&border(w, "bottom", &c.bottom));
        s.push_str(&border(w, "right", &c.right));
        s.push_str(&border(w, "tl2br", &tl2br));
        s.push_str(&border(w, "tr2bl", &tr2bl));
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
    if c.fit_text {
        s.push_str(&format!("<{w}tcFitText/>"));
    }
    if c.v_align != "top" {
        s.push_str(&format!("<{w}vAlign {w}val=\"{}\"/>", c.v_align));
    }
    if c.hide_mark {
        s.push_str(&format!("<{w}hideMark/>"));
    }
    s.push_str(&header_refs(w, c).unwrap_or_default());
    for (_, tag) in revisions::cell_marks(w, &c.rev, people) {
        s.push_str(&tag);
    }
    s.push_str(&format!("</{w}tcPr>"));
    s
}

/// A cell's `w:headers`: the header cells it lists, when it lists any.
fn header_refs(w: &str, c: &Cell) -> Option<String> {
    let ids = c.ids.as_ref().filter(|i| !i.headers.is_empty())?;
    let list: String = ids.headers.iter().map(|h| format!("<{w}header {w}val=\"{}\"/>", props::esc(h))).collect();
    Some(format!("<{w}headers>{list}</{w}headers>"))
}

/// A start tag without its attribute `*:name` (either quote), for markup not parsed (a made comment paragraph's).
fn strip_attr(tag: &str, name: &str) -> String {
    let pat = format!(":{name}=");
    let Some(at) = tag.find(&pat) else { return tag.to_string() };
    let start = tag[..at].rfind(char::is_whitespace).unwrap_or(at);
    let value = at + pat.len();
    let end = match tag[value..].chars().next() {
        Some(q @ ('"' | '\'')) => tag[value + 1..].find(q).map_or(tag.len(), |e| value + 1 + e + 1),
        _ => return tag.to_string(),
    };
    format!("{}{}", &tag[..start], &tag[end..])
}

/// A control's start markup and its content's start tag: as read while its properties are, else written
/// from them (`w:sdtPr` children in schema order, the settings the editor doesn't change as read).
pub(crate) fn control_open(w: &str, id: ControlId, c: &Control) -> String {
    if c.as_read() {
        return c.open.clone();
    }
    let p = &c.props;
    let esc = props::esc;
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
        assert_eq!(strip_attr(r#"<w:p w14:paraId='1"A' w:rsidR="00"><"#, "paraId"), r#"<w:p w:rsidR="00"><"#);
    }

    #[test]
    fn ids_are_read_from_tags_not_text() {
        let xml = r#"<w:t>a:docPr id="5" x</w:t><wp:docPr descr='b id="6"' id="7" name="n"/><w:bookmarkStart w:name="m" w:id="8"/><docPr id='9'>"#;
        let ids: Vec<i64> = tag_ids(xml, DRAWING_TAGS)
            .into_iter()
            .map(|(r, id)| {
                assert_eq!(xml[r].parse::<i64>().ok(), Some(id));
                id
            })
            .collect();
        assert_eq!(ids, [7, 9]);
        assert_eq!(tag_ids(xml, BOOKMARK_TAGS).into_iter().map(|(_, id)| id).collect::<Vec<_>>(), [8]);
    }

    #[test]
    fn ids_are_read_by_namespace_and_as_xml_reads_them() {
        let wp = super::super::xml::WP;
        let xml = format!(
            r#"<r xmlns:wp="{wp}" xmlns:w="{W}"><!-- <wp:docPr id="1"/> --><x:docPr xmlns:x="urn:x" id="2"/><wp:docPr id="&#51;" name="n"/><q:docPr xmlns:q="{wp}" id=" 4 "/><w:bookmarkStart x:id="5" xmlns:x="urn:x" w:id="6"/><![CDATA[<wp:docPr id="7"/>]]><s xmlns:wp="urn:x"><wp:docPr id="8"/></s><wp:docPr id="9"/></r>"#
        );
        let ids: Vec<(String, i64)> = tag_ids(&xml, DRAWING_TAGS).into_iter().map(|(r, id)| (xml[r].to_string(), id)).collect();
        assert_eq!(ids, [("&#51;".to_string(), 3), (" 4 ".to_string(), 4), ("9".to_string(), 9)]);
        assert_eq!(tag_ids(&xml, BOOKMARK_TAGS).into_iter().map(|(_, id)| id).collect::<Vec<_>>(), [6]);
    }

    fn render_fixture(name: &str) -> (Vec<u8>, Document) {
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render").join(name)).expect("fixture");
        let doc = crate::docx::read_editable(&bytes).expect("reads");
        (bytes, doc)
    }

    fn part_text(package: &[u8], name: &str) -> String {
        let mut z = zip::ZipArchive::new(Cursor::new(package)).expect("a package");
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name(name).expect("the part"), &mut s).expect("text");
        s
    }

    #[test]
    fn an_untouched_page_background_keeps_its_markup_and_a_changed_one_is_written() {
        let (bytes, mut doc) = render_fixture("page-pattern.docx");
        assert_eq!(save(&doc).expect("saves"), bytes, "untouched");
        // A new colour: the fill as read is kept, the theme attributes go with the old colour.
        let was = part_text(&bytes, "word/document.xml");
        let fill = &was[was.find("<v:background").unwrap()..was.find("</v:background>").unwrap() + 15];
        doc.page_colour = Some(0x12_3456);
        doc.settings.display_background_shape = false;
        let saved = save(&doc).expect("saves");
        let xml = part_text(&saved, "word/document.xml");
        assert!(xml.contains(&format!(r#"<w:background w:color="123456">{fill}</w:background>"#)), "{xml}");
        assert!(!part_text(&saved, "word/settings.xml").contains("displayBackgroundShape"));
        let again = crate::docx::read(&saved).expect("reads again");
        assert_eq!((again.page_colour, again.page_fill == doc.page_fill, again.settings.display_background_shape), (Some(0x12_3456), true, false));
        // No colour and no fill: no background.
        doc.page_colour = None;
        doc.page_fill = None;
        let xml = part_text(&save(&doc).expect("saves"), "word/document.xml");
        assert!(!xml.contains("<w:background"), "{xml}");
    }

    #[test]
    fn a_fill_that_takes_the_page_colour_is_written_from_the_model_when_the_colour_changes() {
        // The gradient names no first colour: it starts from the page's white.
        let (_, mut doc) = render_fixture("page-gradient.docx");
        let fill = doc.page_fill.clone();
        doc.page_colour = Some(0x12_3456);
        let saved = save(&doc).expect("saves");
        let xml = part_text(&saved, "word/document.xml");
        let el = &xml[xml.find("<w:background").unwrap()..xml.find("</w:background>").unwrap()];
        assert!(!el.contains("xmlns") && !el.contains("opacity"), "as Word writes it: {el}");
        let again = crate::docx::read(&saved).expect("reads again");
        assert_eq!((again.page_colour, &again.page_fill), (Some(0x12_3456), &fill), "the fill as the model held it");
    }

    #[test]
    fn a_fill_on_a_part_without_vml_declares_its_namespaces_on_the_root() {
        let (bytes, _) = render_fixture("page-colour.docx");
        let xml = part_text(&bytes, "word/document.xml");
        let bare =
            xml.replace(r#" xmlns:v="urn:schemas-microsoft-com:vml""#, "").replace(r#" xmlns:o="urn:schemas-microsoft-com:office:office""#, "");
        assert!(!bare.contains("xmlns:v="));
        let package = repackage(&bytes, &[("word/document.xml".into(), bare.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages");
        let mut doc = crate::docx::read_editable(&package).expect("reads");
        doc.page_fill = render_fixture("page-gradient.docx").1.page_fill;
        let saved = save(&doc).expect("saves");
        let again = crate::docx::read(&saved).expect("reads again");
        assert_eq!(again.page_fill, doc.page_fill);
        let xml = part_text(&saved, "word/document.xml");
        let root = &xml[xml.find("<w:document").unwrap()..];
        assert!(root[..root.find('>').unwrap()].contains(r#"xmlns:v="urn:schemas-microsoft-com:vml""#), "{xml}");
    }

    #[test]
    fn a_page_background_made_since_reading_is_written_in_its_model_form() {
        use crate::drawing::paint::Fill;
        // A gradient on a page that had none, its colour, shown.
        let (_, mut doc) = render_fixture("watermark-text.docx");
        let (_, gradient) = render_fixture("page-gradient.docx");
        doc.page_colour = Some(0xFF_FFFF);
        doc.page_fill = gradient.page_fill.clone();
        doc.settings.display_background_shape = true;
        let saved = save(&doc).expect("saves");
        let xml = part_text(&saved, "word/document.xml");
        let el = &xml[xml.find("<w:background").unwrap()..xml.find("</w:background>").unwrap()];
        assert!(
            el.starts_with(r#"<w:background w:color="FFFFFF"><v:background id="_x0000_s1025" o:bwmode="white""#) && !el.contains("xmlns"),
            "{el}"
        );
        let again = crate::docx::read(&saved).expect("reads again");
        assert_eq!((again.page_colour, again.settings.display_background_shape), (Some(0xFF_FFFF), true));
        let (Some(Fill::Gradient(a)), Some(Fill::Gradient(b))) = (&again.page_fill, &gradient.page_fill) else { panic!("{:?}", again.page_fill) };
        assert_eq!((a.kind, a.tile, a.flip), (b.kind, b.tile, b.flip));
        assert_eq!(a.stops.len(), b.stops.len());
        // A tiled picture: the image goes with the part.
        let (_, picture) = render_fixture("page-picture.docx");
        let mut doc = render_fixture("watermark-text.docx").1;
        doc.images = picture.images.clone();
        doc.page_fill = picture.page_fill.clone();
        let saved = save(&doc).expect("saves");
        let again = crate::docx::read(&saved).expect("reads again");
        let Some(Fill::Blip(b)) = &again.page_fill else { panic!("{:?}", again.page_fill) };
        assert_eq!(again.images[b.image.expect("its picture")], picture.images[0]);
    }

    /// A blank document's body as `body`, read for editing.
    fn body_doc(body: &str) -> Document {
        crate::docx::read_editable(&crate::editing::tests::blank_body(body)).expect("reads")
    }

    fn first_table(doc: &mut Document) -> &mut Table {
        doc.sections[0].blocks.iter_mut().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table")
    }

    /// A one-cell table, its `w:tblPr` children `pr` and its cell's start tag `tc`.
    fn one_cell(pr: &str, tc: &str, text: &str) -> String {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/>{pr}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr>{tc}<w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        )
    }

    #[test]
    fn a_boxs_content_starts_after_its_whole_start_tag() {
        let xml = r#"<wps:txbx><w:txbxContent x:a="1>2" y='3>4'><w:p/></w:txbxContent></wps:txbx><v:textbox><w:txbxContent/></v:textbox>"#;
        let got: Vec<&str> = box_contents(xml).into_iter().map(|r| &xml[r]).collect();
        assert_eq!(got, ["<w:p/>"]);
    }

    #[test]
    fn a_kept_rows_alignment_is_written_and_taken_off() {
        let mut doc = body_doc(&format!("{}<w:p/>", one_cell("", "<w:tc>", "a")));
        first_table(&mut doc).rows[0].jc = Some(Jc::Center);
        let saved = save(&doc).expect("saves");
        let mut again = crate::docx::read_editable(&saved).expect("reads again");
        assert_eq!(first_table(&mut again).rows[0].jc, Some(Jc::Center), "{}", part_text(&saved, "word/document.xml"));
        first_table(&mut again).rows[0].jc = None;
        let saved = save(&again).expect("saves");
        let mut back = crate::docx::read_editable(&saved).expect("reads again");
        assert_eq!(first_table(&mut back).rows[0].jc, None, "{}", part_text(&saved, "word/document.xml"));
    }

    #[test]
    fn markup_before_a_controls_first_row_stays_in_the_control() {
        let row =
            |t: &str| format!(r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr>"#);
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:bookmarkStart w:id="1" w:name="table"/><w:sdt xmlns:x="{W}"><w:sdtPr><w:id w:val="5"/></w:sdtPr><w:sdtContent><x:bookmarkStart x:id="7" x:name="rows"/>{}<x:bookmarkEnd x:id="7"/></w:sdtContent></w:sdt>{}<w:bookmarkEnd w:id="1"/></w:tbl><w:p/>"#,
            row("a"),
            row("b")
        );
        let mut doc = body_doc(&body);
        let t = first_table(&mut doc);
        assert!(!t.rows[0].controls.is_empty() && t.rows[1].controls.is_empty(), "the first row is in the control");
        t.rows[1].jc = Some(Jc::Center);
        let saved = save(&doc).expect("saves");
        let xml = part_text(&saved, "word/document.xml");
        roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("well formed: {e}: {xml}"));
        let at = |s: &str| xml.find(s).unwrap_or_else(|| panic!("{s} in {xml}"));
        assert!(at(r#"w:name="table""#) < at("<w:sdt ") && at("<w:sdt ") < at(r#"x:name="rows""#), "{xml}");
    }

    #[test]
    fn a_header_reference_goes_by_its_parsed_slot() {
        let doc = body_doc("<w:p/>");
        let mut was = doc.sections[0].page();
        was.headers = [Some("rId1".into()), Some("rId2".into()), None];
        let mut now = was.clone();
        now.headers[0] = None;
        let sect = r#"<w:sectPr><w:headerReference w:type = 'default' r:id="rId1"/><w:headerReference w:type="first" r:id="rId2"/></w:sectPr>"#;
        let out = sect_with_page(sect, "w:", &now, Some(&was), None, &People::default());
        assert!(!out.contains("rId1") && out.contains(r#"<w:headerReference w:type="first" r:id="rId2"/>"#), "{out}");
    }

    #[test]
    fn a_new_blip_declares_its_relationship_namespace() {
        let a = super::super::xml::A;
        let src = r#"<w:drawing><a:graphic><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId1"/></a:graphic></w:drawing>"#;
        let out = Placement { blip: Some(2), ..Placement::default() }.apply(src);
        let xml = format!(r#"<x xmlns:w="{W}" xmlns:a="{a}">{out}</x>"#);
        let dom = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("well formed: {e}: {out}"));
        let blip = dom.descendants().find(|n| n.tag_name().name() == "blip").expect("a blip");
        assert_eq!(blip.attribute((R, "embed")), Some(format!("{NEW_IMAGE}2").as_str()));
    }

    #[test]
    fn a_vml_picture_named_by_its_relid_takes_its_new_image() {
        let src = r#"<w:pict><v:shape style="width:1pt;height:1pt"><v:imagedata o:relid="rId3" o:title=""/></v:shape></w:pict>"#;
        let out = picture_patched(src, Placement::default(), Some(0), Some(1));
        assert!(out.contains(&format!(r#"o:relid="{NEW_IMAGE}1""#)), "{out}");
        let src = r#"<w:pict><v:shape style="width:1pt;height:1pt"><v:imagedata r:id="rId3" o:relid="rId4"/></v:shape></w:pict>"#;
        let out = picture_patched(src, Placement::default(), Some(0), Some(1));
        assert!(out.contains(&format!(r#"r:id="{NEW_IMAGE}1""#)) && out.contains(r#"o:relid="rId4""#), "{out}");
    }

    #[test]
    fn only_relationship_attributes_take_a_new_images_id() {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("[Content_Types].xml", zip::write::SimpleFileOptions::default()).expect("a part");
        z.write_all(br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>"#).expect("written");
        let package = z.finish().expect("a package").into_inner();
        let (wp, a) = (super::super::xml::WP, super::super::xml::A);
        let xml = format!(
            r#"<w:document xmlns:w="{W}" xmlns:wp="{wp}" xmlns:a="{a}" xmlns:r="{R}"><wp:docPr id="1" descr="{NEW_IMAGE}0"/><a:blip r:embed="{NEW_IMAGE}0"/></w:document>"#
        );
        let mut parts = vec![("word/document.xml".to_string(), xml.into_bytes())];
        add_images(&package, &[crate::render::svg::png(1, 1, &[0, 0, 0, 255])], &mut parts).expect("adds the image");
        let out = String::from_utf8(parts[0].1.clone()).expect("text");
        assert!(out.contains(&format!(r#"descr="{NEW_IMAGE}0""#)) && out.contains(r#"r:embed="rIdCeImg1""#), "{out}");
    }

    fn edit_first_cell(t: &mut Table) {
        let Block::Para(p) = &mut t.rows[0].cells[0].blocks[0] else { panic!("a paragraph") };
        let Some(Inline::Text(s, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Text(..))) else { panic!("text") };
        s.push_str(" more");
    }

    #[test]
    fn a_joined_table_keeps_each_part_s_shading_when_written_as_one() {
        let shd = |fill: &str| format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{fill}"/>"#);
        for (later, want) in [(shd("0000FF"), (Some(0x00_00FF), None)), (String::new(), (None, Some((None, None))))] {
            let body = format!("{}{}<w:p/>", one_cell(&shd("FF0000"), "<w:tc>", "a"), one_cell(&later, "<w:tc>", "b"));
            let mut doc = body_doc(&body);
            let t = first_table(&mut doc);
            assert_eq!(t.rows.len(), 2, "read as one");
            edit_first_cell(t);
            let saved = save(&doc).expect("saves");
            let mut again = crate::docx::read_editable(&saved).expect("reads again");
            let t = first_table(&mut again);
            assert_eq!(t.rows[0].shading, None);
            let row = &t.rows[1];
            let got = (row.shading, if want.0.is_some() { None } else { row.table_shading });
            assert_eq!(got, want, "{}", part_text(&saved, "word/document.xml"));
        }
    }

    #[test]
    fn a_joined_table_s_later_rows_stay_unshaded_under_shading_the_first_takes_since_reading() {
        let body = format!("{}{}<w:p/>", one_cell("", "<w:tc>", "a"), one_cell("", "<w:tc>", "b"));
        let mut doc = body_doc(&body);
        let t = first_table(&mut doc);
        assert_eq!(t.rows[1].table_shading, Some((None, None)));
        t.direct = Raw(Some(Rc::from(r#"<w:tblPr><w:shd w:val="clear" w:color="auto" w:fill="00FF00"/></w:tblPr>"#)));
        let saved = save(&doc).expect("saves");
        let mut again = crate::docx::read_editable(&saved).expect("reads again");
        let t = first_table(&mut again);
        assert_eq!(t.rows[1].table_shading, Some((None, None)), "{}", part_text(&saved, "word/document.xml"));
    }

    #[test]
    fn a_joined_table_s_later_rows_take_its_exceptions_in_schema_order() {
        let shd = r#"<w:shd w:val="clear" w:color="auto" w:fill="0000FF"/>"#;
        let mar = r#"<w:tblCellMar><w:left w:w="200" w:type="dxa"/></w:tblCellMar>"#;
        for (tr, edit) in [("<w:tr>", false), ("<w:tr>", true), ("<w:tr><w:tblPrEx/>", false)] {
            let later = one_cell(&format!("<w:jc w:val=\"center\"/>{shd}{mar}"), "<w:tc>", "b").replace("<w:tr>", tr);
            let body = format!("{}{later}<w:p/>", one_cell(shd, "<w:tc>", "a"));
            let mut doc = body_doc(&body);
            let t = first_table(&mut doc);
            if edit {
                let cell = t.rows[1].cells[0].clone();
                t.rows[1].cells.push(cell);
            }
            let saved = save(&doc).expect("saves");
            let xml = part_text(&saved, "word/document.xml");
            let ex = &xml[xml.find("<w:tblPrEx>").expect("exceptions")..xml.find("</w:tblPrEx>").unwrap()];
            let at = |name: &str| ex.find(&format!("<w:{name}")).unwrap_or_else(|| panic!("{name} in {ex}"));
            assert!(at("jc") < at("shd") && at("shd") < at("tblCellMar"), "{ex}");
        }
    }

    #[test]
    fn a_table_s_width_and_shading_change_together_in_a_self_closing_tbl_pr() {
        let body = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>"#;
        let mut doc = body_doc(body);
        let t = first_table(&mut doc);
        t.width = Width::Dxa(144.0);
        t.direct = Raw(Some(Rc::from(r#"<w:tblPr><w:shd w:val="clear" w:color="auto" w:fill="00FF00"/></w:tblPr>"#)));
        let saved = save(&doc).expect("saves");
        let xml = part_text(&saved, "word/document.xml");
        assert!(
            xml.contains(r#"<w:tblPr><w:tblW w:w="2880" w:type="dxa"/><w:shd w:val="clear" w:color="auto" w:fill="00FF00"/></w:tblPr>"#),
            "{xml}"
        );
    }

    #[test]
    fn a_cell_s_id_alone_changing_is_written_over_its_own_however_quoted() {
        let body = format!("{}<w:p/>", one_cell("", "<w:tc w:id='old'>", "a"));
        for id in [Some("new"), None] {
            let mut doc = body_doc(&body);
            let cell = &mut first_table(&mut doc).rows[0].cells[0];
            assert_eq!(cell.ids.as_ref().and_then(|i| i.id.as_deref()), Some("old"));
            cell.ids = id.map(|v| Rc::new(crate::docx::CellIds { id: Some(v.into()), headers: Vec::new() }));
            let saved = save(&doc).expect("saves");
            let xml = part_text(&saved, "word/document.xml");
            roxmltree::Document::parse(&xml).expect("well formed");
            let mut again = crate::docx::read_editable(&saved).expect("reads again");
            let ids = first_table(&mut again).rows[0].cells[0].ids.clone();
            assert_eq!(ids.and_then(|i| i.id.clone()).as_deref(), id, "{xml}");
        }
    }

    #[test]
    fn a_page_fill_takes_a_free_prefix_and_kept_markup_keeps_its_own() {
        use crate::drawing::paint::Fill;
        // The root's `v` is another namespace's: the fill's VML takes a free prefix.
        let (bytes, _) = render_fixture("page-colour.docx");
        let xml = part_text(&bytes, "word/document.xml").replace(r#"xmlns:v="urn:schemas-microsoft-com:vml""#, r#"xmlns:v="urn:example:other""#);
        let package = repackage(&bytes, &[("word/document.xml".into(), xml.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages");
        let mut doc = crate::docx::read_editable(&package).expect("reads");
        doc.page_fill = render_fixture("page-gradient.docx").1.page_fill;
        let saved = save(&doc).expect("saves");
        let out = part_text(&saved, "word/document.xml");
        roxmltree::Document::parse(&out).expect("well formed");
        assert!(out.contains(r#"xmlns:v1="urn:schemas-microsoft-com:vml""#) && out.contains("<v1:background"), "{out}");
        assert_eq!(crate::docx::read(&saved).expect("reads again").page_fill, doc.page_fill);
        // VML declared on the `w:background` alone: a colour change keeps the fill, and its declaration.
        let (bytes, read) = render_fixture("page-picture.docx");
        let xml = part_text(&bytes, "word/document.xml");
        let decl = r#" xmlns:v="urn:schemas-microsoft-com:vml""#;
        let local = xml.replacen(decl, "", 1).replacen("<w:background ", &format!("<w:background{decl} "), 1);
        assert!(!local[..local.find("<w:body").unwrap()].contains(&format!("<w:document{decl}")));
        let package = repackage(&bytes, &[("word/document.xml".into(), local.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages");
        let mut doc = crate::docx::read_editable(&package).expect("reads");
        assert!(matches!(doc.page_fill, Some(Fill::Blip(_))), "{:?}", doc.page_fill);
        doc.page_colour = read.page_colour.map(|c| c ^ 0x10);
        let saved = save(&doc).expect("saves");
        let out = part_text(&saved, "word/document.xml");
        roxmltree::Document::parse(&out).expect("well formed");
        assert!(out.contains(&format!("<w:background w:color=\"FFFFEF\"{decl}><v:background")), "{out}");
        assert_eq!(crate::docx::read(&saved).expect("reads again").page_fill, doc.page_fill);
    }

    #[test]
    fn a_kept_page_fill_s_local_declarations_leave_the_background_wordprocessingml() {
        use crate::drawing::paint::Fill;
        let (bytes, read) = render_fixture("page-picture.docx");
        let xml = part_text(&bytes, "word/document.xml");
        let open = r#"<w:background w:color="FFFFFF">"#;
        assert!(xml.contains(open));
        for (tag, attrs) in [("x:background", format!(r#"xmlns:x="{W}" x:color"#)), ("background", format!(r#"xmlns="{W}" color"#))] {
            let local = xml.replacen(open, &format!(r#"<{tag} xmlns:w="urn:example:other" {attrs}="FFFFFF">"#), 1).replacen(
                "</w:background>",
                &format!("</{tag}>"),
                1,
            );
            let package =
                repackage(&bytes, &[("word/document.xml".into(), local.into_bytes())], zip::CompressionMethod::Deflated).expect("repackages");
            let mut doc = crate::docx::read_editable(&package).expect("reads");
            assert!(matches!(doc.page_fill, Some(Fill::Blip(_))), "{:?}", doc.page_fill);
            doc.page_colour = read.page_colour.map(|c| c ^ 0x10);
            let saved = save(&doc).expect("saves");
            let out = part_text(&saved, "word/document.xml");
            roxmltree::Document::parse(&out).expect("well formed");
            assert!(out.contains(r#"xmlns:w="urn:example:other""#), "{out}");
            let again = crate::docx::read(&saved).expect("reads again");
            assert_eq!((again.page_colour, &again.page_fill), (doc.page_colour, &doc.page_fill), "{out}");
        }
    }

    #[test]
    fn a_replaced_tiled_picture_is_written_with_its_new_image() {
        use crate::drawing::paint::Fill;
        let (_, mut doc) = render_fixture("page-picture.docx");
        let Some(Fill::Blip(mut b)) = doc.page_fill.clone() else { panic!("{:?}", doc.page_fill) };
        let png = crate::render::svg::png(2, 2, &[0, 128, 255, 255].repeat(4));
        doc.images.push(png.clone());
        b.image = Some(doc.images.len() - 1);
        doc.page_fill = Some(Fill::Blip(b));
        let saved = save(&doc).expect("saves");
        let again = crate::docx::read(&saved).expect("reads again");
        let Some(Fill::Blip(b)) = &again.page_fill else { panic!("{:?}", again.page_fill) };
        assert_eq!(again.images[b.image.expect("its picture")], png);
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
