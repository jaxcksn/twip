//! Opening, creating and exporting documents.

use serde::{Deserialize, Serialize};

use super::Session;
use crate::api::error::{Error, ErrorCode};
use crate::api::fonts::FontLibrary;
use crate::api::value::Bytes;
use crate::api::warning::{Warning, WarningCode};
use crate::docx::Document;
use crate::editing::Editor;

/// The formats a document exports to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// Office Open XML.
    #[default]
    Docx,
    /// PDF, laid out completely first.
    Pdf,
}

/// What to export: the format, and for a PDF what it writes beyond the pages.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "camelCase")]
pub enum ExportOptions {
    /// Office Open XML.
    #[default]
    Docx,
    /// PDF, laid out completely first.
    Pdf(PdfOptions),
}

impl ExportOptions {
    /// The format written.
    pub fn format(&self) -> ExportFormat {
        match self {
            ExportOptions::Docx => ExportFormat::Docx,
            ExportOptions::Pdf(_) => ExportFormat::Pdf,
        }
    }
}

impl From<ExportFormat> for ExportOptions {
    /// The format with default options.
    fn from(format: ExportFormat) -> ExportOptions {
        match format {
            ExportFormat::Docx => ExportOptions::Docx,
            ExportFormat::Pdf => ExportOptions::Pdf(PdfOptions::default()),
        }
    }
}

/// What a PDF writes beyond the pages. The default writes neither a password nor keywords.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PdfOptions {
    /// Encrypts the PDF (AES-256): readers ask for this password before showing anything. Must not be empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_password: Option<String>,
    /// Written to the PDF's keywords metadata, in order. Blank entries are dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

impl std::fmt::Debug for PdfOptions {
    /// Never prints the password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfOptions").field("open_password", &self.open_password.as_ref().map(|_| "***")).field("keywords", &self.keywords).finish()
    }
}

impl PdfOptions {
    /// Fails for an empty open password, which would write a PDF that looks protected and isn't.
    pub fn validate(&self) -> Result<(), Error> {
        match &self.open_password {
            Some(p) if p.is_empty() => Err(Error::invalid("the open password is empty")),
            _ => Ok(()),
        }
    }

    /// The keywords as the PDF writes them: blank ones dropped, the rest joined with ", ". `None` when none are left.
    pub(crate) fn keywords_line(&self) -> Option<String> {
        let kept: Vec<&str> = self.keywords.iter().map(String::as_str).filter(|k| !k.trim().is_empty()).collect();
        (!kept.is_empty()).then(|| kept.join(", "))
    }
}

/// An exported document, the revision it captures, and what writing it couldn't keep.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Export {
    /// The file. The C ABI returns it apart from the rest, as raw bytes.
    pub bytes: Bytes,
    /// The state revision the file holds.
    pub revision: u64,
    /// Fidelity notes from writing.
    pub warnings: Vec<Warning>,
}

/// A document read and laid out by [`Session::open_prepare`], waiting to replace the open one.
pub struct OpenToken {
    editor: Box<Editor>,
    warnings: Vec<Warning>,
}

/// A blank document's editor.
pub(super) fn blank(fonts: &FontLibrary) -> Editor {
    let document = crate::docx::read_editable(&crate::docx::blank::blank_docx()).expect("the blank document always reads");
    Editor::with_document(document, &fonts.fonts())
}

impl Session {
    /// Opens `bytes` in place of the current document, which stays when they don't read. Returns the read's warnings.
    pub fn open(&mut self, bytes: &[u8]) -> Result<Vec<Warning>, Error> {
        let token = self.open_prepare(bytes)?;
        self.open_commit(token)
    }

    /// Reads `bytes` and readies the editor for them without replacing the open document, for an open that may
    /// still be cancelled: all the work that can fail or take long happens here, and committing is a swap.
    pub fn open_prepare(&mut self, bytes: &[u8]) -> Result<OpenToken, Error> {
        unsupported_package(bytes)?;
        let document: Document = crate::docx::read_editable(bytes).map_err(|e| Error::new(ErrorCode::ImportFailed, e))?;
        let warnings = document.diagnostics.warnings.clone();
        Ok(OpenToken { editor: Box::new(Editor::with_document(document, &self.fonts.fonts())), warnings })
    }

    /// Replaces the open document with a prepared one. Returns the read's warnings.
    pub fn open_commit(&mut self, token: OpenToken) -> Result<Vec<Warning>, Error> {
        let mut warnings = token.warnings;
        self.editor = *token.editor;
        let read = warnings.iter().cloned().map(crate::api::event::Event::Warning).collect();
        warnings.extend(self.replaced(read));
        Ok(warnings)
    }

    /// Drops a prepared document: the open one stays.
    pub fn open_discard(&mut self, token: OpenToken) {
        drop(token);
    }

    /// A blank document in place of the current one.
    pub fn new_document(&mut self) {
        self.editor = blank(&self.fonts);
        self.replaced(Vec::new());
    }

    /// After the document was replaced: the session's settings apply to it, the state starts over, and `events` (the
    /// read's warnings) follow, with the warning naming the families no font draws. Returns that warning.
    fn replaced(&mut self, mut events: Vec<crate::api::event::Event>) -> Option<Warning> {
        self.presentation.clear();
        self.configure();
        let (fonts, warning) = self.fonts_opened();
        events.extend(warning.iter().cloned().map(crate::api::event::Event::Warning));
        events.extend(fonts);
        self.changed(events);
        warning
    }

    /// Writes the document as it is at one revision, which the export reports. A DOCX export saves it: the document
    /// is unmodified there, and edits after it are modified again. A PDF export lays out completely first, saves
    /// nothing, and warns of the text it draws in another font.
    pub fn export(&mut self, options: &ExportOptions) -> Result<Export, Error> {
        let revision = self.editor.revision();
        match options {
            ExportOptions::Docx => {
                let bytes = crate::docx::write::save(self.editor.document()).map_err(|e| Error::new(ErrorCode::ExportFailed, e))?;
                self.editor.mark_saved();
                if self.state.modified {
                    self.changed(Vec::new());
                }
                Ok(Export { bytes: Bytes(bytes), revision, warnings: Vec::new() })
            }
            ExportOptions::Pdf(pdf) => {
                pdf.validate()?;
                if self.fonts.fonts().face_count() == 0 {
                    return Err(Error::new(ErrorCode::ExportFailed, "the font library has no font to draw text with"));
                }
                let fonts = self.fonts.fonts();
                let (bytes, written) = if self.editor.document().review.view == crate::docx::review::MarkupView::NoMarkup
                    && !self.editor.shows_expansions()
                    && self.editor.editing_sub_template().is_none()
                {
                    self.editor.complete(&fonts);
                    crate::pdf::export::export(self.editor.document(), &self.editor.snapshot(), "", pdf, None)?
                } else {
                    let mut doc = self.editor.document().clone();
                    doc.review.view = crate::docx::review::MarkupView::NoMarkup;
                    crate::pdf::export::export(&doc, &crate::layout::layout(&doc, &fonts), "", pdf, None)?
                };
                let state = super::fonts::document_fonts(self.editor.document(), self.editor.index(), &fonts);
                let mut warnings = fallback_warnings(&state);
                warnings.extend(written);
                Ok(Export { bytes: Bytes(bytes), revision, warnings })
            }
        }
    }
}

/// Fails for a compound file: a binary .doc, or an encrypted DOCX.
pub(crate) fn unsupported_package(bytes: &[u8]) -> Result<(), Error> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err(Error::new(ErrorCode::Unsupported, "binary and encrypted documents can't be opened"));
    }
    Ok(())
}

/// What a PDF draws in another font: one warning per missing family and style, and one per family with missing
/// characters.
pub(crate) fn fallback_warnings(state: &crate::api::state::FontsState) -> Vec<Warning> {
    let mut diagnostics = crate::docx::diagnostics::Diagnostics::default();
    for m in &state.missing {
        diagnostics.warn(WarningCode::PdfFontFallback, None, || super::fonts::describe(m));
    }
    for c in &state.characters {
        diagnostics.warn(WarningCode::PdfFontFallback, None, || format!("no font has {} ({})", c.characters, c.family));
    }
    diagnostics.warnings
}
