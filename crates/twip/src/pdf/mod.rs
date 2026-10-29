//! PDF export from a layout snapshot: the pages drawn by `render` on Skia's PDF backend, which embeds the
//! fonts with ToUnicode maps (the runs carry their characters, so text extracts as typed). Tagged: body paragraphs
//! as P/H1–H6, list items as L/LI/LBody, tables as Table/TR/TH/TD, pictures as Figures; headers, footers, notes, text
//! boxes, list numbers, borders and shading as artifacts. A bookmarks outline from the headings, and title, language
//! and creation date (with XMP) metadata.
//!
//! The pages show the markup view the snapshot was laid out in: its revision marks and change bars are layout items.
//! Only layout items are drawn, so view overlays (comment tints, the selection, squiggles) never reach the PDF.
// shortcut: no hyperlink annotations, figure alt text or Note elements: the docx model carries no hyperlinks, alt
// text or story of a run yet.

use std::collections::{BTreeSet, HashMap};
use std::ffi::CString;

use skia_safe::pdf::{self, DateTime, Metadata, Outline, StructureElementNode, node_id};

use crate::docx::{Block, Document, Paragraph};
use crate::layout::fragment::Item;
use crate::layout::{Snapshot, traverse};
use crate::render::Renderer;

/// A structure element before it becomes Skia's node. `key`: the model path a container stands for (reused by the
/// next paragraph in it).
struct Elem {
    kind: String,
    id: i32,
    key: Option<Vec<usize>>,
    kids: Vec<Elem>,
}

impl Elem {
    fn new(kind: &str, id: i32, key: Option<Vec<usize>>) -> Elem {
        Elem { kind: kind.into(), id, key, kids: Vec::new() }
    }

    /// The last kid when it is `kind` for `key`, else a new one.
    fn child(&mut self, kind: &str, key: Option<&[usize]>, next: &mut i32) -> &mut Elem {
        let reuse = key.is_some() && self.kids.last().is_some_and(|k| k.kind == kind && k.key.as_deref() == key);
        if !reuse {
            *next += 1;
            self.kids.push(Elem::new(kind, *next, key.map(<[usize]>::to_vec)));
        }
        self.kids.last_mut().unwrap()
    }

    fn into_node<'a>(self, th: &'a (CString, CString, CString)) -> StructureElementNode<'a> {
        let mut n = StructureElementNode::new(&self.kind);
        n.set_node_id(self.id);
        if self.kind == "Figure" {
            n.set_alt("Picture");
        }
        if self.kind == "TH" {
            n.attributes_mut().append_name(&th.0, &th.1, &th.2);
        }
        for k in self.kids {
            n.append_child(k.into_node(th));
        }
        n
    }
}

/// The body's paragraphs in document order with their paths ([section, block, (row, cell, block)*]) and whether
/// each table step is a header row.
fn body_paragraphs(doc: &Document) -> Vec<(Vec<usize>, Vec<bool>, &Paragraph)> {
    fn walk<'d>(blocks: &'d [Block], path: &mut Vec<usize>, header: &mut Vec<bool>, out: &mut Vec<(Vec<usize>, Vec<bool>, &'d Paragraph)>) {
        for (k, b) in blocks.iter().enumerate() {
            path.push(k);
            match b {
                Block::Para(p) => out.push((path.clone(), header.clone(), p)),
                Block::Table(t) => {
                    for (r, row) in t.rows.iter().enumerate() {
                        for (c, cell) in row.cells.iter().enumerate() {
                            path.push(r);
                            path.push(c);
                            header.push(row.header);
                            walk(&cell.blocks, path, header, out);
                            header.pop();
                            path.pop();
                            path.pop();
                        }
                    }
                }
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    for (i, s) in doc.sections.iter().enumerate() {
        walk(&s.blocks, &mut vec![i], &mut Vec::new(), &mut out);
    }
    out
}

/// The document's laid-out pages (`snapshot`, laid out completely) as PDF bytes.
pub fn export(doc: &Document, snapshot: &Snapshot, title: &str) -> Vec<u8> {
    let paras = body_paragraphs(doc);
    // Glyph runs name their paragraph by id; body paragraphs are tagged, the rest (headers, notes...) are artifacts.
    let index: HashMap<u64, usize> = paras.iter().enumerate().map(|(i, p)| (p.2.id, i)).collect();
    let pages: Vec<Vec<Item>> = snapshot.pages().iter().map(|p| traverse::paint_items(p)).collect();

    // Node ids: paragraph i is i + 1, figures follow, then containers.
    let n = paras.len() as i32;
    let mut drawn = vec![false; paras.len()];
    let mut figures: Vec<(Option<usize>, i32)> = Vec::new();
    let mut ids: Vec<Vec<i32>> = Vec::new();
    let mut last = None;
    for items in &pages {
        let mut page_ids = Vec::with_capacity(items.len());
        for item in items {
            page_ids.push(match item {
                Item::Glyphs(r) => match r.para.and_then(|p| index.get(&p.0)) {
                    // Whitespace alone (an empty paragraph's caret stop) doesn't make a paragraph's element.
                    Some(&i) if r.chars.iter().any(|c| c.2.is_some() && !c.0.is_whitespace()) => {
                        drawn[i] = true;
                        last = Some(i);
                        i as i32 + 1
                    }
                    Some(&i) if drawn[i] => i as i32 + 1,
                    Some(_) => node_id::OTHER_ARTIFACT,
                    None => node_id::PAGINATION_ARTIFACT,
                },
                Item::Rect { .. } | Item::Gridline { .. } => node_id::LAYOUT_ARTIFACT,
                Item::Image { .. } => {
                    figures.push((last, n + 1 + figures.len() as i32));
                    figures.last().unwrap().1
                }
            });
        }
        ids.push(page_ids);
    }
    // The structure tree, in document order.
    let mut next = n + figures.len() as i32;
    let mut root = Elem::new("Document", 0, None);
    for (i, (path, header, p)) in paras.iter().enumerate() {
        if !drawn[i] && !figures.iter().any(|f| f.0 == Some(i)) {
            continue;
        }
        let mut c = &mut root;
        // A table per (row, cell) step of the path.
        let mut k = 1;
        let mut step = 0;
        while k + 3 < path.len() {
            c = c.child("Table", Some(&path[..=k]), &mut next);
            c = c.child("TR", Some(&path[..=k + 1]), &mut next);
            c = c.child(if header[step] { "TH" } else { "TD" }, Some(&path[..=k + 2]), &mut next);
            k += 3;
            step += 1;
        }
        let kind = match p.style.outline_lvl {
            Some(l @ 0..6) => format!("H{}", l + 1),
            _ => "P".into(),
        };
        let mut e = Elem::new(&kind, i as i32 + 1, None);
        if kind == "P" && p.list.is_some() {
            let li = c.child("L", Some(&[][..]), &mut next).child("LI", None, &mut next);
            e.kind = "LBody".into();
            li.kids.push(e);
            c = li;
        } else {
            c.kids.push(e);
        }
        let e = c.kids.last_mut().unwrap();
        e.kids.extend(figures.iter().filter(|f| f.0 == Some(i)).map(|f| Elem::new("Figure", f.1, None)));
    }
    root.id = next + 1;
    root.kids.extend(figures.iter().filter(|f| f.0.is_none()).map(|f| Elem::new("Figure", f.1, None)));

    let th = (CString::new("Table").unwrap(), CString::new("Scope").unwrap(), CString::new("Column").unwrap());
    let lang = paras.first().map(|p| p.2.mark.lang.clone()).filter(|l| !l.is_empty());
    let metadata = Metadata {
        title: if title.trim().is_empty() { "Document".into() } else { title.into() },
        creator: "Twip".into(),
        producer: "Twip".into(),
        creation: Some(now()),
        lang: lang.unwrap_or_else(|| "en-US".into()),
        pdf_a: true, // XMP metadata (title, dates) and a document id
        structure_element_tree_root: Some(root.into_node(&th)),
        outline: Outline::StructureElementHeaders,
        ..Default::default()
    };

    let mut out = Vec::new();
    {
        let mut pdoc = pdf::new_document(&mut out, Some(&metadata));
        let mut renderer = Renderer::default();
        renderer.with_text = true;
        // Fonts are embedded as subsets of the glyphs drawn. A variable font's instance is Skia's to embed: the subset
        // keeps only the default outlines (FONT-17).
        let mut used: HashMap<(usize, u32, u32), (&crate::font::Face, BTreeSet<u16>)> = HashMap::new();
        for item in pages.iter().flatten() {
            if let Item::Glyphs(r) = item
                && r.face.variations.is_empty()
            {
                let key = (std::sync::Arc::as_ptr(&r.face.data) as usize, r.face.index, r.face.instance);
                used.entry(key).or_insert_with(|| (&*r.face, BTreeSet::new())).1.extend(r.glyphs.iter().map(|g| g.0 as u16));
            }
        }
        for (face, glyphs) in used.values() {
            if let Some(data) = subset(&face.data, face.index, glyphs) {
                renderer.set_face_data(face, &data);
            }
        }
        for ((page, items), page_ids) in snapshot.pages().iter().zip(&pages).zip(&ids) {
            let mut on_page = pdoc.begin_page((page.w as f32, page.h as f32), None);
            let canvas = on_page.canvas();
            let mut current = node_id::NOTHING;
            for (item, &id) in items.iter().zip(page_ids) {
                if id != current {
                    pdf::set_node_id(canvas, id);
                    current = id;
                }
                renderer.draw_item(canvas, item, &doc.images);
            }
            pdoc = on_page.end_page();
        }
        pdoc.close();
    }
    out
}

/// A single-face TrueType file with `face`'s glyph ids but only the `used` glyphs' outlines (and their components),
/// without shaping and hinting tables: what a PDF embeds (Skia's own subsetting needs its HarfBuzz build). `None` for
/// CFF faces and unreadable files, which embed whole.
fn subset(data: &[u8], index: u32, used: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let be16 = |b: &[u8], o: usize| b.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let be32 = |b: &[u8], o: usize| b.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let base = if data.get(..4)? == b"ttcf" { be32(data, 12 + 4 * index as usize)? as usize } else { 0 };
    let mut tables: HashMap<[u8; 4], &[u8]> = HashMap::new();
    for i in 0..be16(data, base + 4)? as usize {
        let r = base + 12 + 16 * i;
        let (off, len) = (be32(data, r + 8)? as usize, be32(data, r + 12)? as usize);
        tables.insert(data.get(r..r + 4)?.try_into().ok()?, data.get(off..off + len)?);
    }
    let (head, maxp, loca, glyf) = (*tables.get(b"head")?, *tables.get(b"maxp")?, *tables.get(b"loca")?, *tables.get(b"glyf")?);
    let long = be16(head, 50)? != 0;
    let count = be16(maxp, 4)? as usize;
    let at = |g: usize| if long { be32(loca, 4 * g).map(|o| o as usize) } else { be16(loca, 2 * g).map(|o| 2 * o as usize) };
    let outline = |g: usize| glyf.get(at(g)?..at(g + 1)?);

    // Composite glyphs draw their components: keep those too.
    let mut keep: BTreeSet<u16> = used.iter().copied().filter(|&g| (g as usize) < count).collect();
    keep.insert(0);
    let mut todo: Vec<u16> = keep.iter().copied().collect();
    while let Some(g) = todo.pop() {
        let o = outline(g as usize)?;
        if o.len() < 10 || be16(o, 0)? as i16 >= 0 {
            continue;
        }
        let mut p = 10;
        loop {
            let (flags, component) = (be16(o, p)?, be16(o, p + 2)?);
            if (component as usize) < count && keep.insert(component) {
                todo.push(component);
            }
            p += 4 + if flags & 1 != 0 { 4 } else { 2 };
            p += if flags & 0x80 != 0 {
                8
            } else if flags & 0x40 != 0 {
                4
            } else if flags & 0x08 != 0 {
                2
            } else {
                0
            };
            if flags & 0x20 == 0 {
                break;
            }
        }
    }
    let (mut new_glyf, mut new_loca) = (Vec::new(), Vec::with_capacity(4 * (count + 1)));
    for g in 0..count {
        new_loca.extend((new_glyf.len() as u32).to_be_bytes());
        if keep.contains(&(g as u16)) {
            new_glyf.extend_from_slice(outline(g)?);
            new_glyf.resize(new_glyf.len().next_multiple_of(4), 0);
        }
    }
    new_loca.extend((new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head.to_vec();
    new_head.get_mut(8..12)?.fill(0); // checksumAdjustment
    new_head.get_mut(50..52)?.copy_from_slice(&1u16.to_be_bytes()); // long loca
    // post format 3: no glyph names.
    let post = tables.get(b"post").filter(|p| p.len() >= 32).map(|p| [&0x0003_0000u32.to_be_bytes()[..], &p[4..32]].concat());

    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = vec![(*b"glyf", new_glyf), (*b"head", new_head), (*b"loca", new_loca)];
    out_tables.extend(post.map(|p| (*b"post", p)));
    for tag in [b"cmap", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2"] {
        out_tables.push((*tag, tables.get(tag)?.to_vec()));
    }
    out_tables.sort_by_key(|t| t.0);
    let n = out_tables.len();
    let selector = n.ilog2();
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    for v in [n as u16, 16 << selector, selector as u16, (n as u16) * 16 - (16 << selector)] {
        out.extend(v.to_be_bytes());
    }
    let mut offset = 12 + 16 * n;
    for (tag, body) in &out_tables {
        let sum = body.chunks(4).fold(0u32, |s, c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            s.wrapping_add(u32::from_be_bytes(w))
        });
        out.extend(tag);
        out.extend(sum.to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((body.len() as u32).to_be_bytes());
        offset += body.len().next_multiple_of(4);
    }
    for (_, body) in &out_tables {
        out.extend(body);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    Some(out)
}

/// The current UTC time.
fn now() -> DateTime {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    DateTime {
        time_zone_minutes: 0,
        year: year as u16,
        month: month as u8,
        day_of_week: (days + 4).rem_euclid(7) as u8,
        day: day as u8,
        hour: (rem / 3600) as u8,
        minute: (rem / 60 % 60) as u8,
        second: (rem % 60) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    /// Exports fixtures and checks the PDF's shape; `PDF_OUT` (a folder) keeps the files for inspection.
    #[test]
    fn exports_fixtures() {
        let fonts = Fonts::system();
        let root = std::env::var_os("TWIP_REFERENCE_FIXTURES").map(std::path::PathBuf::from).unwrap_or_default();
        let mut files: Vec<std::path::PathBuf> =
            ["T1/T1g_two_rows.docx", "L1/L1l_multilevel.docx", "P1/P1c_word_default.docx"].iter().map(|f| root.join(f)).collect();
        files.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STORIES.docx"));
        for f in &files {
            let Ok(bytes) = std::fs::read(f) else {
                eprintln!("{} not found; skipped", f.display());
                continue;
            };
            let mut e = crate::editing::Editor::open(&bytes, &fonts).unwrap();
            e.complete(&fonts);
            let pdf = export(e.document(), &e.snapshot(), "Fixture");
            let text = String::from_utf8_lossy(&pdf);
            assert!(pdf.starts_with(b"%PDF-") && text.contains("%%EOF"), "{}", f.display());
            assert!(text.contains("/StructTreeRoot") && text.contains("/MarkInfo"), "{}: untagged", f.display());
            assert!(
                root.as_os_str().is_empty() || !f.starts_with(&root) || pdf.len() < 150_000,
                "{}: {} bytes, fonts not subset",
                f.display(),
                pdf.len()
            );
            if let Ok(dir) = std::env::var("PDF_OUT") {
                let name = f.file_stem().unwrap().to_string_lossy().into_owned();
                std::fs::write(std::path::Path::new(&dir).join(format!("{name}.pdf")), &pdf).unwrap();
            }
        }
    }
}
