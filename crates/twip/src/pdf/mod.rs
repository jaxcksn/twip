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
pub use crate::api::fonts::{Alias, AliasTarget, FaceInfo, FontRule, GenericDefaults, GenericFamily, SymbolFallback};
pub use crate::api::session::PdfOptions;
pub use crate::api::state::{MissingCharacters, MissingFont, StandIn};
pub use crate::api::warning::Warning;
use crate::font::{Catalog, FontSettings, Fonts};

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

    /// The fonts installed on this system and the bundled symbol fallback, with no other settings.
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
    settings: FontSettings,
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

    /// Draw-time replacements, replacing earlier ones: each applies even when the replaced family is in the set.
    pub fn replacements(mut self, rules: Vec<FontRule>) -> Self {
        self.settings.replacements = rules;
        self
    }

    /// Aliases, replacing earlier ones: each applies when its family isn't in the set.
    pub fn aliases(mut self, aliases: Vec<Alias>) -> Self {
        self.settings.aliases = aliases;
        self
    }

    /// The family drawn for a missing font of each generic category. None by default.
    pub fn generic_defaults(mut self, defaults: GenericDefaults) -> Self {
        self.settings.generic = defaults;
        self
    }

    /// Whether a document's font table gives a missing family its generic category. On by default; it has no effect
    /// until a generic default is set.
    pub fn classification(mut self, from_document: bool) -> Self {
        self.settings.classify_from_document = from_document;
        self
    }

    /// The general fallback: the family drawn when nothing else resolves a name.
    pub fn fallback(mut self, family: impl Into<String>) -> Self {
        self.settings.fallback = Some(family.into());
        self
    }

    /// The families searched, in order, for a character a run's font lacks.
    pub fn character_fallback(mut self, families: Vec<String>) -> Self {
        self.settings.character_fallback = families;
        self
    }

    /// The face for characters no other font has. [`SymbolFallback::Bundled`] by default; a font that doesn't read
    /// fails the build.
    pub fn symbol_fallback(mut self, fallback: SymbolFallback) -> Self {
        self.settings.symbol_fallback = fallback;
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
        if let SymbolFallback::Font(bytes) = &self.settings.symbol_fallback {
            Fonts::default().add_bytes(bytes.as_ref().clone()).map_err(|e| Error::invalid(format!("symbol fallback: {}", e.message)))?;
        }
        fonts.set_settings(|s| *s = self.settings);
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
    /// The document's fonts the set couldn't draw as the document asks, and the characters no font has.
    pub fonts: FontsReport,
}

/// The document's families (per style) that drew with a stand-in, and the characters no font has: the same report a
/// session gives, without pending families, since a conversion has no resolver.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontsReport {
    /// Families that drew with a stand-in.
    pub missing: Vec<MissingFont>,
    /// Characters no font has, by the family their runs name.
    pub characters: Vec<MissingCharacters>,
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
    let (bytes, warnings, state) = export::convert(docx, &fonts, options, cancel)?;
    Ok(Conversion { bytes, warnings, fonts: FontsReport { missing: state.missing, characters: state.characters } })
}
