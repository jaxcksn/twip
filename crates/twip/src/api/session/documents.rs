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

/// What to export.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    /// The format to write.
    pub format: ExportFormat,
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
        // A compound file: a binary .doc, or an encrypted DOCX.
        if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
            return Err(Error::new(ErrorCode::Unsupported, "binary and encrypted documents can't be opened"));
        }
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
        match options.format {
            ExportFormat::Docx => {
                let bytes = crate::docx::write::save(self.editor.document()).map_err(|e| Error::new(ErrorCode::ExportFailed, e))?;
                self.editor.mark_saved();
                if self.state.modified {
                    self.changed(Vec::new());
                }
                Ok(Export { bytes: Bytes(bytes), revision, warnings: Vec::new() })
            }
            ExportFormat::Pdf if self.fonts.fonts().face_count() == 0 => {
                Err(Error::new(ErrorCode::ExportFailed, "the font library has no font to draw text with"))
            }
            ExportFormat::Pdf => {
                let fonts = self.fonts.fonts();
                self.editor.complete(&fonts);
                let bytes = crate::pdf::export(self.editor.document(), &self.editor.snapshot(), "");
                Ok(Export { bytes: Bytes(bytes), revision, warnings: font_fallbacks(&self.editor, &fonts) })
            }
        }
    }
}

/// The families the document's text asks for that nothing in the library draws, which a PDF draws in the default.
fn font_fallbacks(editor: &Editor, fonts: &crate::font::Fonts) -> Vec<Warning> {
    let (doc, index) = (editor.document(), editor.index());
    let mut seen = std::collections::HashSet::new();
    let mut diagnostics = crate::docx::diagnostics::Diagnostics::default();
    for p in index.para_ids().filter_map(|id| index.para(doc, id)) {
        for i in &p.inlines {
            if let crate::docx::Inline::Text(_, run) = i
                && !run.ascii.is_empty()
                && seen.insert(run.ascii.as_str())
                && !fonts.resolves(&run.ascii, run.bold, run.italic)
            {
                let drawn = fonts.face(&run.ascii, run.bold, run.italic).map_or_else(|| "no font".to_string(), |f| f.family.clone());
                diagnostics.warn(WarningCode::PdfFontFallback, None, || format!("{} is drawn in {drawn}", run.ascii));
            }
        }
    }
    diagnostics.warnings
}
