//! Headless DOCX to PDF conversion: no editor session, and safe to run on many threads at once.
//!
//! Build a [`FontSet`] once and share it; each [`convert`] runs on the thread that calls it.
//!
//! ```no_run
//! use twip::pdf::{FontSet, PdfOptions, convert};
//!
//! let fonts = FontSet::system();
//! let documents: Vec<Vec<u8>> = vec![std::fs::read("a.docx")?, std::fs::read("b.docx")?];
//! let pdfs: Vec<_> = std::thread::scope(|s| {
//!     let jobs: Vec<_> = documents.iter().map(|d| s.spawn(|| convert(d, &fonts, &PdfOptions::default()))).collect();
//!     jobs.into_iter().map(|j| j.join().expect("a conversion doesn't panic")).collect()
//! });
//! for pdf in pdfs {
//!     let pdf = pdf?;
//!     println!("{} bytes, {} warnings", pdf.bytes.len(), pdf.warnings.len());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub(crate) mod encrypt;
pub(crate) mod export;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub use crate::api::error::Error;
pub use crate::api::fonts::{FaceInfo, SubstitutionRule};
pub use crate::api::session::PdfOptions;
pub use crate::api::warning::Warning;
use crate::font::{Catalog, Fonts};

/// The faces conversions draw with, and how document font names map to them. Immutable once built; clone it to share
/// it between threads (a clone is a reference count).
#[derive(Clone, Default)]
pub struct FontSet {
    /// Shared by every conversion that uses the set, on any thread.
    catalog: Arc<Catalog>,
}

impl FontSet {
    /// A builder, starting with no fonts.
    pub fn builder() -> FontSetBuilder {
        FontSetBuilder::default()
    }

    /// The fonts installed on this system, with no substitution rules and no default family.
    pub fn system() -> FontSet {
        FontSet { catalog: Arc::clone(Fonts::system().catalog()) }
    }

    /// The families in the set, sorted.
    pub fn families(&self) -> Vec<String> {
        Fonts::with_catalog(Arc::clone(&self.catalog)).families()
    }
}

impl std::fmt::Debug for FontSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSet").field("families", &self.families().len()).finish()
    }
}

/// Builds a [`FontSet`].
#[derive(Debug, Default)]
#[must_use]
pub struct FontSetBuilder {
    system: bool,
    fonts: Vec<Vec<u8>>,
    files: Vec<PathBuf>,
    rules: Vec<SubstitutionRule>,
    default: Option<String>,
}

impl FontSetBuilder {
    /// Whether the set includes the fonts installed on this system. Off by default.
    pub fn system_fonts(mut self, include: bool) -> Self {
        self.system = include;
        self
    }

    /// Adds a font file's bytes: TrueType, OpenType, a collection, WOFF or WOFF2.
    pub fn font(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.fonts.push(bytes.into());
        self
    }

    /// Adds a font file by path, read when the set is built.
    pub fn font_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    /// Rules that draw one family in another, replacing earlier ones.
    pub fn substitutions(mut self, rules: Vec<SubstitutionRule>) -> Self {
        self.rules = rules;
        self
    }

    /// The family drawn when nothing else resolves a name.
    pub fn default_family(mut self, family: impl Into<String>) -> Self {
        self.default = Some(family.into());
        self
    }

    /// The set, and the faces each added font gave in the order added. A font that doesn't read fails with
    /// `InvalidArgument`, naming it.
    pub fn build(self) -> Result<(FontSet, Vec<FaceInfo>), Error> {
        let mut fonts = if self.system { Fonts::system() } else { Fonts::default() };
        let mut faces = Vec::new();
        for (i, bytes) in self.fonts.into_iter().enumerate() {
            let added = fonts.add_bytes(bytes).map_err(|e| Error::invalid(format!("font {i}: {}", e.message)))?;
            faces.extend(added);
        }
        for path in self.files {
            let bytes = std::fs::read(&path).map_err(|e| Error::invalid(format!("{}: {e}", path.display())))?;
            let added = fonts.add_bytes(bytes).map_err(|e| Error::invalid(format!("{}: {}", path.display(), e.message)))?;
            faces.extend(added);
        }
        if !self.rules.is_empty() {
            fonts.set_rules(self.rules);
        }
        if let Some(family) = self.default {
            fonts.set_default(&family);
        }
        Ok((FontSet { catalog: Arc::clone(fonts.catalog()) }, faces))
    }
}

/// A converted document.
#[derive(Clone, Debug)]
pub struct Conversion {
    /// The PDF.
    pub bytes: Vec<u8>,
    /// What the PDF couldn't keep exactly: the read's approximations, text drawn in another font, and what the
    /// options dropped.
    pub warnings: Vec<Warning>,
}

/// Converts DOCX bytes to a PDF of the final document (no revision marks or comments) on the calling thread. Safe to
/// call from any number of threads at once with the same `fonts`; no conversion waits on another.
///
/// Fails with `InvalidArgument` for an empty open password, `ImportFailed` for bytes that aren't a readable DOCX,
/// `Unsupported` for an encrypted or binary document, and `ExportFailed` for a font set with no faces.
pub fn convert(docx: &[u8], fonts: &FontSet, options: &PdfOptions) -> Result<Conversion, Error> {
    run(docx, fonts, options, None)
}

/// As [`convert`], stopping with `Aborted` soon after `cancel` becomes true.
pub fn convert_cancellable(docx: &[u8], fonts: &FontSet, options: &PdfOptions, cancel: &AtomicBool) -> Result<Conversion, Error> {
    run(docx, fonts, options, Some(cancel))
}

fn run(docx: &[u8], fonts: &FontSet, options: &PdfOptions, cancel: Option<&AtomicBool>) -> Result<Conversion, Error> {
    let fonts = Fonts::with_catalog(Arc::clone(&fonts.catalog));
    let (bytes, warnings) = export::convert(docx, &fonts, options, cancel)?;
    Ok(Conversion { bytes, warnings })
}
