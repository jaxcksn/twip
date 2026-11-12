//! PDF export from a layout snapshot: the pages recorded by `render::graphics` and written by twip-graphics' PDF
//! writer, which embeds the fonts subset with ToUnicode maps (the runs carry their characters, so text extracts as
//! typed). Tagged: body paragraphs
//! as P/H1–H6, list items as L/LI/LBody, tables as Table/TR/TH/TD, pictures as Figures; headers, footers, notes, text
//! boxes, list numbers, borders and shading as artifacts. A bookmarks outline from the headings, and title, language
//! and creation date (with XMP) metadata.
//!
//! The pages show the markup view the snapshot was laid out in: its revision marks and change bars are layout items.
//! Only layout items are drawn, so view overlays (comment tints, the selection, squiggles) never reach the PDF.
// shortcut: no hyperlink annotations, figure alt text or Note elements: the docx model carries no hyperlinks, alt
// text or story of a run yet.

use crate::api::state::FontsState;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

use twip_graphics::pdf::{self, ArtifactKind, Date, Mark};
use twip_graphics::scene::Recorder;

use crate::api::error::{Error, ErrorCode};
use crate::api::session::PdfOptions;
use crate::api::warning::Warning;
use crate::docx::{Block, Document, Paragraph};
use crate::font::Fonts;
use crate::layout::fragment::Item;
use crate::layout::{Snapshot, traverse};
use crate::render::Renderer;
use crate::render::graphics::Graphics;

/// What an item is in the structure: a structure element's content (its id), an artifact, or (before the first item)
/// nothing yet.
const OTHER_ARTIFACT: i32 = -1;
const PAGINATION_ARTIFACT: i32 = -2;
const LAYOUT_ARTIFACT: i32 = -3;
const NOTHING: i32 = 0;

/// A structure element before it becomes the writer's. `key`: the model path a container stands for (reused by the
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

    fn into_element(self) -> pdf::Element {
        let kind = match self.kind.as_str() {
            "P" => pdf::Kind::Paragraph,
            "L" => pdf::Kind::List,
            "LI" => pdf::Kind::ListItem,
            "LBody" => pdf::Kind::ListBody,
            "Table" => pdf::Kind::Table,
            "TR" => pdf::Kind::Row,
            "TH" => pdf::Kind::HeaderCell,
            "TD" => pdf::Kind::Cell,
            "Figure" => pdf::Kind::Figure(Some("Picture".into())),
            h => pdf::Kind::Heading(h.strip_prefix('H').and_then(|l| l.parse().ok()).unwrap_or(1)),
        };
        // Paragraphs, headings and figures are content; containers hold only their kids.
        let content =
            matches!(kind, pdf::Kind::Paragraph | pdf::Kind::Heading(_) | pdf::Kind::ListBody | pdf::Kind::Figure(_)).then_some(self.id as u32);
        pdf::Element { kind, content, kids: self.kids.into_iter().map(Elem::into_element).collect() }
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

/// Reads `docx` and writes it as a PDF on this thread: the final document, without revision marks or comments, laid
/// out with `fonts` in steps so `cancel` stops it early. The warnings are the read's, the text drawn in another font,
/// and what writing the PDF dropped.
pub(crate) fn convert(
    docx: &[u8],
    fonts: &Fonts,
    options: &PdfOptions,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<u8>, Vec<Warning>, FontsState), Error> {
    options.validate()?;
    if fonts.face_count() == 0 {
        return Err(Error::new(ErrorCode::ExportFailed, "the font library has no font to draw text with"));
    }
    crate::api::session::documents::unsupported_package(docx)?;
    let mut doc = crate::docx::read(docx).map_err(|e| Error::new(ErrorCode::ImportFailed, e))?;
    doc.review.view = crate::docx::review::MarkupView::NoMarkup;
    let snapshot = crate::layout::layout_until(&doc, fonts, || cancel.is_some_and(|c| c.load(Ordering::Relaxed)))
        .ok_or_else(|| cancelled(cancel).expect_err("layout stops only when cancelled"))?;
    let (bytes, written) = export(&doc, &snapshot, "", options, cancel)?;
    let mut warnings = doc.diagnostics.warnings.clone();
    let state = crate::api::session::unopened_document_fonts(&doc, fonts);
    warnings.extend(crate::api::session::documents::fallback_warnings(&state));
    warnings.extend(written);
    Ok((bytes, warnings, state))
}

/// Whether `cancel` asks the conversion to stop.
pub(crate) fn cancelled(cancel: Option<&AtomicBool>) -> Result<(), Error> {
    match cancel {
        Some(c) if c.load(Ordering::Relaxed) => Err(Error::new(ErrorCode::Aborted, "the conversion was cancelled")),
        _ => Ok(()),
    }
}

/// The document's laid-out pages (`snapshot`, laid out completely) as PDF bytes, with what `options` adds, and the
/// warnings writing them gave. Stops with `Aborted` between pages once `cancel` is set.
pub fn export(
    doc: &Document,
    snapshot: &Snapshot,
    title: &str,
    options: &PdfOptions,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<u8>, Vec<Warning>), Error> {
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
                    Some(_) => OTHER_ARTIFACT,
                    None => PAGINATION_ARTIFACT,
                },
                Item::Rect { .. } | Item::Gridline { .. } | Item::Turned(_) => LAYOUT_ARTIFACT,
                // A picture is a figure; another shape is an artifact.
                Item::Shape(s) if !matches!(s.fill, crate::drawing::paint::Paint::Picture { .. }) => LAYOUT_ARTIFACT,
                Item::Image { .. } | Item::Shape(_) => {
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

    let lang = paras.first().map(|p| p.2.mark.lang.clone()).filter(|l| !l.is_empty());
    // The bookmarks: the drawn headings, in document order.
    let outline: Vec<pdf::Heading> = paras
        .iter()
        .enumerate()
        .filter(|&(i, _)| drawn[i])
        .filter_map(|(i, (_, _, p))| match p.style.outline_lvl {
            Some(l @ 0..6) => {
                Some(pdf::Heading { level: l as u8 + 1, content: i as u32 + 1, title: crate::docx::fields::text(p).trim().to_string() })
            }
            _ => None,
        })
        .collect();
    let info = pdf::Info {
        title: if title.trim().is_empty() { "Document".into() } else { title.into() },
        language: Some(lang.unwrap_or_else(|| "en-US".into())),
        keywords: options.keywords_line().into_iter().collect(),
        creator: "Twip".into(),
        producer: "Twip".into(),
        created: Some(now()),
        // XMP metadata (title, dates) and a document id; dropped when encrypted, or when what's drawn breaks it.
        archival: options.open_password.is_none(),
    };

    // Each page as pictures: one per stretch of items with the same structure id.
    let mut graphics = Graphics::default();
    graphics.with_text = true;
    let renderer = Renderer::default();
    let mut rec = Recorder::new();
    let mut out_pages = Vec::with_capacity(pages.len());
    for ((page, items), page_ids) in snapshot.pages().iter().zip(&pages).zip(&ids) {
        cancelled(cancel)?;
        let mut parts = Vec::new();
        let mut current = NOTHING;
        for (item, &id) in items.iter().zip(page_ids) {
            if id != current {
                if current != NOTHING {
                    parts.push((mark(current), rec.finish()));
                }
                current = id;
            }
            graphics.record_item(&mut rec, &renderer, item, &doc.images);
        }
        if current != NOTHING {
            parts.push((mark(current), rec.finish()));
        }
        out_pages.push(pdf::Page { width: page.w, height: page.h, parts });
    }
    let structure: Vec<pdf::Element> = root.kids.into_iter().map(Elem::into_element).collect();
    let written =
        pdf::write(graphics.resources(), &out_pages, &structure, &outline, &info).map_err(|e| Error::new(ErrorCode::ExportFailed, e.to_string()))?;
    let archival = written.archival;
    let mut out = written.bytes;
    let mut written = crate::docx::diagnostics::Diagnostics::default();
    if !archival && options.open_password.is_none() {
        written.warn(crate::api::warning::WarningCode::PdfArchivalClaimDropped, None, || {
            "what the document draws breaks PDF/A-2b, so the PDF doesn't claim it".into()
        });
    }
    if let Some(password) = &options.open_password {
        out = super::encrypt::encrypt(&out, password)?;
        written.warn(crate::api::warning::WarningCode::PdfArchivalClaimDropped, None, || {
            "a password-protected PDF can't claim PDF/A conformance".into()
        });
    }
    Ok((out, written.warnings))
}

/// `data` (a single-face TrueType file) with only the outlines of `chars`' glyphs: the bundled chrome fonts
/// (`twip-tools chrome-fonts`). `None` for a file Skia can't read or a CFF face.
pub fn subset_chars(data: &[u8], chars: &str) -> Option<Vec<u8>> {
    let face = crate::font::shipped(data, "subset")?;
    let used: BTreeSet<u16> = chars.chars().filter_map(|c| face.glyph(c)).filter(|&g| g != 0).map(|g| g as u16).collect();
    twip_graphics::pdf::subset_truetype(data, 0, &used)
}

/// What a structure id marks.
fn mark(id: i32) -> Mark {
    match id {
        OTHER_ARTIFACT => Mark::Artifact(ArtifactKind::Other),
        PAGINATION_ARTIFACT => Mark::Artifact(ArtifactKind::Pagination),
        LAYOUT_ARTIFACT => Mark::Artifact(ArtifactKind::Layout),
        id => Mark::Content(id as u32),
    }
}

/// The current UTC time.
fn now() -> Date {
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
    Date { year: year as u16, month: month as u8, day: day as u8, hour: (rem / 3600) as u8, minute: (rem / 60 % 60) as u8, second: (rem % 60) as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    /// Exports fixtures and checks the PDF's shape; `PDF_OUT` (a folder) keeps the files for inspection.
    #[test]
    fn exports_fixtures() {
        let fonts = Fonts::system_recommended();
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
            let (pdf, _) = export(e.document(), &e.snapshot(), "Fixture", &PdfOptions::default(), None).expect("exports");
            let text = String::from_utf8_lossy(&pdf);
            assert!(pdf.starts_with(b"%PDF-") && text.contains("%%EOF"), "{}", f.display());
            assert!(text.contains("/StructTreeRoot") && text.contains("/MarkInfo"), "{}: untagged", f.display());
            assert!(text.contains("/ToUnicode"), "{}: the text doesn't map back to its characters", f.display());
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
