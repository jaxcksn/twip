//! PDF export from a layout snapshot: the pages recorded by `render::graphics` and written by twip-graphics' PDF
//! writer, which embeds the fonts subset with ToUnicode maps (the runs carry their characters, so text extracts as
//! typed). Tagged for PDF/UA ([`super::tags`]): paragraphs as P/H1–H6 (a heading's number in it), list items as
//! L/LI/Lbl/LBody, tables as Table/TR/TH/TD with their scopes and header ids, pictures with alternative text as
//! Figures, text boxes as Divs, notes as Notes after their references, links (their text and pictures) as Links with
//! their annotations; headers and footers as pagination artifacts, decorative pictures, borders and shading as others.
//! PDF/UA is claimed while every figure has alternative text. Bookmarks are named destinations; an outline from the
//! headings, and title, language and creation date (with XMP) metadata, all of the text as the snapshot's markup view
//! shows it.
//!
//! The pages show the markup view the snapshot was laid out in: its revision marks and change bars are layout items.
//! Only layout items are drawn, so view overlays (comment tints, the selection, squiggles) never reach the PDF.

use crate::api::state::FontsState;
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

use twip_graphics::kurbo;
use twip_graphics::pdf::{self, Date};
use twip_graphics::scene::{Clip, Recorder};

use crate::api::error::{Error, ErrorCode};
use crate::api::session::PdfOptions;
use crate::api::warning::Warning;
use crate::docx::{Block, Document, Paragraph};
use crate::font::Fonts;
use crate::layout::{Snapshot, traverse};
use crate::render::Renderer;
use crate::render::graphics::Graphics;

use super::mark::Marker;
use super::tags::{self, Tags};

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
    let doc = crate::docx::read(docx).map_err(|e| Error::new(ErrorCode::ImportFailed, e))?;
    // Its imported HTML drawn as a session draws it
    let mut doc = crate::editing::opened(doc, fonts);
    doc.review.view = crate::docx::review::MarkupView::NoMarkup;
    doc.review.show_hidden = options.hidden_text.unwrap_or(false);
    let snapshot = crate::layout::layout_until(&doc, fonts, || cancel.is_some_and(|c| c.load(Ordering::Relaxed))).ok_or_else(aborted)?;
    let (bytes, written) = export_with(&doc, &snapshot, "", options, cancel, Some(fonts))?;
    let mut warnings = doc.diagnostics.warnings.clone();
    let state = crate::api::session::unopened_document_fonts(&doc, fonts);
    warnings.extend(crate::api::session::documents::fallback_warnings(&state));
    warnings.extend(written);
    Ok((bytes, warnings, state))
}

/// Whether `cancel` asks the conversion to stop.
pub(crate) fn cancelled(cancel: Option<&AtomicBool>) -> Result<(), Error> {
    match cancel {
        Some(c) if c.load(Ordering::Relaxed) => Err(aborted()),
        _ => Ok(()),
    }
}

fn aborted() -> Error {
    Error::new(ErrorCode::Aborted, "the conversion was cancelled")
}

/// The document's laid-out pages (`snapshot`, laid out completely) as PDF bytes, with what `options` adds, and the
/// warnings writing them gave. Stops with `Aborted` between pages once `cancel` is set. Metafile pictures draw no text.
pub fn export(
    doc: &Document,
    snapshot: &Snapshot,
    title: &str,
    options: &PdfOptions,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<u8>, Vec<Warning>), Error> {
    export_with(doc, snapshot, title, options, cancel, None)
}

/// [`export`], metafile pictures' text drawn with `fonts`.
pub fn export_with(
    doc: &Document,
    snapshot: &Snapshot,
    title: &str,
    options: &PdfOptions,
    cancel: Option<&AtomicBool>,
    fonts: Option<&Fonts>,
) -> Result<(Vec<u8>, Vec<Warning>), Error> {
    cancelled(cancel)?;
    let tags = Tags::of(doc, first_language(doc).as_deref().unwrap_or("en-US"), cancel).ok_or_else(aborted)?;
    let mut info = pdf::Info {
        title: if title.trim().is_empty() { tags::title(doc, &tags) } else { title.into() },
        language: Some(tags.lang.clone()),
        keywords: options.keywords_line().into_iter().collect(),
        creator: "Twip".into(),
        producer: "Twip".into(),
        created: Some(now()),
        // XMP metadata (title, dates) and a document id; dropped when encrypted, or when what's drawn breaks it.
        archival: options.open_password.is_none(),
        accessible: true,
    };

    // Each page as pictures: one per stretch of items with the same mark.
    let mut graphics = Graphics::default();
    graphics.with_text = true;
    if let Some(fonts) = fonts {
        graphics.set_fonts(fonts);
    }
    let renderer = Renderer::default();
    let mut rec = Recorder::new();
    let mut marker = Marker::new(doc, tags);
    let mut out_pages = Vec::with_capacity(snapshot.pages().len());
    let mut pieces = Vec::new();
    for (index, page) in snapshot.pages().iter().enumerate() {
        cancelled(cancel)?;
        let mut placed = Vec::new();
        let mut lines: HashMap<u32, Option<crate::layout::ParaId>> = HashMap::new();
        traverse::paint_lines(page, &mut |p| placed.push(p), &mut |line, para, l, dx, dy| {
            lines.insert(line, para);
            marker.line(index, para, l, dx, dy);
        });
        let mut parts = Vec::new();
        let mut current = None;
        // The clip open in `rec`.
        let mut clip: Option<[f64; 4]> = None;
        for p in &placed {
            marker.mark(index, p, lines.get(&p.line).copied().flatten(), &mut pieces);
            for (mark, item) in pieces.drain(..) {
                if current != Some(mark) {
                    if let Some(m) = current {
                        if clip.take().is_some() {
                            rec.pop();
                        }
                        parts.push((m, rec.finish()));
                    }
                    current = Some(mark);
                }
                if clip != p.clip {
                    if clip.is_some() {
                        rec.pop();
                    }
                    if let Some([x0, y0, x1, y1]) = p.clip {
                        rec.push_clip(Clip::Rect { rect: kurbo::Rect::new(x0, y0, x1, y1), aa: true });
                    }
                    clip = p.clip;
                }
                graphics.record_page_item(&mut rec, &renderer, page, &item, &doc.images);
            }
        }
        if let Some(m) = current {
            if clip.is_some() {
                rec.pop();
            }
            parts.push((m, rec.finish()));
        }
        out_pages.push(pdf::Page { width: page.w, height: page.h, parts, links: Vec::new(), destinations: Vec::new() });
    }
    let structure = marker.finish(&mut out_pages);
    // The PDF/UA claim: none with a figure that says nothing.
    info.accessible = structure.missing_alt == 0;
    let written = pdf::write(graphics.resources(), &out_pages, &structure.elements, &structure.outline, &info)
        .map_err(|e| Error::new(ErrorCode::ExportFailed, e.to_string()))?;
    let archival = written.archival;
    let mut out = written.bytes;
    let mut written = crate::docx::diagnostics::Diagnostics::default();
    for _ in 0..structure.missing_alt {
        written.warn(crate::api::warning::WarningCode::PdfFigureWithoutAlt, None, || {
            "a picture has no alternative text and isn't marked decorative: its figure says nothing to a screen reader, so the PDF doesn't claim PDF/UA".into()
        });
    }
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
/// (`twip-tools chrome-fonts`). `None` for a file that can't be read or a CFF face.
pub fn subset_chars(data: &[u8], chars: &str) -> Option<Vec<u8>> {
    let face = crate::font::shipped(data, "subset")?;
    let used: BTreeSet<u16> = chars.chars().filter_map(|c| face.glyph(c)).filter(|&g| g != 0).map(|g| g as u16).collect();
    twip_graphics::pdf::subset_truetype(data, 0, &used)
}

/// The language of the body's first paragraph.
fn first_language(doc: &Document) -> Option<String> {
    fn first(blocks: &[Block]) -> Option<&Paragraph> {
        blocks.iter().find_map(|b| match b {
            Block::Para(p) => Some(p),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| first(&c.blocks)),
        })
    }
    doc.sections.iter().find_map(|s| first(&s.blocks)).map(|p| p.mark.lang.clone()).filter(|l| !l.is_empty())
}

/// The current UTC time.
fn now() -> Date {
    let secs = crate::clock::unix_secs();
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let (year, month, day) = crate::docx::review::civil_from_days(days);
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
