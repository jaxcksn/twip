//! Headless conversion: `twip::pdf` behind the C ABI. Nothing here touches an engine, a session or the engine thread,
//! so every entry point may be called from any thread at the same time.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};

use engine::api::error::{Error, ErrorCode};
use engine::pdf::{FontSet, PdfOptions, SubstitutionRule};
use serde::Deserialize;
use serde_json::json;

use super::{TwipBuf, bytes, put, request, respond, to_value};

/// What a font set is built from, besides the font bytes passed alongside.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct FontSetOptions {
    include_system_fonts: bool,
    font_files: Vec<String>,
    substitutions: Vec<SubstitutionRule>,
    default_family: Option<String>,
}

/// `f` with a panic reported as `Internal`.
fn caught<T>(f: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(Error::new(ErrorCode::Internal, "the conversion failed")))
}

/// Builds a font set (`opts`: `{ includeSystemFonts, fontFiles, substitutions, defaultFamily }`) with `count` font
/// files passed as bytes. `*out` gets the set, freed with `twip_pdf_fonts_free`; `faces` gets `{"ok": [FaceInfo]}` or
/// the error.
///
/// # Safety
/// `opts` is null or valid for `olen` bytes; `fonts` and `lens` are null or valid for `count` entries, each font
/// valid for its length; `out` and `faces` are null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_fonts_new(
    opts: *const u8,
    olen: usize,
    fonts: *const *const u8,
    lens: *const usize,
    count: usize,
    out: *mut *mut FontSet,
    faces: *mut TwipBuf,
) -> i32 {
    // SAFETY: the caller's promise.
    let opts = unsafe { bytes(opts, olen) };
    let data: Vec<&[u8]> = if fonts.is_null() || lens.is_null() || count == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller's promise: `count` pointers and lengths, each font valid for its length.
        let (ptrs, lens) = unsafe { (std::slice::from_raw_parts(fonts, count), std::slice::from_raw_parts(lens, count)) };
        // SAFETY: as above.
        ptrs.iter().zip(lens).map(|(&p, &n)| unsafe { bytes(p, n) }).collect()
    };
    let result = caught(|| {
        let options: FontSetOptions = if opts.is_empty() { FontSetOptions::default() } else { request(opts)? };
        let mut builder = FontSet::builder().system_fonts(options.include_system_fonts).substitutions(options.substitutions);
        for font in data {
            builder = builder.font(font);
        }
        for file in options.font_files {
            builder = builder.font_file(file);
        }
        if let Some(family) = options.default_family {
            builder = builder.default_family(family);
        }
        builder.build()
    });
    let result = result.and_then(|(set, added)| {
        if !out.is_null() {
            // SAFETY: the caller's promise.
            unsafe { out.write(Box::into_raw(Box::new(set))) };
        }
        to_value(added)
    });
    // SAFETY: the caller's promise.
    unsafe { respond(faces, result) }
}

/// Frees a font set. Conversions running with it must have returned.
///
/// # Safety
/// `fonts` is null or from `twip_pdf_fonts_new`, and not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_fonts_free(fonts: *mut FontSet) {
    if !fonts.is_null() {
        // SAFETY: the caller's promise: `twip_pdf_fonts_new` boxed it.
        drop(unsafe { Box::from_raw(fonts) });
    }
}

/// The families in a font set, sorted; `out` gets `{"ok": [family]}`.
///
/// # Safety
/// `fonts` is from `twip_pdf_fonts_new` and not freed; `out` is null or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_fonts_families(fonts: *const FontSet, out: *mut TwipBuf) -> i32 {
    // SAFETY: the caller's promise.
    let result = match unsafe { fonts.as_ref() } {
        Some(f) => caught(|| to_value(f.families())),
        None => Err(Error::invalid("no font set")),
    };
    // SAFETY: the caller's promise.
    unsafe { respond(out, result) }
}

/// A cancellation flag, unset; free with `twip_pdf_cancel_free`.
#[unsafe(no_mangle)]
pub extern "C" fn twip_pdf_cancel_new() -> *mut AtomicBool {
    Box::into_raw(Box::new(AtomicBool::new(false)))
}

/// Sets a cancellation flag: the conversion using it stops soon after.
///
/// # Safety
/// `cancel` is null or from `twip_pdf_cancel_new` and not freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_cancel_set(cancel: *const AtomicBool) {
    // SAFETY: the caller's promise.
    if let Some(c) = unsafe { cancel.as_ref() } {
        c.store(true, Ordering::Relaxed);
    }
}

/// Frees a cancellation flag. The conversion using it must have returned.
///
/// # Safety
/// `cancel` is null or from `twip_pdf_cancel_new`, and not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_cancel_free(cancel: *mut AtomicBool) {
    if !cancel.is_null() {
        // SAFETY: the caller's promise: `twip_pdf_cancel_new` boxed it.
        drop(unsafe { Box::from_raw(cancel) });
    }
}

/// Converts a DOCX to PDF on the calling thread (`opts`: an `api::PdfOptions`; `cancel` may be null). The PDF goes
/// to `pdf`, raw; `meta` gets `{"ok": {warnings}}` or the error.
///
/// # Safety
/// `docx` and `opts` are null or valid for their lengths; `fonts` is from `twip_pdf_fonts_new` and not freed during
/// the call; `cancel` is null or from `twip_pdf_cancel_new` and not freed during the call; `pdf` and `meta` are null
/// or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twip_pdf_convert(
    docx: *const u8,
    len: usize,
    fonts: *const FontSet,
    opts: *const u8,
    olen: usize,
    cancel: *const AtomicBool,
    pdf: *mut TwipBuf,
    meta: *mut TwipBuf,
) -> i32 {
    // SAFETY: the caller's promise.
    let (docx, opts, fonts, cancel) = unsafe { (bytes(docx, len), bytes(opts, olen), fonts.as_ref(), cancel.as_ref()) };
    let result = caught(|| {
        let fonts = fonts.ok_or_else(|| Error::invalid("no font set"))?;
        let options: PdfOptions = if opts.is_empty() { PdfOptions::default() } else { request(opts)? };
        match cancel {
            Some(c) => engine::pdf::convert_cancellable(docx, fonts, &options, c),
            None => engine::pdf::convert(docx, fonts, &options),
        }
    });
    let result = result.map(|conversion| {
        // SAFETY: the caller's promise.
        unsafe { put(pdf, conversion.bytes) };
        json!({ "warnings": conversion.warnings })
    });
    // SAFETY: the caller's promise.
    unsafe { respond(meta, result) }
}
