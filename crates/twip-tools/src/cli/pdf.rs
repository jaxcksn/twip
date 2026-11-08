//! PDF export command

use std::path::Path;

/// Exports `docx` to `out` with `options` (an open password, keywords). Returns an error message if the document can't
/// be read or the PDF can't be written.
pub fn run(docx: &Path, out: &Path, options: &twip::api::session::PdfOptions) -> Result<(), String> {
    options.validate().map_err(|e| e.message)?;
    let bytes = std::fs::read(docx).map_err(|e| format!("{}: {e}", docx.display()))?;
    // A fixed clock (2026-10-30T00:00:00Z), as `dump` uses: DATE/TIME fields print the same on every run.
    let mut doc = twip::internal::docx::read_at(&bytes, 1_793_318_400).map_err(|e| format!("{}: {e:?}", docx.display()))?;
    doc.review.view = twip::internal::docx::review::MarkupView::NoMarkup;
    let fonts = twip::internal::font::Fonts::system();
    let layout = twip::internal::layout::layout(&doc, &fonts);
    let (pdf, warnings) = twip::internal::pdf::export(&doc, &layout, "", options, None).map_err(|e| e.message)?;
    for w in warnings {
        eprintln!("warning: {}: {}", w.code.name(), w.message);
    }
    std::fs::write(out, pdf).map_err(|e| format!("{}: {e}", out.display()))
}
