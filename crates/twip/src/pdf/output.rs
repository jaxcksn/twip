//! PDF output snapshots

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use hayro_interpret::hayro_syntax::Pdf;
use hayro_interpret::hayro_syntax::object::{Array, Dict, Name, Object};
use hayro_interpret::hayro_syntax::page::Page;
use hayro_interpret::kurbo::BezPath;
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache, InterpreterSettings, RectExt, SoftMask,
    TransformExt, font::GlyphRun,
};

use super::tests::undated;
use crate::api::session::PdfOptions;
use crate::layout::tests::output::{bundled_fonts, corpus, entries, root};

/// How deep the structure tree and the outline are followed, and how many of their entries are written.
const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: usize = 100_000;
/// The pages whose drawing operators are written.
const OPERATOR_PAGES: usize = 2;

/// A number rounded to two decimals, without trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A PDF text string: UTF-16 with its byte order mark, else PDFDocEncoding read as Latin-1.
fn text(b: &[u8]) -> String {
    match b.strip_prefix(&[0xFE, 0xFF]) {
        Some(u) => char::decode_utf16(u.as_chunks::<2>().0.iter().map(|&c| u16::from_be_bytes(c))).map(|c| c.unwrap_or('\u{FFFD}')).collect(),
        None => b.iter().map(|&c| c as char).collect(),
    }
}

fn name(n: &Name<'_>) -> String {
    format!("/{}", n.as_str())
}

/// An operand: numbers rounded, strings as hex, arrays and dictionaries written out.
fn operand(o: &Object<'_>, out: &mut String, depth: usize) {
    match o {
        Object::Null(_) => out.push_str("null"),
        Object::Boolean(b) => write!(out, "{b}").unwrap(),
        Object::Number(n) => out.push_str(&num(n.as_f64())),
        Object::String(s) => {
            out.push('<');
            s.as_bytes().iter().for_each(|b| write!(out, "{b:02x}").unwrap());
            out.push('>');
        }
        Object::Name(n) => out.push_str(&name(n)),
        Object::Array(a) if depth < MAX_DEPTH => {
            out.push('[');
            for (i, o) in a.iter::<Object<'_>>().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                operand(&o, out, depth + 1);
            }
            out.push(']');
        }
        Object::Dict(d) if depth < MAX_DEPTH => {
            out.push_str("<<");
            for k in d.keys() {
                write!(out, " {} ", name(&k)).unwrap();
                if let Some(v) = d.get::<Object<'_>>(k.as_ref()) {
                    operand(&v, out, depth + 1);
                }
            }
            out.push_str(" >>");
        }
        Object::Array(_) | Object::Dict(_) => out.push_str("..."),
        Object::Stream(_) => out.push_str("stream"),
    }
}

/// The characters drawn, in content order: a marked span's ActualText in place of its glyphs, a line break where
/// the baseline moves.
#[derive(Default)]
struct TextDevice {
    out: String,
    y: Option<f64>,
    /// The open marked spans: whether each carries ActualText.
    spans: Vec<bool>,
    /// The outermost ActualText span's text, until its first glyph places it.
    pending: Option<String>,
    /// The last run filled: a run filled and stroked comes as both, and is read once.
    filled: Option<(usize, usize)>,
}

impl TextDevice {
    fn at(&mut self, y: f64) {
        if self.y.is_some_and(|last| (last - y).abs() > 0.5) {
            self.out.push('\n');
        }
        self.y = Some(y);
    }
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, _: DrawProps<'a>, mode: &DrawMode) {
        let key = Some((run.glyphs().as_ptr() as usize, run.glyphs().len()));
        if matches!(mode, DrawMode::Stroke(_)) && std::mem::take(&mut self.filled) == key {
            return;
        }
        self.filled = if matches!(mode, DrawMode::Fill(_)) { key } else { None };
        if self.spans.iter().any(|&a| a) {
            if let (Some(t), Some(g)) = (self.pending.take(), run.glyphs().first()) {
                self.at(g.transform().translation().y);
                self.out.push_str(&t);
            }
            return;
        }
        for g in run.glyphs() {
            self.at(g.transform().translation().y);
            match g.as_unicode() {
                Some(hayro_interpret::hayro_cmap::BfString::Char(c)) => self.out.push(c),
                Some(hayro_interpret::hayro_cmap::BfString::String(s)) => self.out.push_str(&s),
                None => self.out.push('\u{FFFD}'),
            }
        }
    }

    fn begin_marked_content(&mut self, _: &[u8], properties: Option<&Dict<'_>>) {
        let actual = properties.and_then(|p| p.get::<hayro_interpret::hayro_syntax::object::String<'_>>(b"ActualText"));
        if let Some(a) = &actual
            && !self.spans.iter().any(|&a| a)
        {
            self.pending = Some(text(a.as_bytes()));
        }
        self.spans.push(actual.is_some());
    }

    fn end_marked_content(&mut self) {
        self.spans.pop();
        if !self.spans.iter().any(|&a| a)
            && let Some(t) = self.pending.take()
        {
            self.out.push_str(&t);
        }
    }
}

/// A page's text, as [`TextDevice`] collects it.
fn page_text(page: &Page<'_>) -> String {
    let cache = InterpreterCache::new();
    let mut ctx = Context::new(
        page.initial_transform(true).to_kurbo(),
        page.intersected_crop_box().to_kurbo(),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut device = TextDevice::default();
    hayro_interpret::interpret_page(page, &mut ctx, &mut device);
    device.out
}

/// A structure element and its kids, one indented line each: its role, `Alt` and `ActualText`.
fn structure(e: &Dict<'_>, depth: usize, budget: &mut usize, out: &mut String) {
    if depth > MAX_DEPTH || *budget == 0 {
        return;
    }
    *budget -= 1;
    let role = e.get::<Name<'_>>(b"S").map_or_else(|| "?".into(), |n| n.as_str().to_owned());
    write!(out, "{}{role}", "  ".repeat(depth + 1)).unwrap();
    for key in ["Alt", "ActualText", "ID", "Lang"] {
        if let Some(s) = e.get::<hayro_interpret::hayro_syntax::object::String<'_>>(key.as_bytes()) {
            write!(out, " {key}={:?}", text(s.as_bytes())).unwrap();
        }
    }
    // Table attributes: one dictionary, or an array of them.
    let attributes: Vec<Dict<'_>> = match e.get::<Object<'_>>(b"A") {
        Some(Object::Dict(d)) => vec![d],
        Some(Object::Array(a)) => a.iter::<Object<'_>>().filter_map(Object::into_dict).collect(),
        _ => Vec::new(),
    };
    for a in attributes {
        for key in ["Scope", "Headers", "RowSpan", "ColSpan", "Summary", "ListNumbering"] {
            if let Some(v) = a.get::<Object<'_>>(key.as_bytes()) {
                let mut s = String::new();
                match &v {
                    Object::String(t) => write!(s, "{:?}", text(t.as_bytes())).unwrap(),
                    Object::Array(items) => {
                        let list: Vec<String> = items.iter::<Object<'_>>().filter_map(|o| o.into_string()).map(|t| text(t.as_bytes())).collect();
                        write!(s, "{list:?}").unwrap();
                    }
                    other => operand(other, &mut s, 0),
                }
                write!(out, " {key}={s}").unwrap();
            }
        }
    }
    out.push('\n');
    kids(e, depth + 1, budget, out);
}

/// The element kids under `e`'s `K` (marked-content ids and references aren't elements).
fn kids(e: &Dict<'_>, depth: usize, budget: &mut usize, out: &mut String) {
    let element = |d: &Dict<'_>| d.contains_key(b"S");
    if let Some(d) = e.get::<Dict<'_>>(b"K") {
        if element(&d) {
            structure(&d, depth, budget, out);
        }
    } else if let Some(a) = e.get::<Array<'_>>(b"K") {
        for d in a.iter::<Object<'_>>().filter_map(Object::into_dict).filter(element) {
            structure(&d, depth, budget, out);
        }
    }
}

/// An outline item and its siblings after it, their children indented.
fn outline(first: Option<Dict<'_>>, depth: usize, budget: &mut usize, out: &mut String) {
    let mut item = first;
    while let Some(i) = item {
        if depth > MAX_DEPTH || *budget == 0 {
            return;
        }
        *budget -= 1;
        let title = i.get::<hayro_interpret::hayro_syntax::object::String<'_>>(b"Title").map(|s| text(s.as_bytes())).unwrap_or_default();
        writeln!(out, "{}{title:?}", "  ".repeat(depth + 1)).unwrap();
        outline(i.get::<Dict<'_>>(b"First"), depth + 1, budget, out);
        item = i.get::<Dict<'_>>(b"Next");
    }
}

/// Each image a resource dictionary draws, by name (those of its forms after it, indented, to the bounds): its size
/// and a digest of its data and its soft mask's.
fn images(resources: Option<Dict<'_>>, depth: usize, budget: &mut usize, out: &mut String) {
    use hayro_interpret::hayro_syntax::object::Stream;
    use sha2::{Digest, Sha256};
    let Some(xobjects) = resources.and_then(|r| r.get::<Dict<'_>>(b"XObject")) else { return };
    if depth > MAX_DEPTH {
        return;
    }
    let mut keys: Vec<Name<'_>> = xobjects.keys().collect();
    keys.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    for key in keys {
        let Some(x) = xobjects.get::<Stream<'_>>(key.as_ref()).filter(|_| *budget > 0) else { continue };
        *budget -= 1;
        let d = x.dict();
        let indent = "  ".repeat(depth + 1);
        match d.get::<Name<'_>>(b"Subtype").as_ref().map(|n| n.as_str()) {
            Some("Image") => {
                let mut h = Sha256::new();
                for s in std::iter::once(x.clone()).chain(d.get::<Stream<'_>>(b"SMask")) {
                    h.update(s.decoded().unwrap_or_else(|_| s.raw_data()));
                }
                let digest: String = h.finalize()[..8].iter().map(|b| format!("{b:02x}")).collect();
                let size = |k: &[u8]| d.get::<u32>(k).unwrap_or_default();
                let mask = if d.contains_key(b"SMask") { " masked" } else { "" };
                writeln!(out, "{indent}image {} {}x{}{mask} {digest}", name(&key), size(b"Width"), size(b"Height")).unwrap();
            }
            Some("Form") => {
                writeln!(out, "{indent}form {}", name(&key)).unwrap();
                images(d.get::<Dict<'_>>(b"Resources"), depth + 1, budget, out);
            }
            _ => {}
        }
    }
}

/// What the snapshot records of a written PDF: the pages and their sizes, each page's text, the structure tree, the
/// outline, the annotations, the images, and the first pages' drawing operators.
pub(crate) fn dump(bytes: Vec<u8>) -> String {
    let pdf = Pdf::new(Arc::new(bytes)).expect("the PDF reads back");
    let mut s = String::new();
    let pages = pdf.pages();
    writeln!(s, "pages {}", pages.len()).unwrap();
    for (i, page) in pages.iter().enumerate() {
        let (w, h) = page.base_dimensions();
        writeln!(s, "page {} {} {}", i + 1, num(f64::from(w)), num(f64::from(h))).unwrap();
        for line in page_text(page).lines() {
            writeln!(s, "  text {line}").unwrap();
        }
        if let Some(annots) = page.raw().get::<Array<'_>>(b"Annots") {
            for a in annots.iter::<Dict<'_>>() {
                let mut line = String::new();
                for key in ["Subtype", "Rect", "Contents", "A"] {
                    if let Some(v) = a.get::<Object<'_>>(key.as_bytes()) {
                        write!(line, " {key} ").unwrap();
                        operand(&v, &mut line, 0);
                    }
                }
                writeln!(s, "  annot{line}").unwrap();
            }
        }
        images(page.raw().get::<Dict<'_>>(b"Resources"), 0, &mut MAX_ENTRIES.clone(), &mut s);
        if i < OPERATOR_PAGES {
            let mut ops = page.operations();
            while let Some(op) = ops.next() {
                let mut line = String::new();
                for o in op.operands() {
                    operand(o, &mut line, 0);
                    line.push(' ');
                }
                writeln!(s, "  op {line}{}", op.operator).unwrap();
            }
        }
    }
    let catalog = pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()).expect("a catalog");
    s.push_str("structure\n");
    if let Some(root) = catalog.get::<Dict<'_>>(b"StructTreeRoot") {
        kids(&root, 0, &mut MAX_ENTRIES.clone(), &mut s);
    }
    s.push_str("outline\n");
    outline(catalog.get::<Dict<'_>>(b"Outlines").and_then(|o| o.get::<Dict<'_>>(b"First")), 0, &mut MAX_ENTRIES.clone(), &mut s);
    s
}

/// Every corpus document exported as PDF (with the gate's fonts) equals its committed dump in `tests/pdf`.
/// `TWIP_WRITE_PDF=1` rewrites them.
#[test]
fn pdf_output_matches_snapshots() {
    let (dir, write) = (root().join("pdf"), std::env::var_os("TWIP_WRITE_PDF").is_some());
    let fonts = bundled_fonts();
    let (mut got, hyphenation) = (BTreeMap::new(), crate::layout::tests::output::hyphenation());
    for (name, d) in corpus() {
        let snap = crate::layout::tests::layout_hyphenated(&d, &fonts, Some(&hyphenation));
        let (bytes, _) =
            super::export::export_with(&d, &crate::layout::tests::view(&d), &snap, "", &PdfOptions::default(), None, Some(&fonts)).expect("exports");
        got.insert(format!("{name}.txt"), dump(undated(bytes)));
    }
    assert!(!got.is_empty(), "no fixtures read from {}", root().join("fixtures").display());
    if write {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in &got {
            std::fs::write(dir.join(name), text).unwrap();
        }
        return;
    }
    let mut failures = Vec::new();
    for (name, text) in &got {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(want) if want.replace("\r\n", "\n") == *text => {}
            Ok(want) => {
                let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), text.lines().collect());
                let at = w.iter().zip(&g).position(|(a, b)| a != b).unwrap_or(w.len().min(g.len()));
                let (a, b) = (w.get(at).copied().unwrap_or("<end>"), g.get(at).copied().unwrap_or("<end>"));
                failures.push(format!("{name}: first change at line {}:\n    - {a}\n    + {b}", at + 1));
            }
            Err(_) => failures.push(format!("{name}: no snapshot")),
        }
    }
    for e in entries(&dir) {
        let name = e.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if !got.contains_key(&name) {
            failures.push(format!("{name}: snapshot without a fixture"));
        }
    }
    assert!(
        failures.is_empty(),
        "PDF output changed for {} fixture(s). If intended, run `TWIP_WRITE_PDF=1 cargo nextest run -p twip --lib pdf_output` and commit tests/pdf:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn a_cyclic_outline_and_structure_tree_stop_at_their_bounds() {
    let pdf = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /Outlines 4 0 R /StructTreeRoot 6 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >> endobj
4 0 obj << /First 5 0 R >> endobj
5 0 obj << /Title (loop) /Next 5 0 R /First 5 0 R >> endobj
6 0 obj << /K 7 0 R >> endobj
7 0 obj << /S /P /K [7 0 R 7 0 R] >> endobj
trailer << /Root 1 0 R >>
%%EOF
";
    let s = dump(pdf.to_vec());
    let structure = s.lines().filter(|l| l.trim() == "P").count();
    let outline = s.lines().filter(|l| l.trim() == "\"loop\"").count();
    assert!(structure > 0 && structure <= MAX_ENTRIES, "{structure} structure lines");
    assert!(outline > 0 && outline <= MAX_ENTRIES, "{outline} outline lines");
}
