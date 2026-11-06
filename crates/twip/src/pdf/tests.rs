use std::sync::atomic::AtomicBool;

use super::*;
use crate::api::engine::{Engine, SessionOptions};
use crate::api::error::ErrorCode;
use crate::api::fonts::FontLibrary;
use crate::api::session::ExportOptions;
use crate::font::Fonts;

/// `b` with what differs between two writes of the same document zeroed: the document id, the XMP uuids and the
/// creation and modification dates.
pub(crate) fn undated(mut b: Vec<u8>) -> Vec<u8> {
    let digits = |b: &[u8], at: usize, n: usize| b.get(at..at + n).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    // The id is random bytes, written as hex or as a literal string, whichever is shorter: it goes, to the trailer's end.
    if let Some(i) = b.windows(5).position(|w| w == b"/ID [")
        && let Some(k) = b[i..].windows(9).position(|w| w == b"startxref")
    {
        b.drain(i..i + k);
    }
    for i in 0..b.len() {
        if b[i..].starts_with(b"uuid:") && b.len() >= i + 41 {
            b[i + 5..i + 41].fill(b'0');
        }
        if b[i..].starts_with(b"D:") && digits(&b, i + 2, 14) {
            b[i + 2..i + 16].fill(b'0');
        }
        if digits(&b, i, 4) && b.get(i + 4) == Some(&b'-') && b.get(i + 10) == Some(&b'T') && digits(&b, i + 11, 2) {
            for k in [0, 5, 8, 11, 14, 17] {
                let n = if k == 0 { 4 } else { 2 };
                if digits(&b, i + k, n) {
                    b[i + k..i + k + n].fill(b'0');
                }
            }
        }
    }
    b
}

/// The repository's DOCX fixtures, by name.
pub(crate) fn fixtures() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("the fixtures folder exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "docx"))
        .map(|p| (p.file_name().expect("a file").to_string_lossy().into_owned(), std::fs::read(&p).expect("reads")))
        .collect();
    out.sort();
    out
}

#[test]
fn font_set_and_results_cross_threads() {
    const fn shareable<T: Send + Sync + Clone>() {}
    const fn sendable<T: Send>() {}
    shareable::<FontSet>();
    shareable::<PdfOptions>();
    sendable::<Conversion>();
}

#[test]
fn converts_like_the_editor_export() {
    let fonts = FontSet::system();
    let mut engine = Engine::new(FontLibrary::system());
    for (name, bytes) in fixtures() {
        let id = engine.create_session(SessionOptions::default());
        let s = engine.session(id).expect("a session");
        s.open(&bytes).expect("opens");
        let editor = s.export(&ExportOptions::Pdf(PdfOptions::default())).expect("exports");
        let converted = convert(&bytes, &fonts, &PdfOptions::default()).expect("converts");
        assert!(undated(converted.bytes) == undated(editor.bytes.0), "{name}: the converter's PDF differs from the editor's");
        engine.close_session(id);
    }
}

#[test]
fn cancel_stops_conversion() {
    let (_, bytes) = fixtures().into_iter().next().expect("a fixture");
    let err = convert_cancellable(&bytes, &FontSet::system(), &PdfOptions::default(), &AtomicBool::new(true)).expect_err("cancelled");
    assert_eq!(err.code, ErrorCode::Aborted);
}

#[test]
fn an_empty_font_set_fails_to_draw() {
    let (_, bytes) = fixtures().into_iter().next().expect("a fixture");
    let err = convert(&bytes, &FontSet::default(), &PdfOptions::default()).expect_err("no fonts");
    assert_eq!(err.code, ErrorCode::ExportFailed);
}

#[test]
fn converts_fixtures_in_parallel() {
    let fonts = FontSet::system();
    let fixtures = fixtures();
    let sequential: Vec<Vec<u8>> =
        fixtures.iter().map(|(_, b)| undated(convert(b, &fonts, &PdfOptions::default()).expect("converts").bytes)).collect();
    let parallel: Vec<Vec<u8>> = std::thread::scope(|s| {
        let jobs: Vec<_> = (0..8)
            .map(|t| {
                let (fonts, fixtures) = (&fonts, &fixtures);
                s.spawn(move || {
                    fixtures
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| i % 8 == t)
                        .map(|(i, (_, b))| (i, convert(b, fonts, &PdfOptions::default())))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<(usize, Vec<u8>)> =
            jobs.into_iter().flat_map(|j| j.join().expect("no panic")).map(|(i, r)| (i, undated(r.expect("converts").bytes))).collect();
        all.sort_by_key(|r| r.0);
        all.into_iter().map(|r| r.1).collect()
    });
    for ((name, _), (a, b)) in fixtures.iter().zip(sequential.iter().zip(&parallel)) {
        assert!(a == b, "{name}: a conversion on another thread differs");
    }
}

#[test]
fn malformed_inputs_fail_typed() {
    let fonts = FontSet::system();
    let (_, good) = fixtures().into_iter().next().expect("a fixture");
    let compound = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0, 0, 0];
    for (bytes, code) in [
        (&good[..good.len() / 2], ErrorCode::ImportFailed),
        (b"not a document at all".as_slice(), ErrorCode::ImportFailed),
        (&[][..], ErrorCode::ImportFailed),
        (&compound[..], ErrorCode::Unsupported),
    ] {
        assert_eq!(convert(bytes, &fonts, &PdfOptions::default()).expect_err("fails").code, code);
    }
}

#[test]
fn threads_share_font_bytes() {
    let fonts = FontSet::system();
    let (_, bytes) = fixtures().into_iter().find(|f| f.0 == "text-basics.docx").expect("the fixture");
    let probe = Fonts::with_catalog(std::sync::Arc::clone(&fonts.catalog));
    std::thread::scope(|s| s.spawn(|| convert(&bytes, &fonts, &PdfOptions::default()).expect("converts")).join().expect("no panic"));
    let after_one = probe.files_read();
    std::thread::scope(|s| s.spawn(|| convert(&bytes, &fonts, &PdfOptions::default()).expect("converts")).join().expect("no panic"));
    assert_eq!(probe.files_read(), after_one, "a second thread reads no font file again");
}

/// A page-independent memory check, run by hand: `cargo test -p twip --release -- --ignored repeated_conversions`.
#[test]
#[ignore = "10,000 conversions"]
fn repeated_conversions_hold_memory() {
    fn resident() -> Option<usize> {
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        Some(statm.split_whitespace().nth(1)?.parse::<usize>().ok()? * 4096)
    }
    let fonts = FontSet::system();
    let (_, bytes) = fixtures().into_iter().find(|f| f.0 == "text-basics.docx").expect("the fixture");
    for _ in 0..100 {
        convert(&bytes, &fonts, &PdfOptions::default()).expect("converts");
    }
    let Some(warm) = resident() else { return };
    for _ in 100..10_000 {
        convert(&bytes, &fonts, &PdfOptions::default()).expect("converts");
    }
    let end = resident().expect("still readable");
    assert!(end <= warm + warm / 10, "resident memory grew from {warm} to {end} bytes");
}

/// The Info dictionary's `/Keywords` string, each string passed through `f` first.
fn info_keywords(pdf: &[u8], f: &dyn Fn(&[u8]) -> Vec<u8>) -> Option<Vec<u8>> {
    use super::encrypt::{Tok, parse};
    let parsed = parse(pdf)?;
    parsed.objects.iter().find_map(|o| {
        let at = o.body.iter().position(|t| matches!(t, Tok::Name(n) if *n == b"/Keywords"))?;
        o.body[at..].iter().find_map(|t| if let Tok::Str(s) = t { Some(f(s)) } else { None })
    })
}

/// UTF-16BE text with its byte order mark, or bytes as they are.
fn text(b: &[u8]) -> String {
    match b {
        [0xFE, 0xFF, rest @ ..] => {
            String::from_utf16_lossy(&rest.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect::<Vec<_>>())
        }
        _ => String::from_utf8_lossy(b).into_owned(),
    }
}

#[test]
fn keywords_reach_the_info_dictionary_and_the_xmp() {
    let fonts = FontSet::system();
    let (_, bytes) = fixtures().into_iter().find(|f| f.0 == "text-basics.docx").expect("the fixture");
    let with = |keywords: &[&str]| {
        let options = PdfOptions { keywords: keywords.iter().map(|k| k.to_string()).collect(), ..PdfOptions::default() };
        convert(&bytes, &fonts, &options).expect("converts").bytes
    };
    let pdf = with(&["2026-CV-1042", "notice", "civil"]);
    assert_eq!(info_keywords(&pdf, &|s| s.to_vec()).map(|k| text(&k)).as_deref(), Some("2026-CV-1042, notice, civil"));
    let xmp = b"<pdf:Keywords>2026-CV-1042, notice, civil</pdf:Keywords>";
    assert!(pdf.windows(xmp.len()).any(|w| w == xmp), "the XMP holds the keywords");
    let pdf = with(&["  ", "a;b", "Δίκη"]);
    assert_eq!(info_keywords(&pdf, &|s| s.to_vec()).map(|k| text(&k)).as_deref(), Some("a;b, Δίκη"));
    assert_eq!(info_keywords(&with(&[]), &|s| s.to_vec()), None);
}

#[test]
fn a_protected_pdf_keeps_its_keywords_and_warns() {
    use super::encrypt::{Tok, aes_cbc_decrypt, file_key, parse};
    let fonts = FontSet::system();
    let (_, bytes) = fixtures().into_iter().find(|f| f.0 == "text-basics.docx").expect("the fixture");
    let options = PdfOptions { open_password: Some("secret".into()), keywords: vec!["sealed".into()] };
    let conversion = convert(&bytes, &fonts, &options).expect("converts");
    assert!(conversion.warnings.iter().any(|w| w.code == crate::api::warning::WarningCode::PdfArchivalClaimDropped));
    let parsed = parse(&conversion.bytes).expect("parses");
    let entry = |key: &[u8]| {
        parsed.objects.iter().find_map(|o| {
            let at = o.body.iter().position(|t| matches!(t, Tok::Name(n) if *n == key))?;
            o.body[at..].iter().find_map(|t| if let Tok::Str(s) = t { Some(s.clone()) } else { None })
        })
    };
    let key = file_key(&entry(b"/U").expect("/U"), &entry(b"/UE").expect("/UE"), "secret").expect("the password opens it");
    let keywords = info_keywords(&conversion.bytes, &|s| aes_cbc_decrypt(&key, s).unwrap_or_default());
    assert_eq!(keywords.map(|k| text(&k)).as_deref(), Some("sealed"));
    let empty = PdfOptions { open_password: Some(String::new()), ..PdfOptions::default() };
    assert_eq!(convert(&bytes, &fonts, &empty).expect_err("rejected").code, ErrorCode::InvalidArgument);
}
