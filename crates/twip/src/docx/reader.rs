//! DOCX package → [`Document`]: resolves styles (STYLE), numbering (LIST) and fields (FIELD) while reading.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::rc::Rc;

use roxmltree::Node;

use super::diagnostics::{Diagnostics, WarningCode};
use super::model::*;
use super::numbering::{Counters, Numbering};
use super::props::*;
use super::resolve::{ParaResolved, Resolver, div_chain};
use super::review::{
    AuthorId, CellMarkKind, CellRevisions, Comment, CommentId, FormatChange, GridChange, IdCounter, Identity, MarkKind, ParaChange, People,
    PreviousProps, Protection, ProtectionKind, RangeKind, RangeMark, RevisionId, RevisionMark, RowRevisions, RunRevisions, SectionChange, ShowMarkup,
    TableChange, Timestamp,
};
use super::styles::{Region, Styles, ThemeFonts};
use super::xml::*;

struct Package {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
    /// Bytes the reads may still decompress, across every part: a small package can inflate to gigabytes.
    budget: u64,
}

/// The most bytes of HTML one `w:altChunk` imports.
const MAX_ALT_CHUNK: usize = 8 << 20;
/// The most bytes of HTML a document's `w:altChunk`s import in all: each is parsed and laid out as content.
const MAX_ALT_CHUNKS: usize = 32 << 20;

/// The most `w:altChunk`s a document imports: past it, each is kept as read.
const MAX_ALT_CHUNK_COUNT: usize = 10_000;

/// The most bytes a document's `w:altChunk` documents import in all, each import counting what reading its package
/// decompressed: each is copied in as content where it stands.
const MAX_ALT_DOCS: u64 = 256 << 20;

/// How many `w:altChunk` documents deep a document imports: one may import itself (a zip holding itself).
// ponytail: a safety bound; raise it if a real document nests deeper.
const MAX_ALT_DEPTH: u8 = 8;

/// HTML bytes as text: UTF-16 when a byte order mark says so, Windows-1252 when its `<meta>` declares a Latin-1
/// charset, else UTF-8 (a bad sequence replaced).
fn decode_html(data: &[u8]) -> String {
    let utf16 = |le: bool| {
        let units = data[2..].as_chunks::<2>().0.iter().map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) });
        char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    };
    match data {
        [0xFF, 0xFE, ..] => utf16(true),
        [0xFE, 0xFF, ..] => utf16(false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ if latin1_declared(data) => data.iter().map(|&b| windows_1252(b)).collect(),
        _ => String::from_utf8_lossy(data).into_owned(),
    }
}

/// Whether the head of HTML `data` declares a charset HTML reads as Windows-1252 (`windows-1252`, `iso-8859-1`,
/// `latin1`, `us-ascii`).
fn latin1_declared(data: &[u8]) -> bool {
    let mut rest = &data[..data.len().min(4096)];
    while let Some(at) = rest.windows(7).position(|w| w.eq_ignore_ascii_case(b"charset")) {
        rest = rest[at + 7..].trim_ascii_start();
        let Some(value) = rest.strip_prefix(b"=") else { continue };
        let name: Vec<u8> = value
            .iter()
            .skip_while(|b| matches!(b, b'"' | b'\'') || b.is_ascii_whitespace())
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            .map(u8::to_ascii_lowercase)
            .collect();
        return matches!(name.as_slice(), b"windows-1252" | b"iso-8859-1" | b"latin1" | b"cp1252" | b"us-ascii" | b"ascii");
    }
    false
}

/// Plain text bytes as text: as a byte order mark says, else UTF-8 when they are, else Windows-1252.
fn decode_text(data: &[u8]) -> String {
    match data {
        [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] | [0xEF, 0xBB, 0xBF, ..] => decode_html(data),
        _ => match std::str::from_utf8(data) {
            Ok(s) => s.to_string(),
            Err(_) => data.iter().map(|&b| windows_1252(b)).collect(),
        },
    }
}

/// A Windows-1252 byte as its character.
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}',
        '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// What reading a package may decompress in all.
// ponytail: one fixed budget; make it an open option if hosts load documents bigger than this.
const PACKAGE_BUDGET: u64 = 2 << 30;

/// The grid columns a row's `gridSpan`s, `gridBefore` and `gridAfter` may add past one a cell: the values come from
/// the file, and every column is layout work and memory, so `gridSpan="2000000000"` would be gigabytes.
// ponytail: a safety bound well past Word's 63 columns; raise it if a real table needs more.
const MAX_SPANNED: usize = 1024;

fn bound_spans(r: &mut Row) {
    let mut room = MAX_SPANNED;
    let mut take = |v: usize, floor: usize| {
        let extra = v.saturating_sub(floor).min(room);
        room -= extra;
        floor + extra
    };
    r.grid_before = take(r.grid_before, 0);
    for c in &mut r.cells {
        c.span = take(c.span, 1);
    }
    r.grid_after = take(r.grid_after, 0);
}

impl Package {
    /// A part's bytes: none when it is missing, unreadable, larger than [`super::package::MAX_PART`] or past the
    /// package's budget.
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        self.read_at_most(path, super::package::MAX_PART)
    }

    /// A part's bytes, as [`Package::read`] reads them, none when it is larger than `max`.
    fn read_at_most(&mut self, path: &str, max: u64) -> Option<Vec<u8>> {
        let f = self.zip.by_name(path.trim_start_matches('/')).ok()?;
        let limit = max.min(super::package::MAX_PART).min(self.budget);
        let mut out = Vec::new();
        // One byte past the limit tells a part that is too large from one that just fits; what was decompressed is
        // spent, read in full or not.
        let read = f.take(limit + 1).read_to_end(&mut out);
        let n = out.len() as u64;
        self.budget = self.budget.saturating_sub(n);
        (read.is_ok() && n <= limit).then_some(out)
    }

    /// A part's text, refused (none) when its elements nest deeper than [`MAX_DEPTH`].
    fn read_str(&mut self, path: &str) -> Option<String> {
        let b = self.read(path)?;
        let mut s = String::from_utf8(b).ok()?;
        let bom = s.len() - s.trim_start_matches('\u{feff}').len();
        s.drain(..bom);
        (!too_deep(&s, MAX_DEPTH)).then_some(s)
    }

    /// Relationships of a part: id → (type suffix, absolute target).
    fn rels(&mut self, part: &str) -> HashMap<String, (String, String)> {
        let (dir, name) = part.rsplit_once('/').unwrap_or(("", part));
        let path = if dir.is_empty() { format!("_rels/{name}.rels") } else { format!("{dir}/_rels/{name}.rels") };
        let mut out = HashMap::new();
        let Some(xml) = self.read_str(&path) else { return out };
        let Ok(doc) = roxmltree::Document::parse(&xml) else { return out };
        for r in doc.root_element().children().filter(|c| c.is_element()) {
            let (Some(id), Some(target)) = (r.attribute("Id"), r.attribute("Target")) else { continue };
            let kind = r.attribute("Type").unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            let external = r.attribute("TargetMode") == Some("External");
            let abs = if external { target.to_string() } else { resolve_path(dir, target) };
            out.insert(id.to_string(), (kind, abs));
        }
        out
    }
}

fn resolve_path(dir: &str, target: &str) -> String {
    if let Some(t) = target.strip_prefix('/') {
        return t.to_string();
    }
    let mut parts: Vec<&str> = if dir.is_empty() { Vec::new() } else { dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

pub fn read(bytes: &[u8]) -> Result<Document, String> {
    read_with(bytes, false, now_local())
}

/// As [`read`], with `DATE`/`TIME` fields and date blocks computed at `now` (seconds since 1970, local time) instead
/// of the system clock, so the layout is the same on every run.
pub fn read_at(bytes: &[u8], now: i64) -> Result<Document, String> {
    read_with(bytes, false, now)
}

/// Seconds since 1970, UTC, by the system clock.
pub fn now_utc() -> i64 {
    crate::clock::unix_secs()
}

/// Seconds since 1970 by the system clock in the local time zone: the wall-clock time dates and times show.
pub fn now_local() -> i64 {
    let t = now_utc();
    t + local_offset(t)
}

/// The local time zone's offset from UTC at `t` (seconds since 1970, UTC), in seconds; 0 without a time zone. In the
/// browser, the page's.
pub(crate) fn local_offset(t: i64) -> i64 {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        crate::clock::local_offset(t)
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        native_offset(t)
    }
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn native_offset(t: i64) -> i64 {
    // The fields `struct tm` starts with on every platform, with room for the rest.
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        rest: [i64; 4],
    }
    let mut tm = Tm::default();
    #[cfg(windows)]
    // SAFETY: `_localtime64_s` writes one `struct tm` (nine ints, within `Tm`) and reads one 64-bit time.
    let ok = unsafe {
        unsafe extern "C" {
            fn _localtime64_s(tm: *mut Tm, t: *const i64) -> i32;
        }
        _localtime64_s(&mut tm, &t) == 0
    };
    #[cfg(all(unix, target_pointer_width = "64"))]
    // SAFETY: `localtime_r` writes one `struct tm` (nine ints, a long and a pointer, within `Tm`) and reads one time_t.
    let ok = unsafe {
        unsafe extern "C" {
            fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
        }
        !localtime_r(&t, &mut tm).is_null()
    };
    #[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
    let ok = false;
    if !ok {
        return 0;
    }
    let days = super::review::days_from_civil(i64::from(tm.year) + 1900, i64::from(tm.mon) + 1, i64::from(tm.mday));
    days * 86_400 + i64::from(tm.hour) * 3600 + i64::from(tm.min) * 60 + i64::from(tm.sec) - t
}

/// As [`read`], keeping what saving needs: the package, the main part's XML, where each body paragraph and table came
/// from ([`Paragraph::src`], [`Table::src`]) and each run's direct formatting ([`RunStyle::direct`]).
pub fn read_editable(bytes: &[u8]) -> Result<Document, String> {
    read_with(bytes, true, now_local())
}

/// The XML text of an element as written, with the namespaces declared between it and the root declared on it (so it
/// parses with the part's root namespaces wherever it is written).
fn raw(n: Node) -> Rc<str> {
    let text = &n.document().input_text()[n.range()];
    let root = n.document().root_element();
    if n.namespaces().len() <= root.namespaces().len() {
        return text.into();
    }
    let own = |p: &str| text.split('>').next().is_some_and(|tag| tag.contains(&format!("xmlns:{p}=")));
    let extra: String = n
        .namespaces()
        .filter(|ns| !root.namespaces().any(|r| r.name() == ns.name() && r.uri() == ns.uri()))
        .filter_map(|ns| match ns.name() {
            Some(p) if !own(p) => Some(format!(" xmlns:{p}=\"{}\"", esc_attr(ns.uri()))),
            // The default namespace, which the element itself or an ancestor below the root declares.
            None if !text.split('>').next().is_some_and(|tag| tag.contains("xmlns=")) => Some(format!(" xmlns=\"{}\"", esc_attr(ns.uri()))),
            _ => None,
        })
        .collect();
    let at = text.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(text.len());
    format!("{}{extra}{}", &text[..at], &text[at..]).into()
}

fn esc_attr(v: &str) -> String {
    v.replace('&', "&amp;").replace('"', "&quot;")
}

/// The break `w:br@type` and `@clear` give.
fn break_kind(kind: Option<&str>, clear: Option<&str>) -> BreakKind {
    match (kind, clear) {
        (Some("page"), _) => BreakKind::Page,
        (Some("column"), _) => BreakKind::Column,
        (_, Some("left")) => BreakKind::ClearLeft,
        (_, Some("right")) => BreakKind::ClearRight,
        (_, Some("all")) => BreakKind::ClearAll,
        _ => BreakKind::Line,
    }
}

/// The run line numbers draw with: the document defaults and the LineNumber character style.
pub(crate) fn line_number_run(res: &Resolver) -> super::model::Run {
    let mut rp = res.styles.doc_rpr.clone();
    if res.styles.get("LineNumber").is_some() {
        rp.merge_level(&res.styles.chain_rpr(Some("LineNumber")));
    }
    res.run_style(&rp)
}

fn read_with(bytes: &[u8], editable: bool, now: i64) -> Result<Document, String> {
    read_nested(bytes, editable, now, &mut PACKAGE_BUDGET.clone(), 0)
}

/// A `w:altChunk`'s document `target` read from `pkg`, with what reading it decompressed: refused past `left` bytes.
fn read_alt_doc(pkg: &mut Package, target: &str, left: u64, now: i64, depth: u8) -> Result<(Rc<Document>, u64), &'static str> {
    if left == 0 {
        return Err("too much to import");
    }
    // Read from a budget one byte past what is left: a read stopped by it has spent it all, and is refused.
    let cap = pkg.budget.min(left + 1);
    let rest = pkg.budget - cap;
    pkg.budget = cap;
    let doc = pkg.read_at_most(target, super::package::MAX_PART).and_then(|bytes| read_nested(&bytes, false, now, &mut pkg.budget, depth).ok());
    let cost = cap - pkg.budget;
    pkg.budget += rest;
    match doc {
        _ if cost > left => Err("too much to import"),
        Some(doc) => Ok((Rc::new(doc), cost)),
        None => Err("missing or not a readable document"),
    }
}

/// [`read_with`] for a package `depth` documents in (a `w:altChunk`'s), its reads taken from `budget`.
fn read_nested(bytes: &[u8], editable: bool, now: i64, budget: &mut u64, depth: u8) -> Result<Document, String> {
    let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|e| e.to_string())?;
    let mut pkg = Package { zip, budget: *budget };
    let doc = read_package(bytes, &mut pkg, editable, now, depth);
    *budget = pkg.budget;
    doc
}

fn read_package(bytes: &[u8], pkg: &mut Package, editable: bool, now: i64, depth: u8) -> Result<Document, String> {
    let root_rels = pkg.rels("");
    let main = root_rels.values().find(|(k, _)| k == "officeDocument").map(|(_, t)| t.clone()).unwrap_or_else(|| "word/document.xml".into());
    let rels = pkg.rels(&main);
    let part_of = |kind: &str| rels.values().find(|(k, _)| k == kind).map(|(_, t)| t.clone());

    let theme_xml = part_of("theme").and_then(|p| pkg.read_str(&p));
    let theme = ThemeFonts::parse(theme_xml.as_deref());
    let drawing_theme = Rc::new(crate::drawing::theme::Theme::parse(theme_xml.as_deref()));
    let settings_xml = part_of("settings").and_then(|p| pkg.read_str(&p));
    let settings = read_settings(settings_xml.as_deref());
    let settings_tree = settings_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
    let settings_root = settings_tree.as_ref().map(|d| d.root_element());
    let colour_map = crate::drawing::theme::ColourMap::parse(settings_root);
    let opening = settings_root.map(opening_view);
    let colours = ThemeColours::new(&drawing_theme, &colour_map);
    let mut styles = Styles::parse(part_of("styles").and_then(|p| pkg.read_str(&p)).as_deref(), colours);
    let mut numbering = Numbering::parse(part_of("numbering").and_then(|p| pkg.read_str(&p)).as_deref(), &colours);
    // The glossary's styles and lists join the document's tables before anything resolves with them.
    let glossary_part = part_of("glossaryDocument");
    let glossary_map = glossary_part.as_deref().map(|path| {
        let grels = pkg.rels(path);
        let mut gpart = |kind: &str| grels.values().find(|(k, _)| k == kind).and_then(|(_, t)| pkg.read_str(t));
        let gstyles = gpart("styles").map_or_else(Styles::default, |x| Styles::parse(Some(&x), colours));
        let gnumbering = Numbering::parse(gpart("numbering").as_deref(), &colours);
        super::glossary::merge_tables(&mut styles, &mut numbering, gstyles, gnumbering)
    });
    styles.compat = settings.compat();
    styles.word2002_rules = styles.compat == Compat::Legacy && settings.policy.use_word2002_table_style_rules;
    styles.open_type = styles.compat == Compat::Generic || settings.policy.enable_open_type_features;
    styles.cell_size_rules = styles.compat == Compat::Legacy && !settings.policy.override_table_style_font_size_and_justification;
    let xml = pkg.read_str(&main).ok_or("no main document part, or one too large or nested too deeply")?;

    // New revision, comment and paragraph ids go past every id in the parts read, so ids are seeded before reading.
    let mut ids = IdCounter::default();
    seed_ids(&xml, &mut ids);
    let id_parts =
        ["header", "footer", "footnotes", "endnotes", "comments", "commentsExtended", "commentsIds", "commentsExtensible", "glossaryDocument"];
    for (_, target) in rels.values().filter(|(k, _)| id_parts.contains(&k.as_str())) {
        if let Some(x) = pkg.read_str(target) {
            seed_ids(&x, &mut ids);
        }
    }
    // The comments part is parsed first so references to missing comments are known while the body is read; the
    // bodies are read after the document's other stories (their lists number after the body's).
    let comments_part = part_of("comments");
    let comments_xml = comments_part.as_deref().and_then(|p| pkg.read_str(p));
    let comments_dom = comments_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
    let comment_ids = comments_dom.iter().flat_map(|d| children(d.root_element(), "comment")).filter_map(|c| int(attr(c, "id"))).collect();
    let people_xml = part_of("people").and_then(|p| pkg.read_str(&p));
    let people_part = super::comments::people(people_xml.as_deref());

    let font_descriptions = part_of("fontTable").and_then(|p| pkg.read_str(&p)).map(|x| super::font_table::parse(&x)).unwrap_or_default();
    let mut res = Resolver::new(styles, numbering, theme);
    res.categories = Rc::new(super::font_table::categories(&font_descriptions));
    res.divisions = Rc::new(read_divs(part_of("webSettings").and_then(|p| pkg.read_str(&p)).as_deref(), &colours));
    let numbering_part = part_of("numbering");
    let mut r = Reader {
        res,
        settings,
        counters: Counters::default(),
        images: Vec::new(),
        image_index: HashMap::new(),
        bookmarks: HashMap::new(),
        fields: Vec::new(),
        para_no: 0,
        para_count: 0,
        float_order: 0,
        pending_hidden: None,
        pending_spec: None,
        uncounted: false,
        part: main.clone(),
        rels: rels.clone(),
        note_id: 0,
        in_endnotes: false,
        record: false,
        segs: Vec::new(),
        simple: Vec::new(),
        lossy: false,
        controls: HashMap::new(),
        bound_runs: HashMap::new(),
        people: People::default(),
        identities: HashMap::new(),
        ids,
        diagnostics: Diagnostics::default(),
        wrap: RunRevisions::default(),
        open_ranges: Vec::new(),
        moves: Vec::new(),
        pending_marks: Vec::new(),
        comment_ids,
        anchored: HashSet::new(),
        orphans: HashSet::new(),
        glossary_comments: None,
        alt_chunks: Vec::new(),
        alt_bytes: 0,
        alt_parts: HashMap::new(),
        alt_docs: Vec::new(),
        alt_depth: depth,
        theme: drawing_theme.clone(),
    };
    // The picture bullets' images, from the numbering part's relationships.
    if let Some(part) = &numbering_part {
        let rels = pkg.rels(part);
        let wanted: Vec<(i64, String)> = r.res.numbering.pic_bullets.iter().filter_map(|(id, p)| Some((*id, p.rel.clone()?))).collect();
        for (id, rel) in wanted {
            let Some((_, target)) = rels.get(&rel).cloned() else { continue };
            let index = match r.image_index.get(&target) {
                Some(&i) => i,
                None => {
                    let Some(data) = pkg.read(&target) else { continue };
                    if super::image::image_type(&data).is_none() {
                        r.diagnostics
                            .warn(WarningCode::PictureBulletUndecodable, Some(part), || format!("{target} isn't an image format the engine draws"));
                    }
                    r.images.push(data);
                    r.image_index.insert(target, r.images.len() - 1);
                    r.images.len() - 1
                }
            };
            if let Some(p) = r.res.numbering.pic_bullets.get_mut(&id) {
                p.image = Some(index);
            }
        }
    }
    for (name, identity) in &people_part {
        if let Some(i) = identity {
            r.identities.entry(name.clone()).or_insert_with(|| i.clone());
        }
    }

    let mut doc = Document::default();
    {
        let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
        let body = child(dom.root_element(), "body").ok_or("no body")?;
        if let Some(b) = child(dom.root_element(), "background") {
            doc.page_colour = colours.colour(b, "color", "themeColor", "themeTint", "themeShade");
            doc.page_colour_theme = crate::docx::props::theme_ref(b, "themeColor", "themeTint", "themeShade");
            let mut src = DrawingSource { reader: &mut r, pkg: &mut *pkg };
            doc.page_fill = crate::drawing::vml::background_fill(b, doc.page_colour, &mut |rid| crate::drawing::read::Source::image(&mut src, rid));
        }
        r.record = editable;
        doc.sections = r.read_body(body, &mut *pkg);
        shift_connector_text(&mut doc.sections);
        r.record = false;
    }

    // Headers and footers are keyed by part path.
    let mut wanted: Vec<String> = Vec::new();
    for s in &doc.sections {
        for p in s.headers.iter().chain(s.footers.iter()).flatten() {
            if !wanted.contains(p) {
                wanted.push(p.clone());
            }
        }
    }
    // Editable reads keep each header's and footer's XML and where its paragraphs came from, as the body's.
    let mut story_xml = HashMap::new();
    for path in wanted {
        r.record = editable;
        if let Some(blocks) = r.read_story(&path, &mut *pkg, |root| Some(root)) {
            if editable {
                story_xml.insert(path.clone(), pkg.read_str(&path).unwrap_or_default());
            }
            doc.stories.insert(path, blocks);
        }
        r.record = false;
    }
    let mut note_parts = Vec::new();
    for (kind, tag, map) in [("footnotes", "footnote", &mut doc.footnotes), ("endnotes", "endnote", &mut doc.endnotes)] {
        let Some(path) = part_of(kind) else { continue };
        let Some(xml) = pkg.read_str(&path) else { continue };
        let Ok(dom) = roxmltree::Document::parse(&xml) else { continue };
        r.enter_part(&path, &mut *pkg);
        // Editable reads keep where each note's paragraphs came from, as the body's.
        r.record = editable;
        if editable {
            note_parts.push((path.clone(), xml.clone(), tag == "footnote"));
        }
        for n in children(dom.root_element(), if tag == "footnote" { "footnote" } else { "endnote" }) {
            let id = int(attr(n, "id")).unwrap_or(0);
            r.note_id = id;
            r.in_endnotes = tag == "endnote";
            let separator = attr(n, "type").filter(|t| matches!(*t, "separator" | "continuationSeparator" | "continuationNotice"));
            let mut blocks = match separator {
                Some(_) => r.read_blocks(n, &mut *pkg, None),
                None => r.read_story_blocks(n, &mut *pkg),
            };
            r.close_ranges(&mut blocks);
            note_floats(&mut blocks);
            match separator {
                // A separator story counts only when the settings list it: else the built-in one is used.
                Some(t) => {
                    if r.settings.listed_notes.contains(&format!("{tag}:{id}")) {
                        doc.separators.insert(format!("{tag}:{t}"), blocks);
                    }
                }
                None => {
                    map.insert(id, blocks);
                }
            }
        }
        r.record = false;
    }
    // Word's built-in separators: one line with no paragraph spacing.
    // ponytail: in the default paragraph style's run font, not the document default's; differs only when that style
    // sets a font or size.
    let mut separator_defaults = HashMap::new();
    for (tag, t) in [("footnote", "separator"), ("footnote", "continuationSeparator"), ("endnote", "separator"), ("endnote", "continuationSeparator")]
    {
        let key = format!("{tag}:{t}");
        let used =
            !(doc.separators.contains_key(&key) || (tag == "endnote" && doc.endnotes.is_empty()) || (tag == "footnote" && doc.footnotes.is_empty()));
        if !used && !editable {
            continue;
        }
        let xml = format!(
            r#"<w:{tag} xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/><w:ind w:left="0" w:right="0" w:firstLine="0"/><w:jc w:val="left"/></w:pPr><w:r><w:{t}/></w:r></w:p></w:{tag}>"#
        );
        if let Ok(dom) = roxmltree::Document::parse(&xml) {
            r.in_endnotes = tag == "endnote";
            let blocks = r.read_blocks(dom.root_element(), &mut *pkg, None);
            if used {
                doc.separators.insert(key.clone(), blocks.clone());
            }
            separator_defaults.insert(key, blocks);
        }
    }
    if let (Some(path), Some(dom)) = (&comments_part, &comments_dom) {
        r.enter_part(path, &mut *pkg);
        r.record = editable;
        let read = r.read_comments(dom.root_element(), &mut *pkg);
        r.record = false;
        let mut part = |kind: &str| part_of(kind).and_then(|p| pkg.read_str(&p));
        let (extended, ids, extensible) = (part("commentsExtended"), part("commentsIds"), part("commentsExtensible"));
        doc.comments = super::comments::link(read, extended.as_deref(), ids.as_deref(), extensible.as_deref());
        for c in doc.comments.by_id.values() {
            r.ids.saw_para(c.para_id);
            if let Some(d) = c.durable_id {
                r.ids.saw_para(d);
            }
        }
        for c in doc.comments.by_id.keys().filter(|c| !r.anchored.contains(&c.0)) {
            r.diagnostics.warn(WarningCode::CommentUnanchored, Some(path), || format!("comment {} has no range or reference", c.0));
        }
    }
    // The glossary's building blocks, read with their bodies' ids mapped onto the document's tables.
    let mut glossary_src = None;
    if let (Some(path), Some(map)) = (&glossary_part, glossary_map) {
        // The bodies are read from the part with their style and list ids as the document's tables hold them, and
        // saving maps them back: the model has one set of ids.
        let xml = pkg.read_str(path).map(|x| map.to_view_xml(&x).into_owned());
        match xml.as_deref().map(roxmltree::Document::parse) {
            Some(Ok(dom)) => {
                r.enter_part(path, &mut *pkg);
                let comments = r.rels.values().find(|(k, _)| k == "comments").and_then(|(_, t)| pkg.read_str(t));
                let ids = comments.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
                r.glossary_comments =
                    Some(ids.iter().flat_map(|d| children(d.root_element(), "comment")).filter_map(|c| int(attr(c, "id"))).collect());
                r.record = editable;
                let read = r.read_glossary(dom.root_element(), &mut *pkg);
                r.record = false;
                r.glossary_comments = None;
                if editable {
                    let parts = read.iter().map(|(b, range)| (range.clone(), b.body.clone())).collect();
                    glossary_src = Some(GlossarySource { part: path.clone(), xml: xml.clone().unwrap_or_default(), parts });
                }
                doc.glossary = Some(Glossary { parts: read.into_iter().map(|(b, _)| b).collect(), map });
            }
            _ => r.diagnostics.warn(WarningCode::GlossaryUnreadable, Some(path), || "the glossary part isn't well-formed XML".into()),
        }
    }
    // People the people part names but no revision or comment does are kept after those that do.
    for (name, identity) in &people_part {
        r.people.author(name, identity.as_ref());
    }
    let refused = r.read_alt_docs(pkg, now);
    if !refused.is_empty() {
        r.record = editable;
        let last = doc.sections.len().saturating_sub(1);
        for (k, s) in doc.sections.iter_mut().enumerate() {
            r.drop_paras(&mut s.blocks, &refused, k == last);
        }
        for b in doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()) {
            r.drop_paras(b, &refused, true);
        }
        r.record = false;
    }
    doc.line_number_run = Some(line_number_run(&r.res));
    let mut image_parts = vec![String::new(); r.images.len()];
    for (part, &i) in &r.image_index {
        image_parts[i] = part.clone();
    }
    doc.images = r.images;
    doc.alt_chunks = std::mem::take(&mut r.alt_chunks);
    doc.controls = std::mem::take(&mut r.controls);
    doc.data_parts = data_parts(&mut *pkg, &rels);
    // Bound controls show their data.
    if !doc.data_parts.is_empty() {
        let (controls, parts) = (&mut doc.controls, super::binding::Parts::new(&doc.data_parts));
        let runs = &r.bound_runs;
        let resolve = |id: ControlId| runs.get(&id).cloned();
        let mut show = |p: &mut Paragraph| {
            super::binding::refresh(p, controls, &parts, &resolve);
        };
        for s in &mut doc.sections {
            super::fields::paras(&mut s.blocks, &mut show);
        }
        for b in doc.stories.values_mut() {
            super::fields::paras(b, &mut show);
        }
    }
    doc.theme = drawing_theme;
    doc.colour_map = colour_map;
    doc.settings = r.settings;
    if let Some((show, hides_white_space)) = opening {
        doc.review.markup = Rc::new(super::review::Markup { show, ..Default::default() });
        doc.hides_white_space = hides_white_space;
    }
    doc.people = r.people;
    doc.review_ids = r.ids;
    doc.diagnostics = r.diagnostics;
    doc.resolver = Some(Rc::new(r.res));
    doc.font_descriptions = font_descriptions;
    let mut prop = |kind: &str| root_rels.values().find(|(k, _)| k == kind).and_then(|(_, t)| pkg.read_str(t));
    doc.properties =
        super::properties::read(prop("core-properties").as_deref(), prop("extended-properties").as_deref(), prop("custom-properties").as_deref());
    super::fields::evaluate(&mut doc, now);
    super::frames::convert(&mut doc);
    doc.derive_image_sets();
    let divs = doc.resolver.as_ref().map(|r| r.divisions.clone()).unwrap_or_default();
    if !divs.is_empty() {
        doc.sections.iter_mut().for_each(|s| fold_divs(&mut s.blocks, &divs, false));
        doc.stories.values_mut().chain(doc.footnotes.values_mut()).chain(doc.endnotes.values_mut()).for_each(|b| fold_divs(b, &divs, false));
    }
    if editable {
        let blocks = doc.sections.iter().map(|s| s.blocks.clone()).collect();
        let stories = story_xml.into_iter().map(|(name, x)| (name.clone(), (x, doc.stories[&name].clone()))).collect();
        let (footnotes, endnotes) = (doc.footnotes.clone(), doc.endnotes.clone());
        let controls = doc.controls.clone();
        doc.source = Some(Rc::new(Source {
            package: bytes.to_vec(),
            main,
            xml,
            blocks,
            stories,
            image_parts,
            notes: note_parts,
            footnotes,
            endnotes,
            controls,
            glossary: glossary_src,
            page_background: (doc.page_colour, doc.page_fill.clone()),
            page_theme: doc.page_colour_theme,
            separators: doc.separators.clone(),
            separator_defaults,
        }));
    }
    Ok(doc)
}

fn has_content(p: Node) -> bool {
    p.descendants().any(|d| {
        d.tag_name().namespace() == Some(W)
            && match d.tag_name().name() {
                "t" | "delText" => d.text().is_some_and(|t| !t.is_empty()),
                "tab" => !d.ancestors().any(|a| is_w(a, "tabs")),
                "br" | "cr" | "ptab" | "sym" | "drawing" | "pict" | "object" | "footnoteReference" | "endnoteReference" | "fldChar" | "fldSimple"
                | "noBreakHyphen" | "softHyphen" => true,
                _ => false,
            }
    })
}

/// A DrawingML `a:xfrm`, EMU: offset, extent, and a group's child offset and extent.
struct Xfrm {
    off: (f64, f64),
    ext: (f64, f64),
    ch_off: (f64, f64),
    ch_ext: (f64, f64),
}

/// The `a:xfrm` in `n`'s `props` child (`spPr`, `grpSpPr`).
fn child_xfrm(n: Node, props: &str) -> Option<Xfrm> {
    let x = n.children().find(|c| c.tag_name().name() == props)?.children().find(|c| is(*c, A, "xfrm"))?;
    let pair = |name: &str, a: &str, b: &str| {
        let e = x.children().find(|c| is(*c, A, name));
        let v = |k: &str| e.and_then(|e| e.attribute(k)).and_then(|v| v.parse::<f64>().ok());
        (v(a), v(b))
    };
    let (ox, oy) = pair("off", "x", "y");
    let (ex, ey) = pair("ext", "cx", "cy");
    let (cx, cy) = pair("chOff", "x", "y");
    let (cex, cey) = pair("chExt", "cx", "cy");
    let (ex, ey) = (ex.unwrap_or(0.0), ey.unwrap_or(0.0));
    Some(Xfrm {
        off: (ox.unwrap_or(0.0), oy.unwrap_or(0.0)),
        ext: (ex, ey),
        ch_off: (cx.unwrap_or(0.0), cy.unwrap_or(0.0)),
        ch_ext: (cex.unwrap_or(ex), cey.unwrap_or(ey)),
    })
}

struct Map {
    origin: (f64, f64),
    from: (f64, f64),
    scale: (f64, f64),
}

impl Map {
    /// EMU as points, for a group without a frame.
    const EMU: Map = Map { origin: (0.0, 0.0), from: (0.0, 0.0), scale: (1.0 / 12700.0, 1.0 / 12700.0) };

    /// A shape's box in points.
    fn rect(&self, f: &Xfrm) -> [f64; 4] {
        let p = |x: f64, y: f64| (self.origin.0 + (x - self.from.0) * self.scale.0, self.origin.1 + (y - self.from.1) * self.scale.1);
        let (a, b) = p(f.off.0, f.off.1);
        let (c, d) = p(f.off.0 + f.ext.0, f.off.1 + f.ext.1);
        [a, b, c, d]
    }

    /// The map of a group's children, the group `w` × `h` points at the origin: its child frame fills its box.
    fn of(f: &Xfrm, w: f64, h: f64) -> Map {
        let s = |len: f64, ext: f64| if ext > 0.0 { len / ext } else { 0.0 };
        Map { origin: (0.0, 0.0), from: f.ch_off, scale: (s(w, f.ch_ext.0), s(h, f.ch_ext.1)) }
    }

    /// This map moved to start at (`x`, `y`) points.
    fn at(self, x: f64, y: f64) -> Map {
        Map { origin: (x, y), ..self }
    }
}

// HTML divisions
fn read_divs(xml: Option<&str>, colours: &ThemeColours) -> HashMap<i64, Division> {
    fn walk(parent: Option<i64>, n: Node, colours: &ThemeColours, out: &mut HashMap<i64, Division>) {
        for d in children(n, "div") {
            let Some(id) = int(attr(d, "id")) else { continue };
            let m = |name: &str| child(d, name).and_then(|e| twips(val(e))).unwrap_or(0.0);
            let bdr = child(d, "divBdr");
            let b = |name: &str| bdr.and_then(|x| child(x, name)).map(|e| Border::parse(e, colours)).filter(|b| b.visible());
            out.insert(
                id,
                Division {
                    parent,
                    margins: [m("marLeft"), m("marRight"), m("marTop"), m("marBottom")],
                    borders: [b("top"), b("left"), b("bottom"), b("right")],
                },
            );
            if let Some(c) = child(d, "divsChild") {
                walk(Some(id), c, colours, out);
            }
        }
    }
    let mut out = HashMap::new();
    let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return out };
    if let Some(divs) = child(doc.root_element(), "divs") {
        walk(None, divs, colours, &mut out);
    }
    out
}

/// A line or connector preset holds no text: from one that has a text box on, each text box goes to the next shape with
/// a text box, in document order, and the last one is dropped.
fn shift_connector_text(sections: &mut [Section]) {
    fn line(s: Option<&crate::drawing::Shape>) -> bool {
        let name = s.and_then(|s| s.geometry()?.preset()).map_or("", |p| p.name());
        name == "line" || ["straightConnector", "bentConnector", "curvedConnector"].iter().any(|c| name.starts_with(c))
    }
    // Each text box in document order, with whether its shape is a line.
    fn walk(blocks: &mut [Block], each: &mut dyn FnMut(bool, &mut Option<TextBox>)) {
        for b in blocks {
            match b {
                Block::Para(p) => p.inlines.iter_mut().for_each(|i| match i {
                    Inline::Float(f, _) => {
                        each(line(f.shape.as_deref()), &mut f.text_box);
                        f.parts.iter_mut().for_each(|g| each(false, &mut g.text_box));
                    }
                    Inline::Picture(pic, _) => {
                        let Some(t) = pic.text.take() else { return };
                        let InlineText { id, text_box } = *t;
                        let mut tb = Some(text_box);
                        each(line(pic.shape.as_deref()), &mut tb);
                        pic.text = tb.map(|text_box| Box::new(InlineText { id, text_box }));
                    }
                    _ => {}
                }),
                Block::Table(t) => t.rows.iter_mut().flat_map(|r| &mut r.cells).for_each(|c| walk(&mut c.blocks, each)),
            }
        }
    }
    let mut texts = Vec::new();
    sections.iter_mut().for_each(|s| walk(&mut s.blocks, &mut |_, tb| texts.extend(tb.as_mut().map(|tb| std::mem::take(&mut tb.blocks)))));
    let mut texts = texts.into_iter();
    sections.iter_mut().for_each(|s| {
        walk(&mut s.blocks, &mut |is_line, tb| match tb {
            Some(_) if is_line => *tb = None,
            Some(tb) => tb.blocks = texts.next().unwrap_or_default(),
            None => {}
        })
    });
}

/// Where a paragraph closing story element `n` goes: before its end tag, or the whole element when it has none.
fn story_end(n: Node) -> std::ops::Range<usize> {
    let r = n.range();
    // An attribute value holds no `<`, so the last `</` is the element's own end tag.
    match n.document().input_text()[r.clone()].rfind("</") {
        Some(k) => r.start + k..r.start + k,
        None => r,
    }
}

/// Marks a note's drawings: a note has no floats, its anchors are laid out inline.
fn note_floats(blocks: &mut [Block]) {
    for b in blocks {
        match b {
            Block::Para(p) => p.inlines.iter_mut().for_each(|i| {
                if let Inline::Float(f, _) = i {
                    f.in_note = f.shape.is_some();
                }
            }),
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| &mut r.cells).for_each(|c| note_floats(&mut c.blocks)),
        }
    }
}

/// The neighbour-dependent part of the divisions around each paragraph (its indents come from the resolver), and each
/// table row's.
fn fold_divs(blocks: &mut [Block], divs: &HashMap<i64, Division>, in_cell: bool) {
    let row_chain = |r: Option<&Row>| div_chain(divs, r.and_then(|r| r.div.as_ref()).map(|d| d.id));
    let ends: Vec<(Vec<i64>, Vec<i64>)> = blocks
        .iter()
        .map(|b| match b {
            Block::Para(p) => {
                // A cell paragraph is in its own division only.
                let mut c = div_chain(divs, p.style.div);
                if in_cell {
                    c.truncate(1);
                }
                (c.clone(), c)
            }
            Block::Table(t) => (row_chain(t.rows.first()), row_chain(t.rows.last())),
        })
        .collect();
    let mut seen: Vec<i64> = Vec::new();
    let mut reopened: Vec<i64> = Vec::new();
    let mut runs = |chain: &[i64], prev: &[i64]| {
        for id in chain.iter().filter(|id| !prev.contains(id)) {
            if seen.contains(id) {
                reopened.push(*id);
            } else {
                seen.push(*id);
                reopened.retain(|r| r != id);
            }
        }
        reopened.clone()
    };
    let most = |ids: &mut dyn Iterator<Item = &i64>, i: usize| ids.map(|id| divs[id].margins[i]).fold(0.0, f64::max);
    let width = |b: &Option<Border>| b.as_ref().map_or(0.0, |b| b.width * if &*b.style == "double" { 3.0 } else { 1.0 } + b.space);
    let boxes = |chain: &[i64], prev: &[i64], next: &[i64], again: &[i64]| -> Vec<DivBox> {
        (0..chain.len())
            .rev()
            .filter(|&k| divs[&chain[k]].borders.iter().any(Option::is_some))
            .map(|k| {
                let id = &chain[k];
                let outward = |i: usize| chain[k..].iter().map(|o| divs[o].margins[i]).sum::<f64>();
                let open = !again.contains(id);
                DivBox {
                    borders: divs[id].borders.clone(),
                    left: outward(0),
                    right: outward(1),
                    starts: open && !prev.contains(id),
                    ends: open && !next.contains(id),
                }
            })
            .collect()
    };
    let empty = Vec::new();
    for k in 0..blocks.len() {
        let (prev, next) = (k.checked_sub(1).map_or(&empty, |j| &ends[j].1), ends.get(k + 1).map_or(&empty, |e| &e.0));
        let table_before = k.checked_sub(1).is_some_and(|j| matches!(blocks[j], Block::Table(_)));
        let table_after = matches!(blocks.get(k + 1), Some(Block::Table(_)));
        let para_before = k.checked_sub(1).is_some_and(|j| matches!(blocks[j], Block::Para(_)));
        let para_after = matches!(blocks.get(k + 1), Some(Block::Para(_)));
        match &mut blocks[k] {
            Block::Para(p) => {
                let chain = &ends[k].0;
                let again = runs(chain, prev);
                let mut before = most(&mut chain.iter().filter(|id| !prev.contains(id)), 2);
                let mut after = most(&mut chain.iter().filter(|id| !next.contains(id)), 3);
                if table_before {
                    before = before.max(most(&mut prev.iter().filter(|id| !chain.contains(id)), 3));
                }
                if table_after {
                    after = after.max(most(&mut next.iter().filter(|id| !chain.contains(id)), 2));
                }
                if chain.is_empty() && before == 0.0 && after == 0.0 {
                    continue;
                }
                let boxes = boxes(chain, prev, next, &again);
                p.div = Some(Rc::new(DivEdge::new(&p.style, before, after, boxes.into())));
            }
            Block::Table(t) => {
                let chains: Vec<Vec<i64>> = t.rows.iter().map(|r| row_chain(Some(r))).collect();
                let n = t.rows.len();
                let split = t.borders[2].as_ref().filter(|b| b.visible()).map_or(0.0, |b| b.width);
                for (ri, r) in t.rows.iter_mut().enumerate() {
                    let chain = &chains[ri];
                    let Some(d) = r.div.as_mut().filter(|_| !chain.is_empty()) else { continue };
                    let row_prev = if ri == 0 { prev } else { &chains[ri - 1] };
                    let row_next = if ri + 1 == n { next } else { &chains[ri + 1] };
                    let again = runs(chain, row_prev);
                    let boxes = boxes(chain, row_prev, row_next, &again);
                    let top = if ri == 0 && para_before { 0.0 } else { most(&mut chain.iter().filter(|id| !row_prev.contains(id)), 2) };
                    let bottom = if ri + 1 == n && para_after { 0.0 } else { most(&mut chain.iter().filter(|id| !row_next.contains(id)), 3) };
                    let d = Rc::make_mut(d);
                    d.shift = chain.iter().map(|id| divs[id].margins[0]).sum();
                    let (split_above, split_below) = (ri > 0 && chains[ri - 1] != *chain, ri + 1 < n && chains[ri + 1].is_empty());
                    d.lead =
                        top + boxes.iter().filter(|b| b.starts).map(|b| width(&b.borders[0])).sum::<f64>() + if split_above { split } else { 0.0 };
                    d.trail =
                        bottom + boxes.iter().filter(|b| b.ends).map(|b| width(&b.borders[2])).sum::<f64>() + if split_below { split } else { 0.0 };
                    d.boxes = boxes.into();
                }
                t.rows.iter_mut().flat_map(|r| &mut r.cells).for_each(|c| fold_divs(&mut c.blocks, divs, true));
            }
        }
    }
}

/// What the document shows when it opens: its Show Markup (`w:revisionView`) and whether the white space between
/// pages is hidden (`w:doNotDisplayPageBoundaries`).
fn opening_view(root: Node) -> (ShowMarkup, bool) {
    let mut show = ShowMarkup::default();
    if let Some(v) = child(root, "revisionView") {
        let on = |name| on_off_str(attr(v, name));
        (show.insertions_deletions, show.formatting, show.comments) = (on("insDel"), on("formatting"), on("comments"));
    }
    (show, flag(root, "doNotDisplayPageBoundaries").unwrap_or(false))
}

/// Turns on the compatibility option `name` (a `w:compat` child, an on/off `w:compatSetting`, or
/// `useWord2013TrackBottomHyphenation=0` for that setting off).
pub(crate) fn set_option(p: &mut LayoutPolicy, name: &str) {
    match name {
        "adjustLineHeightInTable" => p.adjust_line_height_in_table = true,
        "alignTablesRowByRow" => p.align_tables_row_by_row = true,
        "allowHyphenationAtTrackBottom" => p.allow_hyphenation_at_track_bottom = true,
        "allowSpaceOfSameStyleInTable" => p.allow_space_of_same_style_in_table = true,
        "allowTextAfterFloatingTableBreak" => p.allow_text_after_floating_table_break = true,
        "applyBreakingRules" => p.apply_breaking_rules = true,
        "balanceSingleByteDoubleByteWidth" => p.balance_single_byte_double_byte_width = true,
        "doNotAutofitConstrainedTables" => p.do_not_autofit_constrained_tables = true,
        "doNotBreakWrappedTables" => p.do_not_break_wrapped_tables = true,
        "doNotExpandShiftReturn" => p.do_not_expand_shift_return = true,
        "doNotSuppressIndentation" => p.do_not_suppress_indentation = true,
        "doNotUseHTMLParagraphAutoSpacing" => p.do_not_use_html_paragraph_auto_spacing = true,
        "doNotUseIndentAsNumberingTabStop" => p.do_not_use_indent_as_numbering_tab_stop = true,
        "doNotVertAlignCellWithSp" => p.do_not_vert_align_cell_with_sp = true,
        "enableOpenTypeFeatures" => p.enable_open_type_features = true,
        "footnoteLayoutLikeWW8" => p.footnote_layout_like_ww8 = true,
        "growAutofit" => p.grow_autofit = true,
        "layoutRawTableWidth" => p.layout_raw_table_width = true,
        "mwSmallCaps" => p.mw_small_caps = true,
        "noColumnBalance" => p.no_column_balance = true,
        "noExtraLineSpacing" => p.no_extra_line_spacing = true,
        "noLeading" => p.no_leading = true,
        "noSpaceRaiseLower" => p.no_space_raise_lower = true,
        "noTabHangInd" => p.no_tab_hang_ind = true,
        "overrideTableStyleFontSizeAndJustification" => p.override_table_style_font_size_and_justification = true,
        "showBreaksInFrames" => p.show_breaks_in_frames = true,
        "spacingInWholePoints" => p.spacing_in_whole_points = true,
        "splitPgBreakAndParaMark" => p.split_pg_break_and_para_mark = true,
        "suppressBottomSpacing" => p.suppress_bottom_spacing = true,
        "suppressSpBfAfterPgBrk" => p.suppress_sp_bf_after_pg_brk = true,
        "suppressSpacingAtTopOfPage" => p.suppress_spacing_at_top_of_page = true,
        "suppressTopSpacing" => p.suppress_top_spacing = true,
        "suppressTopSpacingWP" => p.suppress_top_spacing_wp = true,
        "ulTrailSpace" => p.ul_trail_space = true,
        "useSingleBorderforContiguousCells" => p.use_single_border_for_contiguous_cells = true,
        "useWord2002TableStyleRules" => p.use_word2002_table_style_rules = true,
        "useWord97LineBreakRules" => p.use_word97_line_break_rules = true,
        "wpJustification" => p.wp_justification = true,
        "useWord2013TrackBottomHyphenation=0" => p.word2013_track_bottom_hyphenation_off = true,
        _ => p.other = true,
    }
}

pub(crate) fn read_settings(xml: Option<&str>) -> Settings {
    let mut s = Settings { policy: LayoutPolicy { mode: 12, ..Default::default() }, default_tab: 36.0, hyphenation_zone: 18.0, ..Default::default() };
    let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return s };
    let root = doc.root_element();
    if let Some(t) = child(root, "defaultTabStop").and_then(|n| twips(val(n)))
        && t > 0.0
    {
        s.default_tab = t;
    }
    s.compress_punctuation = child(root, "characterSpacingControl").is_some_and(|n| val(n).is_some_and(|v| v.starts_with("compressPunctuation")));
    s.even_and_odd = flag(root, "evenAndOddHeaders").unwrap_or(false);
    s.mirror_margins = flag(root, "mirrorMargins").unwrap_or(false);
    s.gutter_at_top = flag(root, "gutterAtTop").unwrap_or(false);
    s.no_form_shading = flag(root, "doNotShadeFormData").unwrap_or(false);
    s.auto_format_override = flag(root, "autoFormatOverride").unwrap_or(false);
    s.style_lock_theme = flag(root, "styleLockTheme").unwrap_or(false);
    s.style_lock_style_set = flag(root, "styleLockQFSet").unwrap_or(false);
    s.default_table_style = child(root, "defaultTableStyle").and_then(val).filter(|v| !v.is_empty()).map(str::to_string);
    s.variables = child(root, "docVars")
        .into_iter()
        .flat_map(|v| children(v, "docVar"))
        .filter_map(|v| Some((attr(v, "name")?.to_string(), attr(v, "val").unwrap_or_default().to_string())))
        .collect();
    let symbol = |name: &str| child(root, name).and_then(val).and_then(|v| v.chars().next());
    (s.decimal_symbol, s.list_separator) = (symbol("decimalSymbol"), symbol("listSeparator"));
    let rev = flag(root, "bookFoldRevPrinting").unwrap_or(false);
    s.book_fold = (rev || flag(root, "bookFoldPrinting").unwrap_or(false)).then_some(rev);
    s.strict_kinsoku = flag(root, "strictFirstAndLastChars").unwrap_or(false);
    s.auto_hyphenation = flag(root, "autoHyphenation").unwrap_or(false);
    s.display_background_shape = flag(root, "displayBackgroundShape").unwrap_or(false);
    s.do_not_hyphenate_caps = flag(root, "doNotHyphenateCaps").unwrap_or(false);
    s.borders_do_not_surround_header = flag(root, "bordersDoNotSurroundHeader").unwrap_or(false);
    s.borders_do_not_surround_footer = flag(root, "bordersDoNotSurroundFooter").unwrap_or(false);
    if let Some(z) = child(root, "hyphenationZone").and_then(|n| twips(val(n))) {
        s.hyphenation_zone = z;
    }
    s.consecutive_hyphen_limit = child(root, "consecutiveHyphenLimit").and_then(|n| int(val(n))).unwrap_or(0);
    s.track_revisions = flag(root, "trackRevisions").unwrap_or(false);
    s.do_not_track_moves = flag(root, "doNotTrackMoves").unwrap_or(false);
    s.do_not_track_formatting = flag(root, "doNotTrackFormatting").unwrap_or(false);
    s.endnotes_sect_end = child(root, "endnotePr").and_then(|p| child(p, "pos")).and_then(val) == Some("sectEnd");
    for tag in ["footnote", "endnote"] {
        for n in child(root, &format!("{tag}Pr")).into_iter().flat_map(|p| children(p, tag)) {
            s.listed_notes.push(format!("{tag}:{}", attr(n, "id").unwrap_or_default()));
        }
    }
    s.protection = child(root, "documentProtection").map(|n| Protection {
        edit: match attr(n, "edit") {
            Some("readOnly") => ProtectionKind::ReadOnly,
            Some("comments") => ProtectionKind::Comments,
            Some("trackedChanges") => ProtectionKind::TrackedChanges,
            Some("forms") => ProtectionKind::Forms,
            _ => ProtectionKind::None,
        },
        enforced: matches!(attr(n, "enforcement"), Some("1" | "true" | "on")),
        formatting: matches!(attr(n, "formatting"), Some("1" | "true" | "on")),
        password: {
            let a = |k: &str| attr(n, k).map(str::to_string);
            let algorithm = a("algorithmName").or_else(|| a("cryptAlgorithmSid").map(|s| super::review::algorithm_name(&s).to_string()));
            let hash = a("hashValue").or_else(|| a("hash"));
            let salt = a("saltValue").or_else(|| a("salt"));
            let spin = a("spinCount").or_else(|| a("cryptSpinCount")).and_then(|s| s.parse().ok()).unwrap_or(0);
            match (algorithm, hash, salt) {
                (Some(algorithm), Some(hash), Some(salt)) => Some(super::review::Password { algorithm, hash, salt, spin }),
                _ => None,
            }
        },
        raw: Raw(Some(raw(n))),
    });
    s.revision_view_raw = Raw(child(root, "revisionView").map(raw));
    if let Some(c) = child(root, "compat") {
        for e in c.children().filter(|e| e.is_element()) {
            if is_w(e, "compatSetting") {
                if attr(e, "name") == Some("compatibilityMode")
                    && let Some(m) = int(attr(e, "val"))
                {
                    s.policy.mode = m.min(15);
                } else if let Some(
                    name @ ("enableOpenTypeFeatures"
                    | "overrideTableStyleFontSizeAndJustification"
                    | "allowHyphenationAtTrackBottom"
                    | "allowTextAfterFloatingTableBreak"),
                ) = attr(e, "name")
                    && on_off_str(attr(e, "val"))
                {
                    set_option(&mut s.policy, name);
                } else if attr(e, "name") == Some("useWord2013TrackBottomHyphenation") && !on_off_str(attr(e, "val")) {
                    set_option(&mut s.policy, "useWord2013TrackBottomHyphenation=0");
                }
            } else if on_off(e) {
                set_option(&mut s.policy, e.tag_name().name());
            }
        }
    }
    s
}

#[derive(Clone)]
struct TableCtx {
    region: Rc<Region>,
}

/// An open complex field.
struct OpenField {
    id: FieldId,
    in_result: bool,
    first_code_run: Option<Run>,
    /// The paragraph it began in ([`Reader::para_no`]), and its begin marker's index in that paragraph's inlines.
    para: u64,
    begin: usize,
    /// It began in an earlier paragraph than the one being read: a field across paragraphs (a table of contents).
    /// Its markers and code were taken out at that paragraph's end; its result reads as plain text.
    spanning: bool,
    /// A TOC field (known at its separator): its result doesn't show the Hyperlink character style.
    toc: bool,
    /// Where its begin marker starts in the part, and then the part's bytes of its begin, code and separator runs when they
    /// hold only those (one parent): what its start edge keeps once it spans paragraphs.
    begin_at: usize,
    marker_runs: Option<std::ops::Range<usize>>,
    /// It spans paragraphs and its start is kept as an edge ([`FieldEdge`]): its end is kept too.
    edge: bool,
    /// A legacy form field's data (`w:ffData`): the kind it lays out as, and a drop-down's shown entry.
    form: Option<(FieldKind, Option<String>)>,
}

/// The main namespace's prefix where `n` is (`w:`, or empty when it is the default namespace).
fn prefix_of(n: Node) -> String {
    n.lookup_prefix(W).filter(|p| !p.is_empty()).map_or_else(String::new, |p| format!("{p}:"))
}

/// The part's bytes from the run holding `from` (where a field's first marker starts) to the end of run `r` (holding
/// its last, `last`), when `r` and the elements before it back to that run are runs holding only field markers and
/// code (one parent) and no marker before `from` or after `last`: what a field's edge keeps.
fn marker_runs(r: Node, last: Node, from: usize) -> Option<std::ops::Range<usize>> {
    let markers =
        |n: Node| is_w(n, "r") && n.children().filter(|c| c.is_element()).all(|c| is_w(c, "rPr") || is_w(c, "fldChar") || is_w(c, "instrText"));
    let marker_at = |n: Node, beyond: &dyn Fn(usize) -> bool| n.children().any(|c| is_w(c, "fldChar") && beyond(c.range().start));
    if marker_at(r, &|c| c > last.range().start) {
        return None;
    }
    // Bounded by the parent's children: one walk back.
    let mut at = r;
    loop {
        if !markers(at) {
            return None;
        }
        let range = at.range();
        if range.contains(&from) {
            return (!marker_at(at, &|c| c < from)).then_some(range.start..r.range().end);
        }
        if range.start < from {
            return None;
        }
        at = at.prev_sibling_element()?;
    }
}

/// A legacy form field's `w:ffData`: a check box (its `w:size` in points, shown checked), or a drop-down and the entry
/// it shows (`listEntry[result]`, `w:default` without `w:result`, entry 0 out of range; none without entries).
fn form_data(ff: Node) -> Option<(FieldKind, Option<String>)> {
    if let Some(cb) = child(ff, "checkBox") {
        let size = child(cb, "size").filter(|_| child(cb, "sizeAuto").is_none()).and_then(|s| num(val(s))).map(|h| h / 2.0);
        let checked = flag(cb, "checked").unwrap_or_else(|| child(cb, "default").and_then(|d| int(val(d))).is_some_and(|v| v != 0));
        return Some((FieldKind::CheckBox { size, checked }, None));
    }
    let dd = child(ff, "ddList")?;
    let entries: Vec<&str> = children(dd, "listEntry").map(|e| val(e).unwrap_or("")).collect();
    let at = child(dd, "result").or_else(|| child(dd, "default")).and_then(|r| int(val(r))).unwrap_or(0);
    let shown = entries.first().map(|first| usize::try_from(at).ok().and_then(|k| entries.get(k)).unwrap_or(first).to_string());
    Some((FieldKind::DropDown, shown))
}

struct Reader {
    res: Resolver,
    /// The theme, for the outline widths text boxes take from their style.
    theme: Rc<crate::drawing::theme::Theme>,
    settings: Settings,
    counters: Counters,
    images: Vec<Vec<u8>>,
    image_index: HashMap<String, usize>,
    /// Bookmark names by `w:id`, for their ends.
    bookmarks: HashMap<String, String>,
    fields: Vec<OpenField>,
    /// The paragraph being read (a number unique in the read), and the last number given.
    para_no: u64,
    para_count: u64,
    float_order: usize,
    pending_hidden: Option<Paragraph>,
    /// Compat 15: the empty paragraph whose `w:specVanish` mark started the pending join; its alignment and indents
    /// go to the joined paragraph.
    pending_spec: Option<Rc<ParaStyle>>,
    uncounted: bool,
    part: String,
    rels: HashMap<String, (String, String)>,
    /// The note being read (for its `w:footnoteRef` mark).
    note_id: i64,
    /// Reading the endnotes part.
    in_endnotes: bool,
    /// Reading the main body for editing: paragraphs and tables get their sources, runs their direct formatting.
    record: bool,
    /// The paragraph being read: elements that produced units ([`ParaSrc::segs`]), and whether any can't be mapped.
    segs: Vec<(std::ops::Range<usize>, usize)>,
    /// [`ParaSrc::simple`].
    simple: Vec<(usize, Vec<(std::ops::Range<usize>, usize)>)>,
    lossy: bool,
    /// [`Document::controls`].
    controls: HashMap<ControlId, Control>,
    /// Each bound inline control's own run (its `w:sdtPr/w:rPr` over its paragraph's base, in its part): what its
    /// data replaces its placeholder in.
    bound_runs: HashMap<ControlId, Run>,
    /// Who made the revisions and comments, and the people part's identities by author name.
    people: People,
    identities: HashMap<String, Identity>,
    /// New ids, past every id in the parts read.
    ids: IdCounter,
    diagnostics: Diagnostics,
    /// The revision wrappers the content being read is in: `w:ins` or `w:moveTo`, and `w:del` or `w:moveFrom`.
    wrap: RunRevisions,
    /// Comment and move range starts the story being read hasn't ended, and the open move ranges' names.
    open_ranges: Vec<(RangeKind, i64)>,
    moves: Vec<(RangeKind, Rc<str>, i64)>,
    /// Range markers between blocks, waiting for the next paragraph's start.
    pending_marks: Vec<Inline>,
    /// The comments part's comment ids; those a range or reference points to; those pointed to but missing.
    comment_ids: HashSet<i64>,
    anchored: HashSet<i64>,
    orphans: HashSet<i64>,
    /// While the glossary is read, its own comments part's ids: markers in a body that name one aren't orphans.
    glossary_comments: Option<HashSet<i64>>,
    /// [`Document::alt_chunks`], and the bytes of HTML they hold.
    alt_chunks: Vec<AltChunk>,
    alt_bytes: usize,
    /// Each HTML or text part a `w:altChunk` imports, read once (none: refused).
    alt_parts: HashMap<String, Option<Rc<str>>>,
    /// The `w:altChunk`s importing a document, read once the stories are: each placeholder, its part and `w:matchSrc`.
    alt_docs: Vec<(u64, String, bool)>,
    /// How many `w:altChunk` documents in this package is.
    alt_depth: u8,
}

/// Units added to `out` since `before` (its length and last inline's units then): text may grow the last inline.
fn added_units(out: &[Inline], (len, last): (usize, usize)) -> usize {
    let grown = if len > 0 { out[len - 1].units() - last } else { 0 };
    grown + out[len..].iter().map(Inline::units).sum::<usize>()
}

fn units_mark(out: &[Inline]) -> (usize, usize) {
    (out.len(), out.last().map_or(0, Inline::units))
}

impl Reader {
    /// The glossary's building blocks, each with its `w:docPart`'s byte range and its body as read. A part without a
    /// name or a body is left out (and kept as written), with a warning.
    fn read_glossary(&mut self, root: Node, pkg: &mut Package) -> Vec<(BuildingBlock, std::ops::Range<usize>)> {
        let mut out: Vec<(BuildingBlock, std::ops::Range<usize>)> = Vec::new();
        for part in child(root, "docParts").into_iter().flat_map(|d| children(d, "docPart")) {
            let (Some((name, category, gallery, guid)), Some(body)) = (super::glossary::part_props(part), child(part, "docPartBody")) else {
                let at = self.part.clone();
                self.diagnostics.warn(WarningCode::GlossaryUnreadable, Some(&at), || "a building block without a name or a body".into());
                continue;
            };
            let mut blocks = self.read_blocks(body, pkg, None);
            self.close_ranges(&mut blocks);
            if out.iter().any(|(b, _)| b.name == name && b.category == category) {
                let at = self.part.clone();
                self.diagnostics.warn(WarningCode::DuplicateBuildingBlock, Some(&at), || format!("two building blocks named {name}"));
            }
            let editable = child(body, "sectPr").is_none();
            out.push((BuildingBlock { name, category, gallery, guid, body: blocks, state: PartState::AsRead, editable }, part.range()));
        }
        out
    }

    fn enter_part(&mut self, part: &str, pkg: &mut Package) {
        self.part = part.to_string();
        self.rels = pkg.rels(part);
    }

    fn read_story(&mut self, path: &str, pkg: &mut Package, pick: impl for<'a, 'i> Fn(Node<'a, 'i>) -> Option<Node<'a, 'i>>) -> Option<Vec<Block>> {
        let xml = pkg.read_str(path)?;
        let dom = roxmltree::Document::parse(&xml).ok()?;
        let saved = (self.part.clone(), self.rels.clone());
        self.enter_part(path, pkg);
        let root = pick(dom.root_element())?;
        let mut blocks = self.read_story_blocks(root, pkg);
        self.close_ranges(&mut blocks);
        (self.part, self.rels) = saved;
        Some(blocks)
    }

    /// The comments part's comments, each body read as blocks, with its author, date and last paragraph's
    /// `w14:paraId` (its replies and done flag are joined in [`super::comments::link`]).
    fn read_comments(&mut self, root: Node, pkg: &mut Package) -> Vec<Comment> {
        let mut out = Vec::new();
        for c in children(root, "comment") {
            let id = self.revision_id(c);
            let author = self.author(c);
            let initials: Option<Rc<str>> = attr(c, "initials").filter(|i| !i.is_empty()).map(Rc::from);
            if let Some(p) = self.people.persons.get_mut(author.0 as usize)
                && p.initials.is_none()
            {
                p.initials = initials.clone();
            }
            let mut body = self.read_story_blocks(c, pkg);
            self.close_ranges(&mut body);
            let last = c.descendants().rfind(|d| is_w(*d, "p"));
            let para_id = last.and_then(|p| super::comments::hex(p.attribute((W14, "paraId")))).unwrap_or_else(|| self.ids.next_para_id());
            out.push(Comment {
                id: CommentId(id.0),
                author,
                initials,
                date: attr(c, "date").map(Timestamp::parse),
                date_utc: None,
                body,
                para_id,
                durable_id: None,
                parent: None,
                done: false,
                extensible: Raw::default(),
            });
        }
        out
    }

    // ---------------- revisions and comments ----------------

    fn warn(&mut self, code: WarningCode, message: impl FnOnce() -> String) {
        self.diagnostics.warn(code, Some(&self.part), message);
    }

    /// An element's `w:id`; a new one, reported, when it has none.
    fn revision_id(&mut self, e: Node) -> RevisionId {
        match int(attr(e, "id")) {
            Some(id) => {
                self.ids.saw_revision(id);
                RevisionId(id)
            }
            None => {
                let name = e.tag_name().name();
                self.warn(WarningCode::RevisionIdMissing, || format!("a w:{name} has no w:id"));
                RevisionId(self.ids.next_revision_id())
            }
        }
    }

    /// An element's `w:author`, with the people part's identity for that name; an empty name, reported, when it has none.
    fn author(&mut self, e: Node) -> AuthorId {
        let name = attr(e, "author").unwrap_or_else(|| {
            let tag = e.tag_name().name();
            self.warn(WarningCode::AuthorMissing, || format!("a w:{tag} has no w:author"));
            ""
        });
        self.people.author(name, self.identities.get(name))
    }

    /// The revision `e` records (`w:ins`, `w:del`, `w:rPrChange`, ...). Moves take the name of the move range they
    /// are in. A property change's kind is `Format`: what changed is the element holding it.
    fn mark(&mut self, e: Node, kind: MarkKind) -> Rc<RevisionMark> {
        let (id, author) = (self.revision_id(e), self.author(e));
        let range = match kind {
            MarkKind::MoveFrom => Some(RangeKind::MoveFromStart),
            MarkKind::MoveTo => Some(RangeKind::MoveToStart),
            MarkKind::Insert | MarkKind::Delete | MarkKind::Format => None,
        };
        let move_name = range.and_then(|k| self.moves.iter().rev().find(|m| m.0 == k)).map(|m| m.1.clone());
        Rc::new(RevisionMark { id, author, date: attr(e, "date").map(Timestamp::parse), kind, move_name })
    }

    /// `e`'s child `name` as a revision of `kind`.
    fn mark_at(&mut self, e: Option<Node>, name: &str, kind: MarkKind) -> Option<Rc<RevisionMark>> {
        let c = child(e?, name)?;
        Some(self.mark(c, kind))
    }

    /// A property change (`w:tblPrChange`, `w:trPrChange`, ...: `name` in `e`), with the properties before it as written
    /// and as `previous` resolves them (given the earlier properties element, if any).
    fn table_change<'a, 'i>(
        &mut self,
        e: Option<Node<'a, 'i>>,
        name: &str,
        previous: impl FnOnce(&Self, Option<Node<'a, 'i>>) -> Option<PreviousProps>,
    ) -> Option<Rc<TableChange>> {
        let c = child(e?, name)?;
        let before = c.children().find(|n| n.is_element());
        let (previous_raw, previous) = (Raw(before.map(raw)), previous(self, before));
        Some(Rc::new(TableChange { mark: self.mark(c, MarkKind::Format), previous_raw, previous }))
    }

    /// A move mark with no move yet, named by the move range of its kind open now; `None` for any other mark.
    fn named_at_end(&self, m: &Option<Rc<RevisionMark>>) -> Option<Rc<RevisionMark>> {
        let m = m.as_ref().filter(|m| m.move_name.is_none())?;
        let range = match m.kind {
            MarkKind::MoveFrom => RangeKind::MoveFromStart,
            MarkKind::MoveTo => RangeKind::MoveToStart,
            MarkKind::Insert | MarkKind::Delete | MarkKind::Format => return None,
        };
        let name = self.moves.iter().rev().find(|o| o.0 == range)?.1.clone();
        Some(Rc::new(RevisionMark { move_name: Some(name), ..(**m).clone() }))
    }

    /// A run's revisions: the wrappers it is in and its `w:rPrChange` (the previous formatting resolved over `base`).
    fn run_revisions(&mut self, base: &RunProps, rpr: Option<Node>) -> RunRevisions {
        RunRevisions { ins: self.wrap.ins.clone(), del: self.wrap.del.clone(), fmt: self.format_change(base, rpr) }
    }

    /// A paragraph mark's revisions: inserted, deleted or moved (`w:ins`, ... in the mark's `w:rPr`), and its
    /// formatting change.
    fn mark_revisions(&mut self, base: &RunProps, rpr: Option<Node>) -> RunRevisions {
        let ins = self.mark_at(rpr, "ins", MarkKind::Insert).or_else(|| self.mark_at(rpr, "moveTo", MarkKind::MoveTo));
        let del = self.mark_at(rpr, "del", MarkKind::Delete).or_else(|| self.mark_at(rpr, "moveFrom", MarkKind::MoveFrom));
        RunRevisions { ins, del, fmt: self.format_change(base, rpr) }
    }

    fn format_change(&mut self, base: &RunProps, rpr: Option<Node>) -> Option<Rc<FormatChange>> {
        let c = child(rpr?, "rPrChange")?;
        let before = child(c, "rPr");
        let mut previous = self.run_style(&self.run_props(base, before));
        if self.record {
            Rc::make_mut(&mut previous).direct = Raw(before.map(raw));
        }
        Some(Rc::new(FormatChange { mark: self.mark(c, MarkKind::Format), previous, previous_raw: Raw(before.map(raw)) }))
    }

    /// `run` with the revisions it is read in.
    fn tracked(&mut self, mut run: Run, base: &RunProps, rpr: Option<Node>) -> Run {
        let rev = self.run_revisions(base, rpr);
        if !rev.is_empty() {
            Rc::make_mut(&mut run).rev = rev;
        }
        run
    }

    /// A paragraph's property change (`w:pPrChange`, the previous properties resolved as the paragraph is), and its
    /// list's numbering change (`w:numberingChange`, kept as written). A numbering change alone keeps the
    /// paragraph's properties as the previous ones.
    fn para_change(&mut self, ppr: Option<Node>, region: Option<&Region>, current: &ParaStyle) -> Option<Rc<ParaChange>> {
        let ppr = ppr?;
        let numbering = child(ppr, "numPr").and_then(|n| child(n, "numberingChange"));
        let change = child(ppr, "pPrChange");
        let mark = self.mark(change.or(numbering)?, MarkKind::Format);
        // With both, the numbering change's own id is kept clear of new ids too.
        if let (Some(_), Some(n)) = (change, numbering) {
            self.revision_id(n);
        }
        let before = change.and_then(|c| child(c, "pPr"));
        let previous = match change {
            Some(_) => {
                let mut s = self.res.paragraph(&ParaProps::parse(before, &self.res.styles.colours), region).style;
                if self.record {
                    s.direct = Raw(before.map(raw));
                }
                s
            }
            None => current.clone(),
        };
        Some(Rc::new(ParaChange { mark, previous: Rc::new(previous), previous_raw: Raw(before.map(raw)), numbering_change: Raw(numbering.map(raw)) }))
    }

    /// Whether comment `id` exists; a missing one is reported once.
    fn comment_exists(&mut self, id: i64) -> bool {
        // shortcut: a body's markers for the glossary's own comments are left out of the model (the bytes stay while
        // their paragraph is unedited) and the comments aren't shown; showing them needs the glossary's comments,
        // whose ids overlap the document's, joined under ids of their own.
        if self.glossary_comments.as_ref().is_some_and(|g| g.contains(&id)) {
            return false;
        }
        self.ids.saw_revision(id);
        self.anchored.insert(id);
        if self.comment_ids.contains(&id) {
            return true;
        }
        if self.orphans.insert(id) {
            self.warn(WarningCode::CommentReferenceOrphan, || format!("comment {id} is referenced but the comments part doesn't have it"));
        }
        false
    }

    /// A comment or move range marker, paired with the other end in the story being read. An end with no start, and
    /// a comment range or reference whose comment is missing, are reported and left out.
    /// A `w:bookmarkStart`'s or `w:bookmarkEnd`'s marker.
    fn bookmark(&mut self, e: Node) -> Option<Inline> {
        if !is_w(e, "bookmarkStart") && !is_w(e, "bookmarkEnd") {
            return None;
        }
        let id = attr(e, "id");
        if e.tag_name().name() == "bookmarkEnd" {
            return id.and_then(|id| self.bookmarks.get(id)).map(|name| Inline::Bookmark(format!("/{name}")));
        }
        let name = attr(e, "name").filter(|n| !n.is_empty() && !n.starts_with('/'))?;
        if let Some(id) = id {
            self.bookmarks.insert(id.to_string(), name.to_string());
        }
        Some(Inline::Bookmark(name.to_string()))
    }

    fn range(&mut self, e: Node) -> Option<RangeMark> {
        if e.tag_name().namespace() != Some(W) {
            return None;
        }
        let kind = match e.tag_name().name() {
            "commentRangeStart" => RangeKind::CommentStart,
            "commentRangeEnd" => RangeKind::CommentEnd,
            "moveFromRangeStart" => RangeKind::MoveFromStart,
            "moveFromRangeEnd" => RangeKind::MoveFromEnd,
            "moveToRangeStart" => RangeKind::MoveToStart,
            "moveToRangeEnd" => RangeKind::MoveToEnd,
            _ => return None,
        };
        let Some(id) = int(attr(e, "id")) else {
            self.warn(WarningCode::RangeUnpaired, || format!("a w:{} has no w:id", e.tag_name().name()));
            return None;
        };
        let start = match kind {
            RangeKind::CommentEnd => RangeKind::CommentStart,
            RangeKind::MoveFromEnd => RangeKind::MoveFromStart,
            RangeKind::MoveToEnd => RangeKind::MoveToStart,
            k => k,
        };
        if kind == start {
            if kind == RangeKind::CommentStart {
                if !self.comment_exists(id) {
                    return None;
                }
            } else {
                self.ids.saw_revision(id);
            }
            let name: Option<Rc<str>> = (kind != RangeKind::CommentStart).then(|| attr(e, "name")).flatten().map(Rc::from);
            if let Some(n) = &name {
                self.moves.push((kind, n.clone(), id));
            }
            self.open_ranges.push((kind, id));
            return Some(RangeMark { kind, id, name });
        }
        // The end of a range whose start was left out goes with it (an orphan's, or a glossary comment's).
        if kind == RangeKind::CommentEnd && (self.orphans.contains(&id) || self.glossary_comments.as_ref().is_some_and(|g| g.contains(&id))) {
            return None;
        }
        let Some(k) = self.open_ranges.iter().rposition(|o| *o == (start, id)) else {
            self.warn(WarningCode::RangeUnpaired, || format!("range {id} ends without a start"));
            return None;
        };
        self.open_ranges.remove(k);
        self.moves.retain(|m| (m.0, m.2) != (start, id));
        Some(RangeMark { kind, id, name: None })
    }

    /// The end of a story: range starts it never ended are reported and taken out of its blocks.
    fn close_ranges(&mut self, blocks: &mut [Block]) {
        let open = self.unended();
        strip_ranges(blocks, &open);
    }

    /// The end of a story: the range starts it never ended, reported. Markers after its last paragraph go.
    fn unended(&mut self) -> Vec<(RangeKind, i64)> {
        self.pending_marks.clear();
        self.moves.clear();
        let open = std::mem::take(&mut self.open_ranges);
        for (_, id) in &open {
            self.warn(WarningCode::RangeUnpaired, || format!("range {id} has no end"));
        }
        open
    }

    // ---------------- blocks ----------------

    fn read_body(&mut self, body: Node, pkg: &mut Package) -> Vec<Section> {
        let mut sections = Vec::new();
        let mut blocks = Vec::new();
        for (block, sect) in self.walk_blocks(body, pkg, None) {
            if let Some(b) = block {
                blocks.push(b);
            }
            if let Some(s) = sect {
                let mut sec = self.read_sect(s);
                sec.blocks = std::mem::take(&mut blocks);
                sections.push(sec);
            }
        }
        let last = child(body, "sectPr");
        let mut sec = match last {
            Some(s) => self.read_sect(s),
            None => self.default_sect(),
        };
        if let Some(p) = self.pending_hidden.take() {
            blocks.push(Block::Para(p));
        }
        let at = last.map_or_else(|| story_end(body), |s| s.range().start..s.range().start);
        self.close_story(&mut blocks, at);
        sec.blocks = blocks;
        sections.push(sec);
        // Ranges pair across the body's sections.
        let open = self.unended();
        for s in &mut sections {
            strip_ranges(&mut s.blocks, &open);
        }
        sections
    }

    /// A story that doesn't end with a paragraph (empty, or ending with a table) ends with an empty one, its source
    /// `at` ([`ParaSrc::closing`]).
    fn close_story(&mut self, blocks: &mut Vec<Block>, at: std::ops::Range<usize>) {
        if matches!(blocks.last(), Some(Block::Para(_))) {
            return;
        }
        let r = self.res.paragraph(&ParaProps::default(), None);
        let mark = self.run_style(&self.run_props(&r.base, None));
        let src = self
            .record
            .then(|| Rc::new(ParaSrc { ranges: vec![at], segs: Vec::new(), simple: Vec::new(), lossy: false, closing: true, joined: Vec::new() }));
        blocks.push(Block::Para(Paragraph {
            style: Rc::new(r.style),
            mark,
            list: None,
            inlines: Vec::new(),
            src,
            id: next_para_id(),
            div: None,
            controls: Vec::new(),
        }));
    }

    /// `blocks` without the paragraphs `ids` names, in tables and text boxes too; a `story` (and a text box) left
    /// without a final paragraph closed again where the last went ([`Reader::close_story`]).
    fn drop_paras(&mut self, blocks: &mut Vec<Block>, ids: &HashSet<u64>, story: bool) {
        let end = match blocks.last() {
            Some(Block::Para(p)) if ids.contains(&p.id) => Some(p.src.as_ref().and_then(|s| s.ranges.first()).map_or(0..0, |r| r.end..r.end)),
            _ => None,
        };
        blocks.retain(|b| !matches!(b, Block::Para(p) if ids.contains(&p.id)));
        for b in blocks.iter_mut() {
            match b {
                Block::Para(p) => {
                    for i in &mut p.inlines {
                        if let Some(tb) = i.story_mut() {
                            self.drop_paras(&mut tb.blocks, ids, true);
                        }
                    }
                }
                Block::Table(t) => {
                    for r in &mut t.rows {
                        for c in &mut r.cells {
                            self.drop_paras(&mut c.blocks, ids, false);
                        }
                    }
                }
            }
        }
        if let Some(at) = end.filter(|_| story) {
            self.close_story(blocks, at);
        }
    }

    /// [`Reader::read_blocks`] of a story's element, closed by [`Reader::close_story`].
    fn read_story_blocks(&mut self, container: Node, pkg: &mut Package) -> Vec<Block> {
        let mut blocks = self.read_blocks(container, pkg, None);
        self.close_story(&mut blocks, story_end(container));
        blocks
    }

    fn read_blocks(&mut self, container: Node, pkg: &mut Package, table: Option<&TableCtx>) -> Vec<Block> {
        let mut out: Vec<Block> = self.walk_blocks(container, pkg, table).into_iter().filter_map(|(b, _)| b).collect();
        if let Some(p) = self.pending_hidden.take() {
            out.push(Block::Para(p));
        }
        out
    }

    /// Block-level children in order, each with the `sectPr` that ends a section after it.
    fn walk_blocks<'a, 'i>(
        &mut self,
        container: Node<'a, 'i>,
        pkg: &mut Package,
        table: Option<&TableCtx>,
    ) -> Vec<(Option<Block>, Option<Node<'a, 'i>>)> {
        let mut out = Vec::new();
        let kids: Vec<Node> = container.children().filter(|c| c.is_element()).collect();
        let mut i = 0;
        while let Some(&n) = kids.get(i) {
            i += 1;
            if is_w(n, "p") {
                let sect = child(n, "pPr").and_then(|p| child(p, "sectPr"));
                self.uncounted = sect.is_some() && matches!(out.last(), Some((Some(Block::Para(_)), None))) && !has_content(n);
                let para = self.read_paragraph(n, pkg, table);
                self.uncounted = false;
                out.push((para.map(Block::Para), sect));
            } else if is_w(n, "tbl") {
                if let Some(p) = self.pending_hidden.take() {
                    out.push((Some(Block::Para(p)), None));
                }
                // Tables with nothing between them are one table.
                let mut run = vec![n];
                while let Some(&m) = kids.get(i)
                    && is_w(m, "tbl")
                    && self.joins(&run, m)
                {
                    run.push(m);
                    i += 1;
                }
                let t = self.read_table(&run, pkg);
                // A table without rows holds nothing.
                if !t.rows.is_empty() {
                    out.push((Some(Block::Table(t)), None));
                }
            } else if is_w(n, "sdt") || is_w(n, "customXml") {
                let content = if is_w(n, "sdt") { child(n, "sdtContent") } else { Some(n) };
                if let Some(c) = content {
                    // Block-level controls: the blocks of its content carry it, outermost first.
                    let id = self.control(n);
                    let from = out.len();
                    out.extend(self.walk_blocks(c, pkg, table));
                    for (b, _) in &mut out[from..] {
                        match b {
                            Some(Block::Para(p)) => p.controls.insert(0, id),
                            Some(Block::Table(t)) => t.controls.insert(0, id),
                            None => {}
                        }
                    }
                }
            } else if is(n, MC, "AlternateContent")
                && let Some(c) = choose(n)
            {
                out.extend(self.walk_blocks(c, pkg, table));
            } else if is_w(n, "altChunk") {
                self.alt_chunk(n, pkg, &mut out);
            } else if let Some(m) = self.range(n) {
                self.pending_marks.push(Inline::Range(m));
            } else if let Some(b) = self.bookmark(n) {
                self.pending_marks.push(b);
            }
        }
        // Markers after the container's last paragraph end it.
        if !self.pending_marks.is_empty() {
            let last = match &mut self.pending_hidden {
                Some(p) => Some(p),
                None => match out.last_mut() {
                    Some((Some(Block::Para(p)), _)) => Some(p),
                    _ => None,
                },
            };
            if let Some(p) = last {
                p.inlines.append(&mut self.pending_marks);
            }
        }
        out
    }

    /// A `w:altChunk` importing HTML, text or a document: an empty paragraph in its place added to `out`, its source
    /// the element, and the content kept ([`Document::alt_chunks`], a document's once read). Nothing (the element kept
    /// as read, not drawn) for another format, past [`MAX_ALT_DEPTH`], a
    /// glossary's, or past [`MAX_ALT_CHUNK`], [`MAX_ALT_CHUNKS`] or [`MAX_ALT_CHUNK_COUNT`]. Not inlined, and adding to `out` itself:
    /// [`Reader::walk_blocks`] recurses through nested tables, and its frame stays as small as it was.
    #[inline(never)]
    fn alt_chunk<'a, 'i>(&mut self, n: Node<'a, 'i>, pkg: &mut Package, out: &mut Vec<(Option<Block>, Option<Node<'a, 'i>>)>) {
        if let Some(p) = self.alt_chunk_para(n, pkg) {
            if let Some(h) = self.pending_hidden.take() {
                out.push((Some(Block::Para(h)), None));
            }
            out.push((Some(Block::Para(p)), None));
        }
    }

    fn alt_chunk_para(&mut self, n: Node, pkg: &mut Package) -> Option<Paragraph> {
        if self.glossary_comments.is_some() {
            return None;
        }
        let target = n.attribute((R, "id")).and_then(|id| self.rels.get(id)).map(|(_, t)| t.clone())?;
        let ext = target.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let (html, document) = match ext.as_str() {
            "htm" | "html" | "xhtml" => (true, false),
            "txt" => (false, false),
            "docx" | "docm" | "dotx" | "dotm" => (false, true),
            _ => {
                self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: only HTML, text and documents are imported"));
                return None;
            }
        };
        if self.alt_chunks.len() + self.alt_docs.len() >= MAX_ALT_CHUNK_COUNT {
            self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: too many altChunks to import"));
            return None;
        }
        let id = next_para_id();
        if document {
            if self.alt_depth >= MAX_ALT_DEPTH {
                self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: imported documents nest too deeply"));
                return None;
            }
            let match_src = child(n, "altChunkPr").and_then(|p| flag(p, "matchSrc")).unwrap_or(false);
            self.alt_docs.push((id, target, match_src));
        } else {
            let text = match self.alt_parts.get(&target) {
                Some(h) => h.clone(),
                None => {
                    let decode = if html { decode_html } else { decode_text };
                    let h: Option<Rc<str>> = pkg.read_at_most(&target, MAX_ALT_CHUNK as u64).map(|d| decode(&d).into());
                    self.alt_parts.insert(target.clone(), h.clone());
                    h
                }
            };
            let Some(text) = text else {
                self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: missing, unreadable or too large to import"));
                return None;
            };
            // Each import parses and lays out its content: one part imported many times counts each time.
            if self.alt_bytes + text.len() > MAX_ALT_CHUNKS {
                self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: too much content to import"));
                return None;
            }
            self.alt_bytes += text.len();
            let content = if html { AltContent::Html(text) } else { AltContent::Text(text) };
            self.alt_chunks.push(AltChunk { para: id, content });
        }
        let r = self.res.paragraph(&ParaProps::default(), None);
        let mark = self.run_style(&self.run_props(&r.base, None));
        // Written from the model in its place once it holds the content; kept as read while it doesn't.
        let src = self.record.then(|| {
            Rc::new(ParaSrc { ranges: vec![n.range()], segs: Vec::new(), simple: Vec::new(), lossy: true, closing: false, joined: Vec::new() })
        });
        Some(Paragraph { style: Rc::new(r.style), mark, list: None, inlines: Vec::new(), src, id, div: None, controls: Vec::new() })
    }

    /// The documents `w:altChunk`s import ([`Reader::alt_docs`]): each part read once, from this package's budget,
    /// and each import counted against [`MAX_ALT_DOCS`]. Read once the stories are, so a nested read never stands on
    /// a story's nested tables. Returns the placeholders of those refused, to go as one refused while reading does.
    fn read_alt_docs(&mut self, pkg: &mut Package, now: i64) -> HashSet<u64> {
        let depth = self.alt_depth + 1;
        // A document past what is left to import is refused, and stays so: what is left only shrinks.
        let mut read: HashMap<String, Result<(Rc<Document>, u64), &'static str>> = HashMap::new();
        let mut spent = 0u64;
        let mut refused = HashSet::new();
        for (para, target, match_src) in std::mem::take(&mut self.alt_docs) {
            let left = MAX_ALT_DOCS - spent;
            let got = match read.get(&target) {
                Some(got) => got.clone(),
                None => {
                    let got = read_alt_doc(pkg, &target, left, now, depth);
                    read.insert(target.clone(), got.clone());
                    got
                }
            };
            match got {
                Ok((doc, cost)) if cost <= left => {
                    spent += cost;
                    self.alt_chunks.push(AltChunk { para, content: AltContent::Document { doc, match_src } });
                }
                Ok(_) => {
                    read.insert(target.clone(), Err("too much to import"));
                    refused.insert(para);
                    self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: too much to import"));
                }
                Err(why) => {
                    refused.insert(para);
                    self.warn(WarningCode::UnsupportedElement, || format!("altChunk {target}: {why}"));
                }
            }
        }
        refused
    }

    fn default_sect(&self) -> Section {
        Section {
            break_kind: SectionBreak::NextPage,
            page_w: 612.0,
            page_h: 792.0,
            top: 72.0,
            bottom: 72.0,
            left: 72.0,
            right: 72.0,
            header: 36.0,
            footer: 36.0,
            gutter: 0.0,
            columns: Columns { num: 1, equal: true, space: 36.0, ..Default::default() },
            title_pg: false,
            headers: [None, None, None],
            footers: [None, None, None],
            pg_start: None,
            pg_fmt: None,
            chapter: None,
            line_numbers: None,
            v_align: "top".into(),
            page_borders: None,
            page_borders_xml: None,
            line_grid: None,
            notes: Default::default(),
            no_endnote: false,
            text_flow: 0,
            rtl_gutter: false,
            footnote_columns: 0,
            form_protected: true,
            blocks: Vec::new(),
            change: None,
            read: None,
        }
    }

    fn read_sect(&mut self, s: Node) -> Section {
        let mut sec = self.default_sect();
        if let Some(t) = child(s, "type").and_then(val) {
            sec.break_kind = match t {
                "continuous" => SectionBreak::Continuous,
                "evenPage" => SectionBreak::EvenPage,
                "oddPage" => SectionBreak::OddPage,
                "nextColumn" => SectionBreak::NextColumn,
                _ => SectionBreak::NextPage,
            };
        }
        if let Some(pg) = child(s, "pgSz") {
            sec.page_w = attr_twips(pg, "w").unwrap_or(612.0);
            sec.page_h = attr_twips(pg, "h").unwrap_or(792.0);
        }
        if let Some(m) = child(s, "pgMar") {
            let g = |n, d| attr_twips(m, n).unwrap_or(d);
            sec.top = g("top", 72.0);
            sec.bottom = g("bottom", 72.0);
            sec.left = g("left", 72.0);
            sec.right = g("right", 72.0);
            sec.header = g("header", 36.0);
            sec.footer = g("footer", 36.0);
            sec.gutter = g("gutter", 0.0);
        }
        if let Some(c) = child(s, "cols") {
            let num = int(attr(c, "num")).unwrap_or(1).clamp(1, Columns::MAX as i64) as usize;
            let space = attr_twips(c, "space").unwrap_or(36.0);
            let equal = attr(c, "equalWidth").map(|v| on_off_str(Some(v))).unwrap_or(true);
            let explicit: Vec<(f64, f64)> = children(c, "col")
                .take(Columns::MAX)
                .map(|col| (attr_twips(col, "w").unwrap_or(0.0), attr_twips(col, "space").unwrap_or(0.0)))
                .collect();
            sec.columns = Columns {
                num: if !equal && !explicit.is_empty() { explicit.len() } else { num },
                cols: if equal { Vec::new() } else { explicit },
                equal: equal || children(c, "col").next().is_none(),
                space,
                separator: attr(c, "sep").map(|v| on_off_str(Some(v))).unwrap_or(false),
            };
        }
        sec.title_pg = flag(s, "titlePg").unwrap_or(false);
        sec.rtl_gutter = flag(s, "rtlGutter").unwrap_or(false);
        sec.form_protected = flag(s, "formProt").unwrap_or(true);
        sec.footnote_columns = s
            .children()
            .find(|c| c.tag_name().namespace() == Some(W15) && c.tag_name().name() == "footnoteColumns")
            .and_then(|c| int(attr(c, "val")))
            .map_or(0, |n| n.clamp(0, 45) as u32);
        for (name, slot) in [("headerReference", 0), ("footerReference", 1)] {
            for r in children(s, name) {
                let idx = match attr(r, "type") {
                    Some("first") => 1,
                    Some("even") => 2,
                    _ => 0,
                };
                let target = r.attribute((R, "id")).and_then(|id| self.rels.get(id)).map(|(_, t)| t.clone());
                if slot == 0 {
                    sec.headers[idx] = target;
                } else {
                    sec.footers[idx] = target;
                }
            }
        }
        if let Some(p) = child(s, "pgNumType") {
            sec.pg_start = int(attr(p, "start"));
            sec.pg_fmt = attr(p, "fmt").map(str::to_string);
            let sep = match attr(p, "chapSep") {
                Some("period") => ".",
                Some("colon") => ":",
                Some("emDash") => "\u{2014}",
                Some("enDash") => "\u{2013}",
                _ => "-",
            };
            sec.chapter = int(attr(p, "chapStyle")).filter(|&n| n > 0).map(|n| (n, sep.to_string()));
        }
        if let Some(l) = child(s, "lnNumType") {
            sec.line_numbers = Some(LineNumbering {
                count_by: int(attr(l, "countBy")).unwrap_or(1).max(1),
                start: int(attr(l, "start")).unwrap_or(0),
                distance: attr_twips(l, "distance").unwrap_or(0.0),
                restart: attr(l, "restart").unwrap_or("newPage").to_string(),
            });
        }
        for (k, name) in [(0, "footnotePr"), (1, "endnotePr")] {
            let Some(p) = child(s, name) else { continue };
            let n = &mut sec.notes[k];
            n.fmt = child(p, "numFmt").and_then(val).map(str::to_string);
            n.start = child(p, "numStart").and_then(|e| int(val(e))).unwrap_or(n.start);
            n.restart = match child(p, "numRestart").and_then(val) {
                Some("eachSect") => NoteRestart::EachSect,
                Some("eachPage") => NoteRestart::EachPage,
                _ => NoteRestart::Continuous,
            };
            n.beneath = k == 0 && child(p, "pos").and_then(val) == Some("beneathText");
        }
        sec.no_endnote = flag(s, "noEndnote").unwrap_or(false);
        if let Some(v) = child(s, "vAlign").and_then(val) {
            sec.v_align = v.to_string();
        }
        if let Some(g) = child(s, "docGrid").filter(|g| matches!(attr(*g, "type"), Some("lines" | "linesAndChars" | "snapToChars"))) {
            sec.line_grid = Some((int(attr(g, "linePitch")).unwrap_or(0).max(1) * 2048 / 10) as i32);
        }
        sec.text_flow = child(s, "textDirection").and_then(val).map_or(0, |v| if v == "tbLrV" { 5 } else { crate::docx::model::text_flow(v) });
        if let Some(b) = child(s, "pgBorders") {
            sec.page_borders_xml = Some(raw(b).to_string());
            let side = |n| child(b, n).map(|n| Border::parse(n, &self.res.styles.colours));
            sec.page_borders = Some(Rc::new(PageBorders {
                top: side("top"),
                left: side("left"),
                bottom: side("bottom"),
                right: side("right"),
                offset_from_page: attr(b, "offsetFrom") == Some("page"),
                display: attr(b, "display").unwrap_or("allPages").to_string(),
                surround_header: !self.settings.borders_do_not_surround_header,
                surround_footer: !self.settings.borders_do_not_surround_footer,
                behind_text: attr(b, "zOrder") == Some("back"),
            }));
        }
        if let Some(c) = child(s, "sectPrChange") {
            let mark = self.mark(c, MarkKind::Format);
            let before = child(c, "sectPr");
            let previous = match before {
                Some(b) => self.read_sect(b).page(),
                None => self.default_sect().page(),
            };
            sec.change = Some(Rc::new(SectionChange { mark, previous, previous_raw: Raw(before.map(raw)) }));
        }
        if self.record {
            sec.read = Some(Rc::new(sec.page()));
        }
        sec
    }

    /// `run` carrying the `w:rPr` it was read from (editable reads), without change tracking marks: what new runs in
    /// this formatting are written with.
    fn with_direct(&self, run: Run, rpr: Option<Node>) -> Run {
        if !self.record {
            return run;
        }
        let tracked = |c: &Node| c.is_element() && matches!(c.tag_name().name(), "ins" | "del" | "moveFrom" | "moveTo" | "rPrChange");
        let direct = rpr.map(|n| {
            if !n.children().any(|c| tracked(&c)) {
                return raw(n);
            }
            let text = n.document().input_text();
            let open = &text[n.range().start..n.first_child().map_or(n.range().end, |c| c.range().start)];
            let name = open[1..].split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or("w:rPr");
            let kept: String = n.children().filter(|c| !tracked(c)).map(|c| &text[c.range()]).collect();
            format!("{}>{kept}</{name}>", open.trim_end_matches('>').trim_end_matches('/')).into()
        });
        let mut s = (*run).clone();
        s.direct = Raw(direct);
        Rc::new(s)
    }

    // ---------------- paragraphs ----------------

    fn read_paragraph(&mut self, n: Node, pkg: &mut Package, table: Option<&TableCtx>) -> Option<Paragraph> {
        let ppr_node = child(n, "pPr");
        let direct = ParaProps::parse(ppr_node, &self.res.styles.colours);
        let ParaResolved { style: mut para_style, base, level, num_id, ilvl, from_style, style_id } =
            self.res.paragraph(&direct, table.map(|t| &*t.region));
        if self.record {
            para_style.direct = Raw(ppr_node.map(raw));
        }
        para_style.change = self.para_change(ppr_node, table.map(|t| &*t.region), &para_style);
        let para_style = Rc::new(para_style);

        let mark_rpr_node = ppr_node.and_then(|p| child(p, "rPr"));
        let mark_props = self.run_props(&base, mark_rpr_node);
        let mut mark = self.with_direct(self.run_style(&mark_props), mark_rpr_node);
        let mark_rev = self.mark_revisions(&base, mark_rpr_node);
        if !mark_rev.is_empty() {
            Rc::make_mut(&mut mark).rev = mark_rev;
        }

        // List label
        let list = match (&level, num_id) {
            (Some(l), Some(id)) => {
                if !self.uncounted && self.res.label_shows(id, ilvl, from_style, style_id.as_deref()) {
                    self.counters.next(&self.res.numbering, &self.res.styles, id, ilvl).map(|text| {
                        let mut lp = mark_props.clone();
                        lp.merge(&l.rpr);
                        let picture = self.res.numbering.bullet_image(l);
                        ListLabel { text, run: self.res.run_style(&lp), jc: l.jc.clone(), suff: l.suff.clone(), picture, legacy: l.legacy }
                    })
                } else {
                    None
                }
            }
            _ => None,
        };

        // Range markers just before the paragraph start it.
        let mut inlines = std::mem::take(&mut self.pending_marks);
        let outer = (std::mem::take(&mut self.segs), std::mem::replace(&mut self.lossy, false), std::mem::take(&mut self.simple));
        self.para_count += 1;
        let outer_para = std::mem::replace(&mut self.para_no, self.para_count);
        self.read_inlines(n, &base, pkg, &mut inlines);
        // A moved mark is in the move range open where the paragraph ends: one starting inside the paragraph, after
        // its properties, takes the mark in too.
        let (ins, del) = (self.named_at_end(&mark.rev.ins), self.named_at_end(&mark.rev.del));
        if ins.is_some() || del.is_some() {
            let rev = &mut Rc::make_mut(&mut mark).rev;
            rev.ins = ins.or(rev.ins.take());
            rev.del = del.or(rev.del.take());
        }
        self.cut_spanning_fields(n, &mut inlines);
        self.para_no = outer_para;
        let (segs, lossy, simple) =
            (std::mem::replace(&mut self.segs, outer.0), std::mem::replace(&mut self.lossy, outer.1), std::mem::replace(&mut self.simple, outer.2));
        let src = self.record.then(|| Rc::new(ParaSrc { ranges: vec![n.range()], segs, simple, lossy, closing: false, joined: Vec::new() }));
        let para = Paragraph { style: para_style, mark: mark.clone(), list, inlines, src, id: next_para_id(), div: None, controls: Vec::new() };

        let spec = self.pending_spec.take();
        let mut para = match self.pending_hidden.take() {
            // A vanished mark joins the paragraph to the next: the first one's indents, alignment, tabs and space
            // before (unless it is empty), the first one's borders and shading, the last one's other properties; in
            // compat 15 the first one's indents, alignment, tabs, borders and shading for every line, in compat ≤ 14
            // each later line's indents, alignment, tabs, borders and shading from the paragraph of its first
            // character (a line starting at a vanished mark is the mark's).
            Some(mut prev) => {
                let at = prev.inlines.iter().map(Inline::units).sum::<usize>() as u32;
                let style = super::resolve::joined_style(&prev.style, &para.style, at, spec.as_deref(), self.res.styles.compat == Compat::Legacy);
                prev.inlines.extend(para.inlines);
                let src = match (prev.src, para.src) {
                    (Some(a), Some(b)) => Some(Rc::new(ParaSrc {
                        ranges: [a.ranges.clone(), b.ranges.clone()].concat(),
                        segs: [a.segs.clone(), b.segs.clone()].concat(),
                        simple: [a.simple.clone(), b.simple.clone()].concat(),
                        lossy: a.lossy || b.lossy,
                        closing: false,
                        joined: [a.joined.clone(), b.joined.clone()].concat(),
                    })),
                    (a, b) => a.or(b),
                };
                Paragraph {
                    style: Rc::new(style),
                    mark: para.mark,
                    list: prev.list.or(para.list),
                    inlines: prev.inlines,
                    src,
                    id: prev.id,
                    div: None,
                    controls: para.controls,
                }
            }
            None => para,
        };
        if mark.hidden {
            let spec_vanish = mark_rpr_node.and_then(|r| child(r, "specVanish")).is_some_and(on_off);
            let empty = para.inlines.iter().map(Inline::units).sum::<usize>() == 0;
            if spec_vanish && empty && self.res.styles.compat == Compat::Generic {
                self.pending_spec = Some(spec.unwrap_or_else(|| para.style.clone()));
            }
            let end = para.inlines.iter().map(Inline::units).sum::<usize>() as u32;
            if let Some(src) = para.src.as_mut().and_then(Rc::get_mut) {
                src.joined.push(JoinedPart { end, ppr: para.style.direct.clone(), spec_vanish });
            }
            para.mark = mark;
            self.pending_hidden = Some(para);
            return None;
        }
        Some(para)
    }

    /// A run's properties over a base: its character style chain (a toggle level), then direct formatting.
    fn run_props(&self, base: &RunProps, rpr: Option<Node>) -> RunProps {
        self.res.run_props(base, &RunProps::parse(rpr, &self.res.styles.colours))
    }

    fn run_style(&self, p: &RunProps) -> Run {
        self.res.run_style(p)
    }

    fn read_inlines(&mut self, container: Node, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        for e in container.children().filter(|c| c.is_element()) {
            if e.tag_name().namespace() == Some(W) {
                match e.tag_name().name() {
                    "r" => self.read_run(e, base, pkg, out),
                    "br" | "cr" => {
                        let run = self.direct_run(e, base, &mut RunProps::default());
                        let kind = if e.tag_name().name() == "br" { break_kind(attr(e, "type"), attr(e, "clear")) } else { BreakKind::Line };
                        out.push(Inline::Break(kind, run));
                    }
                    // In a code too: a field nested in another's code (an `IF`'s operand).
                    "fldSimple" => {
                        let code = attr(e, "instr").unwrap_or("").to_string();
                        self.read_simple_field(e, code, base, pkg, out);
                    }
                    // A hyperlink reads as the HYPERLINK field it stands for.
                    "hyperlink" if !self.in_code() && self.hyperlink_code(e).is_some() => {
                        let code = self.hyperlink_code(e).unwrap_or_default();
                        self.read_simple_field(e, code, base, pkg, out);
                    }
                    // An inline custom XML wrapper: a control of kind Other around its content.
                    "customXml" if !self.in_code() => {
                        let id = self.control(e);
                        out.push(Inline::ControlStart(id));
                        self.read_inlines(e, base, pkg, out);
                        out.push(Inline::ControlEnd(id));
                    }
                    "hyperlink" | "smartTag" | "customXml" | "bdo" | "dir" => {
                        self.read_inlines(e, base, pkg, out);
                    }
                    // Revision wrappers: their runs carry the revision (one mark for all of them), nested ones both.
                    name @ ("ins" | "moveTo" | "del" | "moveFrom") => {
                        let (kind, ins) = match name {
                            "ins" => (MarkKind::Insert, true),
                            "moveTo" => (MarkKind::MoveTo, true),
                            "del" => (MarkKind::Delete, false),
                            _ => (MarkKind::MoveFrom, false),
                        };
                        let m = Some(self.mark(e, kind));
                        let outer = if ins { std::mem::replace(&mut self.wrap.ins, m) } else { std::mem::replace(&mut self.wrap.del, m) };
                        self.read_inlines(e, base, pkg, out);
                        if ins {
                            self.wrap.ins = outer;
                        } else {
                            self.wrap.del = outer;
                        }
                    }
                    "commentRangeStart" | "commentRangeEnd" | "moveFromRangeStart" | "moveFromRangeEnd" | "moveToRangeStart" | "moveToRangeEnd" => {
                        if let Some(m) = self.range(e) {
                            out.push(Inline::Range(m));
                        }
                    }
                    "sdt" => {
                        let Some(c) = child(e, "sdtContent") else { continue };
                        let pr = child(e, "sdtPr");
                        if pr.is_some_and(|p| p.children().any(|x| is(x, W14, "checkbox"))) {
                            self.read_checkbox(e, c, pr, base, out);
                            continue;
                        }
                        let (mark, segs) = (units_mark(out), self.segs.len());
                        // An inline control's content between its markers.
                        let id = self.control(e);
                        if self.controls.get(&id).is_some_and(|k| k.props.binding.is_some())
                            && let Some(rpr) = pr.and_then(|p| child(p, "rPr"))
                        {
                            let run = self.with_direct(self.run_style(&self.run_props(base, Some(rpr))), Some(rpr));
                            self.bound_runs.insert(id, run);
                        }
                        out.push(Inline::ControlStart(id));
                        self.read_inlines(c, base, pkg, out);
                        out.push(Inline::ControlEnd(id));
                        // A picture control is one object: its seg is the whole control, so it is deleted as a whole
                        // and text typed next to it stays out of it.
                        let picture = pr.is_some_and(|p| child(p, "picture").is_some());
                        if picture && self.record && added_units(out, mark) == 1 {
                            self.segs.truncate(segs);
                            self.segs.push((e.range(), 1));
                        }
                    }
                    "bookmarkStart" | "bookmarkEnd" => out.extend(self.bookmark(e)),
                    // Properties, read with their element; Word's spelling state, rebuilt by Word.
                    "pPr" | "rPr" | "customXmlPr" | "smartTagPr" | "sdtPr" | "sdtEndPr" | "proofErr" => {}
                    // Editing permissions: kept until protection reads them.
                    "permStart" | "permEnd" => self.opaque(e, out),
                    _ => self.unsupported(e, out),
                }
            } else if is(e, MC, "AlternateContent")
                && let Some(c) = choose(e)
            {
                self.read_inlines(c, base, pkg, out);
            } else {
                self.unsupported(e, out);
            }
        }
    }

    /// The run child `c` as the source of the pictures it added (editable reads).
    fn picture_src(&self, c: Node, added: &mut [Inline]) {
        if self.record {
            for i in added {
                if let Inline::Picture(p, _) = i {
                    p.src = Raw(Some(raw(c)));
                }
            }
        }
    }

    /// An element between runs the engine doesn't model, kept as read (editable reads).
    fn opaque(&mut self, e: Node, out: &mut Vec<Inline>) {
        if self.record {
            out.push(Inline::Opaque(Rc::new(Opaque { xml: Raw(Some(raw(e))), field: None, src: None, run: None })));
        }
    }

    /// A run child the engine doesn't model (`w:movie`, an element it doesn't know), kept as a run of its own with the
    /// run's start tag and properties as read: it is written back, moved and pasted with its text (editable reads).
    fn run_opaque(&mut self, r: Node, c: Node, out: &mut Vec<Inline>) {
        if !self.record {
            return;
        }
        let whole = raw(r);
        let open = &whole[..whole.find('>').map_or(0, |k| k + 1)];
        let name = &open[1..open.find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/').unwrap_or(1)];
        let rpr = child(r, "rPr").map_or_else(String::new, |p| raw(p).to_string());
        let xml = format!("{open}{rpr}{}</{name}>", raw(c));
        out.push(Inline::Opaque(Rc::new(Opaque { xml: Raw(Some(xml.into())), field: None, src: None, run: None })));
    }

    /// [`Reader::opaque`], reported as content that isn't drawn.
    fn unsupported(&mut self, e: Node, out: &mut Vec<Inline>) {
        let name = e.tag_name();
        let prefix = e.lookup_prefix(name.namespace().unwrap_or("")).unwrap_or("");
        self.warn(WarningCode::UnsupportedElement, || format!("{prefix}:{} isn't drawn; it is kept as read", name.name()));
        self.opaque(e, out);
    }

    fn read_run(&mut self, r: Node, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        let mut direct = RunProps::default();
        for c in r.children().filter(|c| c.is_element()).take_while(|c| is_w(*c, "rPr")) {
            direct.merge(&RunProps::parse(Some(c), &self.res.styles.colours));
        }
        let mut run = self.direct_run(r, base, &mut direct);
        let mut content = false;
        // The child being read and the units before it: what it added becomes a seg when the next one starts.
        let mut prev: Option<(std::ops::Range<usize>, (usize, usize))> = None;
        // A run-level `mc:AlternateContent` whose choice isn't understood reads as its fallback's run content, all of it
        // one seg (the element is saved whole or rewritten, never split).
        let kids = r.children().filter(|c| c.is_element()).flat_map(|c| {
            let fallback = is(c, MC, "AlternateContent").then(|| choose(c)).flatten().filter(|b| is(*b, MC, "Fallback") && sym_ex(c).is_none());
            let own = fallback.is_none().then_some((c, c));
            own.into_iter().chain(fallback.into_iter().flat_map(move |b| b.children().filter(|x| x.is_element()).map(move |x| (x, c))))
        });
        for (c, seg) in kids {
            if is_w(c, "rPr") {
                if content {
                    direct.merge(&RunProps::parse(Some(c), &self.res.styles.colours));
                    run = self.direct_run(r, base, &mut direct);
                }
                continue;
            }
            content = true;
            if self.record && prev.as_ref().is_none_or(|(r, _)| *r != seg.range()) {
                self.note_seg(out, prev.take());
                prev = Some((seg.range(), units_mark(out)));
            }
            // The pictures a run child adds keep its XML (its fallback with it).
            let first_new = out.len();
            if is(c, MC, "AlternateContent") {
                if let Some((ch, font)) = sym_ex(c) {
                    out.push(Inline::Sym { ch, font, run: run.clone(), control: None });
                } else if let Some(ch) = choose(c) {
                    for d in ch.children().filter(|d| is_w(*d, "drawing")) {
                        self.read_drawing(d, &run, pkg, out);
                    }
                }
                self.picture_src(c, &mut out[first_new..]);
                continue;
            }
            if c.tag_name().namespace() != Some(W) {
                self.run_opaque(r, c, out);
                continue;
            }
            let name = c.tag_name().name();
            // Complex fields
            if name == "fldChar" {
                let id = self.fields.last().map(|f| f.id);
                match attr(c, "fldCharType") {
                    Some("begin") => {
                        let id = FieldId::next();
                        let flag = |a: &str| attr(c, a).is_some_and(|v| v == "1" || v == "true" || v == "on");
                        self.fields.push(OpenField {
                            id,
                            in_result: false,
                            first_code_run: None,
                            para: self.para_no,
                            begin: out.len(),
                            spanning: false,
                            toc: false,
                            begin_at: c.range().start,
                            marker_runs: None,
                            edge: false,
                            form: child(c, "ffData").and_then(form_data),
                        });
                        let (lock, dirty) = (flag("fldLock"), flag("dirty"));
                        let form = child(c, "ffData").filter(|_| self.record).map(|f| {
                            let data = super::forms::FormData::parse(f);
                            Rc::new(FormField { xml: Raw(Some(raw(f))), read: data.clone(), data })
                        });
                        out.push(Inline::FieldBegin {
                            id,
                            kind: None,
                            lock,
                            dirty,
                            view: FieldView::Result,
                            label: None,
                            value: None,
                            form,
                            run: run.clone(),
                        });
                    }
                    Some("separate") => {
                        if let (Some(id), Some(f)) = (id, self.fields.last_mut())
                            && !f.in_result
                        {
                            f.in_result = true;
                            if !f.spanning {
                                f.marker_runs = marker_runs(r, c, f.begin_at);
                                let code: String =
                                    out[f.begin..].iter().filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None }).collect();
                                f.toc = code.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("TOC"));
                                out.push(Inline::FieldSep { id, run: run.clone() });
                            }
                        }
                    }
                    Some("end") => {
                        if let (Some(id), Some(f)) = (id, self.fields.pop()) {
                            if !f.spanning {
                                out.push(Inline::FieldEnd { id, run: run.clone() });
                                let style = self.run_style(&self.run_props(base, None));
                                self.close_field(out, f, &run, &style);
                            } else if f.edge && self.record {
                                let at = marker_runs(r, c, c.range().start);
                                let xml = match &at {
                                    Some(at) => Rc::from(&r.document().input_text()[at.clone()]),
                                    None => {
                                        self.lossy = true;
                                        Rc::from(FieldEdge::End.xml(&prefix_of(r)).as_str())
                                    }
                                };
                                out.push(Inline::Opaque(Rc::new(Opaque {
                                    xml: Raw(Some(xml)),
                                    field: Some(FieldEdge::End),
                                    src: at,
                                    run: Some(run.clone()),
                                })));
                            }
                        }
                    }
                    _ => {}
                }
                continue;
            }
            // A field nested in another's code holds its result as code text.
            let nested = self.fields.len() >= 2 && !self.fields[self.fields.len() - 2].in_result;
            if let Some(f) = self.fields.last_mut() {
                if nested && f.in_result && name == "instrText" {
                    if let Some(t) = c.text().filter(|t| !t.is_empty()) {
                        push_text(out, t, &run);
                    }
                    continue;
                }
                if !f.in_result {
                    if matches!(name, "instrText" | "delInstrText") && !f.spanning {
                        f.first_code_run.get_or_insert(run.clone());
                        if let Some(t) = c.text().filter(|t| !t.is_empty()) {
                            push_text(out, t, &run);
                        }
                    }
                    continue;
                }
            }
            // A comment's anchor, shown or not.
            if name == "commentReference" {
                match int(attr(c, "id")) {
                    Some(id) if self.comment_exists(id) => out.push(Inline::CommentRef { id: CommentId(id), run: run.clone() }),
                    Some(_) => {}
                    None => self.warn(WarningCode::CommentReferenceOrphan, || "a comment reference has no w:id".into()),
                }
                continue;
            }
            match name {
                "t" | "delText" => {
                    if let Some(t) = c.text().filter(|t| !t.is_empty()) {
                        // A soft hyphen character in w:t is no optional hyphen (only w:softHyphen is): an ordinary
                        // character that never breaks.
                        // ponytail: read as a no-break hyphen, so it draws the font's hyphen rather than its own
                        // U+00AD glyph (the same in the common fonts); its own mark in the model if they differ.
                        let t = if t.contains('\u{AD}') { std::borrow::Cow::Owned(t.replace('\u{AD}', "\u{2011}")) } else { t.into() };
                        let t = &*t;
                        // A line break inside w:t shows as a space.
                        if t.contains(['\n', '\r']) {
                            push_text(out, &t.replace("\r\n", " ").replace(['\n', '\r'], " "), &run);
                        } else {
                            push_text(out, t, &run);
                        }
                    }
                }
                "tab" => out.push(Inline::Tab(run.clone())),
                "ptab" => {
                    let align = match attr(c, "alignment") {
                        Some("center") => TabKind::Center,
                        Some("right") => TabKind::Right,
                        _ => TabKind::Left,
                    };
                    let leader = match attr(c, "leader") {
                        Some("dot") => Leader::Dot,
                        Some("hyphen") => Leader::Hyphen,
                        Some("underscore") => Leader::Underscore,
                        Some("middleDot") => Leader::MiddleDot,
                        _ => Leader::None,
                    };
                    out.push(Inline::PTab(PTab { align, margin: attr(c, "relativeTo") != Some("indent"), leader }, run.clone()));
                }
                "br" => {
                    out.push(Inline::Break(break_kind(attr(c, "type"), attr(c, "clear")), run.clone()));
                }
                "cr" => out.push(Inline::Break(BreakKind::Line, run.clone())),
                "noBreakHyphen" => push_text(out, "\u{2011}", &run),
                // `w:pgNum`: the page number, as a PAGE field.
                "pgNum" => {
                    let id = FieldId::next();
                    let view = FieldView::Result;
                    out.push(Inline::FieldBegin {
                        id,
                        kind: Some(FieldKind::PgNum),
                        lock: false,
                        dirty: false,
                        view,
                        label: None,
                        value: None,
                        form: None,
                        run: run.clone(),
                    });
                    out.push(Inline::Text("PAGE".into(), run.clone()));
                    out.push(Inline::FieldSep { id, run: run.clone() });
                    out.push(Inline::FieldEnd { id, run: run.clone() });
                }
                "softHyphen" => push_text(out, "\u{AD}", &run),
                "sym" => {
                    let font = attr(c, "font").unwrap_or("Symbol").to_string();
                    let code = attr(c, "char").and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0x3F);
                    // Symbol fonts map their characters to the private-use F000 page.
                    let code = if code < 0x100 && is_symbol_font(&font) { code + 0xF000 } else { code };
                    out.push(Inline::Sym { ch: char::from_u32(code).unwrap_or('?'), font, run: run.clone(), control: None });
                }
                "drawing" => {
                    self.read_drawing(c, &run, pkg, out);
                    self.picture_src(c, &mut out[first_new..]);
                }
                "pict" | "object" => {
                    self.read_pict(c, &run, pkg, out);
                    self.picture_src(c, &mut out[first_new..]);
                }
                name if let Some(&(element, _)) = DATE_BLOCKS.iter().find(|(e, _)| *e == name) => {
                    out.push(Inline::DateBlock(element, String::new(), run.clone()));
                }
                "ruby" => {
                    let ruby = self.read_ruby(c, base, pkg);
                    out.push(Inline::Ruby(Rc::new(ruby), run.clone()));
                }
                "footnoteReference" | "endnoteReference" => out.push(Inline::NoteRef {
                    kind: if name == "footnoteReference" { NoteKind::Foot } else { NoteKind::End },
                    id: int(attr(c, "id")).unwrap_or(0),
                    custom: attr(c, "customMarkFollows").map(|v| on_off_str(Some(v))).unwrap_or(false),
                    run: run.clone(),
                }),
                "footnoteRef" => out.push(Inline::NoteMark(NoteKind::Foot, self.note_id, run.clone())),
                "endnoteRef" => out.push(Inline::NoteMark(NoteKind::End, self.note_id, run.clone())),
                "separator" => out.push(Inline::Separator { continuation: false, full: false, run: run.clone() }),
                "continuationSeparator" => out.push(Inline::Separator { continuation: true, full: self.in_endnotes, run: run.clone() }),
                // Word's layout cache and a comment's own mark: never kept.
                "lastRenderedPageBreak" | "annotationRef" | "instrText" | "delInstrText" => {}
                _ => self.run_opaque(r, c, out),
            }
        }
        if self.record {
            self.note_seg(out, prev);
        }
    }

    /// A ruby's base and annotation text with their runs, its alignment and raise.
    fn read_ruby(&mut self, ruby: Node, base: &RunProps, pkg: &mut Package) -> Ruby {
        let record = std::mem::replace(&mut self.record, false);
        let mut text = |name: &str| {
            let mut tmp = Vec::new();
            if let Some(n) = child(ruby, name) {
                self.read_inlines(n, base, pkg, &mut tmp);
            }
            tmp.into_iter().filter_map(|i| if let Inline::Text(t, r) = i { Some((t, r)) } else { None }).collect()
        };
        let (base_text, annotation) = (text("rubyBase"), text("rt"));
        self.record = record;
        let pr = child(ruby, "rubyPr");
        let align = match pr.and_then(|p| child(p, "rubyAlign")).and_then(val) {
            Some("distributeLetter") => 1,
            Some("distributeSpace") => 2,
            Some("left") => 3,
            Some("right") => 4,
            Some("rightVertical") => 5,
            _ => 0,
        };
        let raise = pr.and_then(|p| child(p, "hpsRaise")).and_then(|h| half_points(val(h))).unwrap_or(0.0);
        let size = pr.and_then(|p| child(p, "hps")).and_then(|h| half_points(val(h)));
        Ruby { base: base_text, text: annotation, align, raise, size, xml: Raw(Some(raw(ruby))) }
    }

    /// A checkbox content control (`w14:checkbox`) as one symbol: the character it
    /// shows, in its run's font. The symbol's run is the control's formatting without that font: text typed next to
    /// the checkbox takes it.
    fn read_checkbox(&mut self, sdt: Node, content: Node, pr: Option<Node>, base: &RunProps, out: &mut Vec<Inline>) {
        let rpr = content.descendants().find(|x| is_w(*x, "r")).and_then(|r| child(r, "rPr"));
        let props = self.run_props(base, rpr);
        let shown = self.run_style(&props);
        let checked = pr
            .and_then(|p| p.descendants().find(|x| is(*x, W14, "checked")))
            .and_then(|x| x.attribute((W14, "val")))
            .is_some_and(|v| v == "1" || v == "true");
        let state = |name: &str, dflt: char| {
            pr.and_then(|p| p.descendants().find(|x| is(*x, W14, name)))
                .and_then(|x| x.attribute((W14, "val")))
                .and_then(|v| u32::from_str_radix(v, 16).ok())
                .and_then(char::from_u32)
                .unwrap_or(dflt)
        };
        let (on, off) = (state("checkedState", '\u{2612}'), state("uncheckedState", '\u{2610}'));
        // Its face: the text it shows, else its `w:sym` (character and font), else the state it is in.
        let text: String = content.descendants().filter(|d| is_w(*d, "t")).filter_map(|t| t.text()).collect();
        let sym = content.descendants().find(|d| is_w(*d, "sym"));
        let sym_ch = sym.and_then(|s| attr(s, "char")).and_then(|v| u32::from_str_radix(v, 16).ok()).and_then(char::from_u32);
        let ch = text.chars().next().or(sym_ch).unwrap_or(if checked { on } else { off });
        let font = match sym.and_then(|s| attr(s, "font")).filter(|_| text.is_empty() && sym_ch.is_some()) {
            Some(f) => f.to_string(),
            None if shown.hint_east_asia => shown.east_asia.clone(),
            None => shown.h_ansi.clone(),
        };
        let mut plain = props;
        (plain.ascii, plain.h_ansi, plain.east_asia, plain.cs, plain.hint) =
            (base.ascii.clone(), base.h_ansi.clone(), base.east_asia.clone(), base.cs.clone(), base.hint.clone());
        let mut run = (*self.run_style(&plain)).clone();
        run.hint_east_asia = shown.hint_east_asia && sym_ch.is_none_or(|_| !text.is_empty());
        if self.record {
            // What new runs in this formatting are written with: the control's `w:rPr` without its `w:rFonts`.
            let other = |n: &Node| n.children().any(|c| c.is_element() && !is_w(c, "rFonts"));
            run.direct = Raw(rpr.filter(other).map(|n| {
                let at = n.range().start;
                let mut s = n.document().input_text()[n.range()].to_string();
                if let Some(f) = child(n, "rFonts") {
                    s.replace_range(f.range().start - at..f.range().end - at, "");
                }
                s.into()
            }));
            self.segs.push((sdt.range(), 1));
        }
        let id = self.control(sdt);
        if let Some(c) = self.controls.get_mut(&id) {
            // The checkbox's face is its `Sym`; written as it was read, or as a new checkbox control.
            c.props.kind = ControlKind::Checkbox { checked: on, unchecked: off };
            c.read = Some(c.props.clone());
        }
        let run = self.tracked(Rc::new(run), base, rpr);
        out.push(Inline::Sym { ch, font, run, control: Some(id) });
    }

    /// A content control (`w:sdt`) or custom XML wrapper at any level as a control: its properties, and
    /// the markup around its content.
    fn control(&mut self, n: Node) -> ControlId {
        let xml = n.document().input_text();
        let (content, props) =
            if is_w(n, "sdt") { (child(n, "sdtContent"), control_props(child(n, "sdtPr"), xml)) } else { (Some(n), ControlProps::default()) };
        let r = n.range();
        // Everything before the content's children, and everything after them.
        let (open, close, end) = match content {
            Some(c) if c != n => {
                let cr = c.range();
                let inner_start = cr.start + xml[cr.clone()].find('>').map_or(0, |i| i + 1);
                let inner_end = xml[..cr.end].rfind("</").unwrap_or(cr.end);
                (xml[r.start..inner_start].to_string(), xml[inner_end..r.end].to_string(), inner_end)
            }
            _ => {
                let inner_start = r.start + xml[r.clone()].find('>').map_or(0, |i| i + 1);
                // A customXml's own properties come first among its children.
                let start = child(n, "customXmlPr").map_or(inner_start, |p| p.range().end);
                let inner_end = xml[..r.end].rfind("</").unwrap_or(r.end);
                (xml[r.start..start].to_string(), xml[inner_end..r.end].to_string(), inner_end)
            }
        };
        let id = ControlId::next();
        self.controls.insert(id, Control { read: Some(props.clone()), props, open, close, at: Some((r.start, end)) });
        id
    }

    /// Records the units a run child added since `before` as one seg.
    fn note_seg(&mut self, out: &[Inline], before: Option<(std::ops::Range<usize>, (usize, usize))>) {
        if let Some((range, mark)) = before {
            let n = added_units(out, mark);
            if n > 0 {
                self.segs.push((range, n));
            }
        }
    }

    /// True inside an open field's code.
    fn in_code(&self) -> bool {
        self.fields.last().is_some_and(|f| !f.in_result)
    }

    /// `style`: the paragraph's style formatting, which a REF's text takes.
    fn close_field(&mut self, out: &mut [Inline], f: OpenField, end_run: &Run, style: &Run) {
        let Some(span) = super::fields::span_at(out, f.begin) else { return };
        let code = super::fields::code_text(out, span);
        let parsed = super::fields::parse(&code);
        let kind = parsed.kind();
        // With `\* MERGEFORMAT` the value's words take the cached result's formatting at layout; this run is for the
        // words past the cached ones.
        let fmt = if matches!(kind, Some(FieldKind::Ref(_))) {
            // A REF's text isn't in its code's formatting: the paragraph's style, the source's own formatting on it.
            Some(on_style(style, f.first_code_run.as_ref().unwrap_or(end_run)))
        } else {
            f.first_code_run.clone()
        };
        // A legacy check box or drop-down shows its form data, in its code's run.
        let form = f.form.filter(|(k, _)| parsed.name == if *k == FieldKind::DropDown { "FORMDROPDOWN" } else { "FORMCHECKBOX" });
        if let Inline::FieldBegin { kind: k, run, lock, value, .. } = &mut out[f.begin] {
            if let Some((form, shown)) = form {
                *run = f.first_code_run.unwrap_or_else(|| end_run.clone());
                (*k, *value) = (Some(form), shown);
                return;
            }
            // A locked field (`w:fldLock`) isn't updated: Word shows its result.
            let kind = kind.filter(|_| !*lock);
            if kind.is_some() {
                *run = fmt.unwrap_or_else(|| end_run.clone());
            }
            *k = kind;
        }
    }

    /// `w:fldSimple`: the same markers as a complex field, its `w:instr` the code. Editable reads map the element as
    /// one seg (its units all from it), so an unchanged field is copied as it was.
    /// A `w:hyperlink`'s field code: its external address and its bookmark (`w:anchor`). `None` for neither.
    fn hyperlink_code(&self, e: Node) -> Option<String> {
        let address = e.attribute((R, "id")).and_then(|id| self.rels.get(id)).filter(|(kind, _)| kind == "hyperlink").map(|(_, t)| t.as_str());
        let anchor = attr(e, "anchor");
        let quote = super::fields::quote;
        let mut code = String::from("HYPERLINK");
        if let Some(a) = address {
            code += &format!(" {}", quote(a));
        }
        if let Some(a) = anchor {
            code += &format!(" \\l {}", quote(a));
        }
        // The ScreenTip and target frame, as the field's switches: a link rewritten as its field keeps them.
        for (switch, name) in [("o", "tooltip"), ("t", "tgtFrame")] {
            if let Some(v) = attr(e, name) {
                code += &format!(" \\{switch} {}", quote(v));
            }
        }
        (address.is_some() || anchor.is_some()).then_some(code)
    }

    fn read_simple_field(&mut self, e: Node, code: String, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        let first = e.children().find(|c| is_w(*c, "r"));
        let rpr = first.and_then(|r| child(r, "rPr"));
        let run = self.with_direct(self.run_style(&self.run_props(base, rpr)), rpr);
        let run = self.tracked(run, base, rpr);
        let style = self.run_style(&self.run_props(base, None));
        let (mark, segs) = (units_mark(out), self.segs.len());
        let id = FieldId::next();
        let flag = |a: &str| attr(e, a).is_some_and(|v| v == "1" || v == "true" || v == "on");
        let begin = out.len();
        out.push(Inline::FieldBegin {
            id,
            kind: None,
            lock: flag("fldLock"),
            dirty: flag("dirty"),
            view: FieldView::Result,
            label: None,
            value: None,
            form: None,
            run: run.clone(),
        });
        if !code.is_empty() {
            out.push(Inline::Text(code, run.clone()));
        }
        out.push(Inline::FieldSep { id, run: self.run_style(&self.run_props(base, None)) });
        self.fields.push(OpenField {
            id,
            in_result: true,
            // A simple field's code has no run: the paragraph's style formatting is its code's.
            first_code_run: Some(on_style(&style, &run)),
            para: self.para_no,
            begin,
            spanning: false,
            toc: false,
            begin_at: e.range().start,
            marker_runs: None,
            edge: false,
            form: None,
        });
        self.read_inlines(e, base, pkg, out);
        let f = self.fields.pop().expect("the simple field pushed above");
        out.push(Inline::FieldEnd { id, run: run.clone() });
        self.close_field(out, f, &run, &style);
        if self.record {
            let inner = self.segs.split_off(segs);
            self.simple.push((e.range().start, inner));
            self.segs.push((e.range(), added_units(out, mark)));
        }
    }

    /// At a paragraph's end: a field begun in it and still open spans paragraphs. Its markers and code (with the
    /// fields in it) come out of the paragraph, and its result reads as plain text from here on.
    fn cut_spanning_fields(&mut self, n: Node, out: &mut Vec<Inline>) {
        let no = self.para_no;
        let Some(first) = self.fields.iter().position(|f| f.para == no && !f.spanning) else { return };
        let begin = self.fields[first].begin;
        let id = self.fields[first].id;
        // Its code ends at its separator if it is in this paragraph, else at the paragraph's end.
        let stop = out[begin..].iter().position(|i| matches!(i, Inline::FieldSep { id: s, .. } if *s == id)).map_or(out.len(), |k| begin + k + 1);
        let units_before: usize = out[..begin].iter().map(Inline::units).sum();
        let cut: usize = out[begin..stop].iter().map(Inline::units).sum();
        // Its start is kept as an edge, as its runs were written when they hold only its markers and code.
        let sep = out[begin..stop].iter().position(|i| matches!(i, Inline::FieldSep { id: s, .. } if *s == id)).map(|k| begin + k);
        let edge = match (&out[begin], sep) {
            (Inline::FieldBegin { lock, dirty, .. }, Some(sep)) if self.record => {
                let code = super::fields::code_text(out, super::fields::FieldSpan { begin, sep: Some(sep), end: sep });
                Some(FieldEdge::Start { code, lock: *lock, dirty: *dirty, view: FieldView::Result })
            }
            _ => None,
        };
        let begin_run = out[begin].run().cloned();
        out.drain(begin..stop);
        if let Some(edge) = edge {
            let at = self.fields[first].marker_runs.clone();
            let xml = match &at {
                Some(at) => Rc::from(&n.document().input_text()[at.clone()]),
                None => {
                    self.lossy = true;
                    Rc::from(edge.xml(&prefix_of(n)).as_str())
                }
            };
            out.insert(begin, Inline::Opaque(Rc::new(Opaque { xml: Raw(Some(xml)), field: Some(edge), src: at, run: begin_run })));
            self.fields[first].edge = true;
        }
        for f in &mut self.fields[first..] {
            f.spanning = true;
            // A code doesn't go on past its paragraph: what follows shows (a field without its end reads as text).
            f.in_result = true;
        }
        if self.record {
            // The segs of the units taken out go too (each of their elements produced only them).
            let mut at = 0usize;
            self.segs.retain(|(_, n)| {
                let keep = at + n <= units_before || at >= units_before + cut;
                at += n;
                keep
            });
        }
    }

    // ---------------- drawings ----------------

    fn image(&mut self, rid: &str, pkg: &mut Package) -> Option<usize> {
        let (_, target) = self.rels.get(rid)?.clone();
        if let Some(&i) = self.image_index.get(&target) {
            return Some(i);
        }
        let data = pkg.read(&target)?;
        if super::image::image_type(&data).is_none() {
            self.warn(WarningCode::ImageUndecodable, || format!("{target} isn't an image format the engine draws"));
        }
        self.images.push(data);
        self.image_index.insert(target, self.images.len() - 1);
        Some(self.images.len() - 1)
    }

    /// A table's rows with the controls wrapping each (outermost first): row-level `w:sdt` and `w:customXml` are
    /// kept as membership, not flattened away.
    fn rows_of<'a, 'i>(&mut self, t: Node<'a, 'i>, outer: &[ControlId]) -> Vec<(Node<'a, 'i>, Vec<ControlId>)> {
        let mut out = Vec::new();
        for n in t.children().filter(|c| c.is_element()) {
            if is_w(n, "tr") {
                out.push((n, outer.to_vec()));
            } else if is_w(n, "sdt") || is_w(n, "customXml") {
                let id = self.control(n);
                let mut inner = outer.to_vec();
                inner.push(id);
                let content = if is_w(n, "sdt") { child(n, "sdtContent") } else { Some(n) };
                if let Some(c) = content {
                    out.extend(self.rows_of(c, &inner));
                }
            }
        }
        out
    }

    /// A row's cells, each with the controls around it (outermost first).
    fn cells_of<'a, 'i>(&mut self, r: Node<'a, 'i>, outer: &[ControlId]) -> Vec<(Node<'a, 'i>, Vec<ControlId>)> {
        let mut out = Vec::new();
        for n in r.children().filter(|c| c.is_element()) {
            if is_w(n, "tc") {
                out.push((n, outer.to_vec()));
            } else if is_w(n, "sdt") || is_w(n, "customXml") {
                let mut inner = outer.to_vec();
                inner.push(self.control(n));
                let content = if is_w(n, "sdt") { child(n, "sdtContent") } else { Some(n) };
                if let Some(c) = content {
                    out.extend(self.cells_of(c, &inner));
                }
            }
        }
        out
    }

    fn read_pict(&mut self, pict: Node, run: &Run, pkg: &mut Package, out: &mut Vec<Inline>) {
        let mut src = DrawingSource { reader: self, pkg };
        let mut ctx = crate::drawing::read::Ctx { source: &mut src, unknown_presets: Vec::new() };
        let Some(v) = crate::drawing::vml::read(pict, &mut ctx) else { return };
        let st = &v.style;
        let mut shape = v.shape;
        shape.xfrm.ext = [crate::drawing::units::Emu((st.w * 12700.0).round() as i64), crate::drawing::units::Emu((st.h * 12700.0).round() as i64)];
        let image = shape.pic().and_then(|p| p.blip.image);
        let alt: Rc<str> = v.alt.as_str().into();
        let text_box = v.textbox.map(|(t, insets, v_anchor, auto_fit)| {
            let saved = (std::mem::take(&mut self.fields), std::mem::take(&mut self.wrap));
            let blocks = self.read_story_blocks(t, pkg);
            (self.fields, self.wrap) = saved;
            let outline = shape
                .sp()
                .and_then(|s| s.line.as_ref())
                .filter(|l| !matches!(l.fill, Some(crate::drawing::paint::Fill::None)))
                .and_then(|l| l.width)
                .map_or(0.0, |w| w.0 as f64 / 12700.0);
            TextBox { blocks, insets, outline, auto_fit, v_anchor, flow: 0 }
        });
        if !st.absolute {
            let text = text_box.map(|text_box| Box::new(InlineText { id: next_para_id(), text_box }));
            out.push(Inline::Picture(
                Picture {
                    w: st.w,
                    h: st.h,
                    image,
                    alt,
                    lock_aspect: false,
                    effect: [0.0; 4],
                    shape: Some(Box::new(shape)),
                    text,
                    src: Default::default(),
                },
                run.clone(),
            ));
            return;
        }
        let h_relative = match st.h_relative.as_deref() {
            Some("margin") => "margin",
            Some("page") => "page",
            Some("char") => "character",
            Some("left-margin-area") => "leftMargin",
            Some("right-margin-area") => "rightMargin",
            Some("inner-margin-area") => "insideMargin",
            Some("outer-margin-area") => "outsideMargin",
            _ => "column",
        };
        let v_relative = match st.v_relative.as_deref() {
            Some("margin") => "margin",
            Some("page") => "page",
            Some("line") => "line",
            Some("top-margin-area") => "topMargin",
            Some("bottom-margin-area") => "bottomMargin",
            Some("inner-margin-area") => "insideMargin",
            Some("outer-margin-area") => "outsideMargin",
            _ => "paragraph",
        };
        let align = |a: &Option<String>| a.as_deref().filter(|a| *a != "absolute").map(str::to_string);
        let (wrap, side) = v.wrap.unwrap_or_default();
        let wrap = match wrap.as_str() {
            "square" | "tight" | "through" | "topAndBottom" => wrap,
            _ => "none".into(),
        };
        let wrap_text = super::anchor::wrap_text(&side);
        // A style length goes to the nearest twip, halves up (the products of decimal points land just below).
        let tw = |v: f64| (v * 20.0 + 0.5 + 1e-7).floor();
        let twips = |v: f64| tw(v) / 20.0;
        let dflt = [0.0, 0.0, 9.0, 9.0];
        let dist = std::array::from_fn(|i| (st.wrap_dist[i].unwrap_or(dflt[i]) * 20.0).round() / 20.0);
        // Compat 15: a VML float is behind the text only without wrapping.
        let behind = st.z < 0 && (self.res.styles.compat != Compat::Generic || wrap == "none");
        let poly = v.wrap_coords.filter(|_| matches!(wrap.as_str(), "tight" | "through")).map(|p| Rc::from(super::bounded_polygon(p)));
        let line = shape.sp().and_then(|s| s.line.as_ref()).or_else(|| shape.pic().and_then(|p| p.line.as_ref()));
        let pen = line.filter(|l| !matches!(l.fill, Some(crate::drawing::paint::Fill::None))).map(|l| l.width.map_or(15.0, |w| w.0 as f64 / 635.0));
        let (h_align, v_align) = (align(&st.h_align), align(&st.v_align));
        // A line grows the stored box to its pixel bounds, aligned or not. Without one, a shape aligned on one axis
        // grows only at its right and bottom, one aligned on both not at all.
        let aligned = (h_align.is_some(), v_align.is_some());
        let effect = match (pen, aligned) {
            (None, (true, true)) => [0.0; 4],
            _ => vml_effect(pen, Some([tw(st.x), tw(st.y), tw(st.x) + tw(st.w), tw(st.y) + tw(st.h)])),
        };
        let effect = if pen.is_none() && (aligned.0 || aligned.1) { [0.0, 0.0, effect[2], effect[3]] } else { effect };
        self.float_order += 1;
        out.push(Inline::Float(
            Box::new(Float {
                id: next_para_id(),
                w: twips(st.w),
                auto_w: false,
                h: twips(st.h),
                image,
                alt,
                lock_aspect: false,
                text_box,
                h_relative: h_relative.into(),
                h_align,
                h_offset: twips(st.x),
                v_relative: v_relative.into(),
                v_align,
                v_offset: twips(st.y),
                wrap,
                wrap_text,
                dist,
                effect,
                poly,
                // A VML wrap polygon (`wrapcoords`) is kept as it is.
                poly_edited: true,
                behind,
                allow_overlap: v.allow_overlap,
                layout_in_cell: v.allow_in_cell,
                order: self.float_order,
                z: st.z.unsigned_abs().min(u64::from(u32::MAX)) as u32,
                src: Some(pict.range()),
                locked: false,
                frame_src: None,
                in_note: false,
                parts: Vec::new(),
                rot_swap: false,
                vml: true,
                relative: Relative {
                    size: st.size_percent.clone().map(|s| {
                        s.filter(|(_, p)| *p != 0).map(|(from, p)| {
                            let from = match from.as_str() {
                                "page" => "page",
                                "left-margin-area" => "leftMargin",
                                "right-margin-area" => "rightMargin",
                                "top-margin-area" => "topMargin",
                                "bottom-margin-area" => "bottomMargin",
                                "inner-margin-area" => "insideMargin",
                                "outer-margin-area" => "outsideMargin",
                                _ => "margin",
                            };
                            (from.to_string(), p)
                        })
                    }),
                    pos: st.pos_percent.map(|p| p.filter(|&p| p != 0 && p != -10001)),
                    drawn: None,
                },
                shape: Some(Box::new(shape)),
            }),
            run.clone(),
        ));
    }

    fn read_drawing(&mut self, d: Node, run: &Run, pkg: &mut Package, out: &mut Vec<Inline>) {
        let Some(item) = d.children().find(|c| is(*c, WP, "inline") || is(*c, WP, "anchor")) else { return };
        let extent = item.children().find(|c| is(*c, WP, "extent"));
        let w = extent.and_then(|e| emu(e.attribute("cx"))).unwrap_or(0.0);
        let h = extent.and_then(|e| emu(e.attribute("cy"))).unwrap_or(0.0);
        let blip = item.descendants().find(|n| is(*n, A, "blip"));
        let image = blip.and_then(crate::drawing::paint::blip_embed).map(str::to_string).and_then(|rid| self.image(&rid, pkg));
        let txbx = item.descendants().find(|n| is_w(*n, "txbxContent"));
        let shape = self.drawing_shape(item, pkg);
        let alt: Rc<str> = item
            .children()
            .find(|c| is(*c, WP, "docPr"))
            .and_then(|p| p.attribute("descr").filter(|s| !s.is_empty()).or(p.attribute("title")))
            .unwrap_or("")
            .into();
        let lock_aspect = crate::drawing::read::aspect_locked(item);
        if is(item, WP, "inline") {
            let ee = item.children().find(|c| is(*c, WP, "effectExtent"));
            // Negative where a turned picture's bounds are narrower than its box (Word writes a quarter-turned
            // picture's so); never past half the box, so the line box never turns inside out.
            let e = |n: &str, size: f64| ee.and_then(|e| emu(e.attribute(n))).unwrap_or(0.0).max(-size / 2.0);
            let effect = [e("l", w), e("t", h), e("r", w), e("b", h)];
            // An inline shape's text: a stand-alone shape's (a group's children's text isn't read inline).
            let group = item.descendants().any(|n| n.tag_name().name() == "wgp");
            let text = txbx.filter(|_| !group).map(|t| Box::new(InlineText { id: next_para_id(), text_box: self.text_box(item, t, pkg) }));
            out.push(Inline::Picture(
                Picture { w, h, image, alt, lock_aspect, effect, shape: Some(Box::new(shape)), text, src: Default::default() },
                run.clone(),
            ));
            return;
        }
        let pos = |name: &str| item.children().find(|c| is(*c, WP, name));
        let (mut h_rel, h_align, mut h_off) = position(pos("positionH"));
        let (mut v_rel, v_align, mut v_off) = position(pos("positionV"));
        if item.attribute("simplePos").is_some_and(|v| on_off_str(Some(v))) {
            let sp = pos("simplePos");
            let at = |n: &str| sp.and_then(|p| emu(p.attribute(n))).unwrap_or(0.0);
            if h_align.is_none() {
                (h_rel, h_off) = ("page".into(), at("x"));
            }
            if v_align.is_none() {
                (v_rel, v_off) = ("page".into(), at("y"));
            }
        }
        let wrap_el = item.children().find(|c| {
            c.tag_name().namespace() == Some(WP)
                && matches!(c.tag_name().name(), "wrapSquare" | "wrapTight" | "wrapThrough" | "wrapTopAndBottom" | "wrapNone")
        });
        let wrap = match wrap_el.map(|w| w.tag_name().name()) {
            Some("wrapSquare") => "square",
            Some("wrapTight") => "tight",
            Some("wrapThrough") => "through",
            Some("wrapTopAndBottom") => "topAndBottom",
            _ => "none",
        };
        let wrap_text = wrap_el.and_then(|w| w.attribute("wrapText")).map_or(WrapText::BothSides, super::anchor::wrap_text);
        let dist = |n: &str| emu(item.attribute(n)).unwrap_or(0.0);
        // Group shape
        let group = item.descendants().find(|n| n.tag_name().name() == "wgp");
        let mut parts = Vec::new();
        if let Some(g) = group {
            let map = child_xfrm(g, "grpSpPr").map_or(Map::EMU, |f| Map::of(&f, w, h));
            self.group_parts(g, &map, &mut Vec::new(), pkg, &mut parts);
        }
        let text_box = txbx.filter(|_| group.is_none()).map(|t| self.text_box(item, t, pkg));
        let image = if group.is_some() { None } else { image };
        self.float_order += 1;
        out.push(Inline::Float(
            Box::new(Float {
                id: next_para_id(),
                w,
                auto_w: false,
                h,
                image,
                alt: alt.clone(),
                lock_aspect,
                text_box,
                h_relative: h_rel,
                h_align,
                h_offset: h_off,
                v_relative: v_rel,
                v_align,
                v_offset: v_off,
                wrap: wrap.into(),
                wrap_text,
                dist: [dist("distT"), dist("distB"), dist("distL"), dist("distR")],
                effect: {
                    let ee = item.children().find(|c| is(*c, WP, "effectExtent"));
                    let e = |n: &str| ee.and_then(|e| e.attribute(n)?.parse::<f64>().ok()).map_or(0.0, |v| (v / 635.0).trunc() / 20.0);
                    [e("l"), e("t"), e("r"), e("b")]
                },
                poly_edited: wrap_el
                    .and_then(|w| w.children().find(|c| is(*c, WP, "wrapPolygon")))
                    .is_some_and(|p| p.attribute("edited").is_some_and(|v| on_off_str(Some(v)))),
                poly: wrap_el.filter(|_| matches!(wrap, "tight" | "through")).and_then(|w| {
                    let pts: Vec<(f64, f64)> = w
                        .children()
                        .filter(|c| is(*c, WP, "wrapPolygon"))
                        .flat_map(|p| p.children())
                        .filter(|q| q.is_element())
                        .filter_map(|q| Some((q.attribute("x")?.parse::<f64>().ok()? / 21600.0, q.attribute("y")?.parse::<f64>().ok()? / 21600.0)))
                        .collect();
                    (pts.len() >= 2).then(|| Rc::from(super::bounded_polygon(pts)))
                }),
                behind: item.attribute("behindDoc").is_some_and(|v| on_off_str(Some(v))),
                allow_overlap: item.attribute("allowOverlap").map(|v| on_off_str(Some(v))).unwrap_or(true),
                layout_in_cell: item.attribute("layoutInCell").map(|v| on_off_str(Some(v))).unwrap_or(true),
                order: self.float_order,
                z: item.attribute("relativeHeight").and_then(|v| v.parse().ok()).unwrap_or(0),
                src: Some(d.range()),
                locked: item.attribute("locked").is_some_and(|v| on_off_str(Some(v))),
                frame_src: None,
                in_note: false,
                parts,
                rot_swap: false,
                vml: false,
                relative: {
                    let tenths = |n: Option<Node>| n.and_then(|n| n.text()?.trim().parse::<i64>().ok()).map(|v| v / 100).filter(|&v| v != 0);
                    let size = |el: &str, pct: &str| {
                        let s = item.children().find(|c| is(*c, WP14, el))?;
                        Some((s.attribute("relativeFrom").unwrap_or("margin").to_string(), tenths(s.children().find(|c| is(*c, WP14, pct)))?))
                    };
                    let at = |el: &str, pct: &str| tenths(pos(el).and_then(|p| p.descendants().find(|c| is(*c, WP14, pct))));
                    Relative {
                        size: [size("sizeRelH", "pctWidth"), size("sizeRelV", "pctHeight")],
                        pos: [at("positionH", "pctPosHOffset"), at("positionV", "pctPosVOffset")],
                        drawn: None,
                    }
                },
                shape: Some(Box::new(shape)),
            }),
            run.clone(),
        ));
        if let Some(Inline::Float(f, _)) = out.last_mut() {
            let deg = f.shape.as_ref().map_or(0.0, |s| s.xfrm.rot.normalized().degrees()) % 180.0;
            if (45.0..135.0).contains(&deg) && self.settings.policy.mode != 12 {
                let (w0, h0) = (f.w, f.h);
                (f.w, f.h) = (h0, w0);
                if f.h_align.is_none() {
                    f.h_offset += (w0 - h0) / 2.0;
                }
                if f.v_align.is_none() {
                    f.v_offset += (h0 - w0) / 2.0;
                }
                f.rot_swap = true;
            }
        }
    }

    fn drawing_shape(&mut self, item: Node, pkg: &mut Package) -> crate::drawing::Shape {
        let mut src = DrawingSource { reader: self, pkg };
        let mut ctx = crate::drawing::read::Ctx { source: &mut src, unknown_presets: Vec::new() };
        let shape = crate::drawing::read::read(item, &mut ctx);
        for name in ctx.unknown_presets {
            self.warn(WarningCode::UnknownPreset, || format!("preset geometry {name} isn't one the engine draws; it is drawn as a rectangle"));
        }
        shape
    }

    /// A run's formatting from its direct properties `direct` over `base` (its first `w:rPr` kept for saving and
    /// revisions).
    fn direct_run(&mut self, r: Node, base: &RunProps, direct: &mut RunProps) -> Run {
        let toc = self.fields.iter().any(|f| f.toc && f.in_result);
        if toc && direct.style.as_deref() == Some("Hyperlink") {
            direct.style = None;
        }
        let props = self.res.run_props(base, direct);
        let run = self.with_direct(self.run_style(&props), child(r, "rPr"));
        let mut run = self.tracked(run, base, child(r, "rPr"));
        if toc {
            Rc::make_mut(&mut run).toc = true;
        }
        run
    }

    /// A text box's content and box settings: `shape` holds its `bodyPr` and outline, `t` its `w:txbxContent`.
    fn text_box(&mut self, shape: Node, t: Node, pkg: &mut Package) -> TextBox {
        let body = shape.descendants().find(|n| n.tag_name().name() == "bodyPr");
        let ins = |n: &str, dflt: f64| body.and_then(|b| emu(b.attribute(n))).unwrap_or(dflt);
        let ln = shape.descendants().find(|c| c.tag_name().name() == "spPr").and_then(|p| p.children().find(|c| is(*c, A, "ln")));
        // Without its own width the outline takes the width of the theme's line its style refers to.
        let style_w = || {
            let idx = shape.descendants().find(|c| is(*c, A, "lnRef"))?.attribute("idx")?.parse::<u32>().ok()?;
            Some(crate::drawing::paint::style_entry(&self.theme.lines, None, idx)?.width?.0)
        };
        let w = ln.and_then(|l| l.attribute("w")).and_then(|v| v.parse::<i64>().ok()).or_else(style_w).unwrap_or(0);
        let inside = ln.and_then(|l| l.attribute("algn")) == Some("in");
        let outline = 2.0 * if inside { w } else { w / 2 } as f64 / 12700.0;
        let auto_fit = body.is_some_and(|b| b.children().any(|c| is(c, A, "spAutoFit")));
        // A text box is its own content: the anchor's fields and revision wrappers don't reach into it.
        let saved = (std::mem::take(&mut self.fields), std::mem::take(&mut self.wrap));
        let blocks = self.read_story_blocks(t, pkg);
        (self.fields, self.wrap) = saved;
        let v_anchor = match body.and_then(|b| b.attribute("anchor")) {
            Some("ctr") => 1,
            Some("b") => 2,
            _ => 0,
        };
        TextBox { blocks, insets: [ins("lIns", 7.2), ins("tIns", 3.6), ins("rIns", 7.2), ins("bIns", 3.6)], outline, auto_fit, v_anchor, flow: 0 }
    }

    fn group_parts(&mut self, g: Node, map: &Map, path: &mut crate::drawing::ChildPath, pkg: &mut Package, out: &mut Vec<GroupPart>) {
        // The children the shape tree keeps, in its order (drawing/read.rs `child`).
        let kept = |c: &Node| matches!(c.tag_name().name(), "wsp" | "sp" | "cxnSp" | "pic" | "grpSp" | "graphicFrame");
        for (i, c) in g.children().filter(|c| c.is_element()).filter(kept).enumerate() {
            path.push(i as u16);
            match c.tag_name().name() {
                "grpSp" => {
                    if let Some(f) = child_xfrm(c, "grpSpPr") {
                        let r = map.rect(&f);
                        self.group_parts(c, &Map::of(&f, r[2] - r[0], r[3] - r[1]).at(r[0], r[1]), path, pkg, out);
                    }
                }
                "wsp" => {
                    if let (Some(t), Some(f)) = (c.descendants().find(|n| is_w(*n, "txbxContent")), child_xfrm(c, "spPr")) {
                        let text_box = Some(self.text_box(c, t, pkg));
                        out.push(GroupPart { rect: map.rect(&f), image: None, text_box, path: path.clone() });
                    }
                }
                "pic" => {
                    if let Some(f) = child_xfrm(c, "spPr") {
                        let rid = c.descendants().find(|n| is(*n, A, "blip")).and_then(crate::drawing::paint::blip_embed).map(str::to_string);
                        let image = rid.and_then(|r| self.image(&r, pkg));
                        out.push(GroupPart { rect: map.rect(&f), image, text_box: None, path: path.clone() });
                    }
                }
                _ => {}
            }
            path.pop();
        }
    }

    // ---------------- tables ----------------

    /// Whether the table `m` right after the tables `run` is one table with them: neither floats or both float with the
    /// same position, both have the same style and run the same way, and their grids are the same or have no column without a width (their edges then
    /// merge).
    fn joins(&self, run: &[Node], m: Node) -> bool {
        let kind = |t: Node| {
            let direct = TblProps::parse(child(t, "tblPr"), &self.res.styles.colours);
            let style = self.res.styles.table_style(direct.style.as_deref()).map(str::to_string);
            let mut tbl = self.res.styles.table_region(style.as_deref(), &[]).tbl.clone();
            tbl.merge(&direct);
            (tbl.floating, style, tbl.bidi_visual.unwrap_or(false))
        };
        let grids: Vec<Vec<f64>> = run.iter().copied().chain([m]).map(table_grid).collect();
        let first = kind(run[0]);
        kind(m) == first && (grids.iter().all(|g| *g == grids[0]) || grids.iter().flatten().all(|w| *w > 0.0))
    }

    /// One table from `ts`, tables with nothing between them: the first one's properties and grid, every table's
    /// rows in order.
    fn read_table(&mut self, ts: &[Node], pkg: &mut Package) -> Table {
        let t = ts[0];
        let direct = TblProps::parse(child(t, "tblPr"), &self.res.styles.colours);
        let style_id = self.res.styles.table_style(direct.style.as_deref()).map(str::to_string);
        let style_region = self.res.styles.table_region(style_id.as_deref(), &[]);
        let mut tbl = style_region.tbl.clone();
        tbl.merge(&direct);
        // Compat ≤ 14: a floating table at vertical position 0 stands in the flow, whatever it is anchored to.
        if self.res.styles.compat == Compat::Legacy
            && let Some(fp) = tbl.floating.as_mut()
            && fp.y == 0.0
            && fp.y_spec.is_none()
        {
            fp.v_anchor = TableAnchor::Text;
        }
        let widths = |g: Node| children(g, "gridCol").map(|c| attr_twips(c, "w").unwrap_or(0.0)).collect::<Vec<f64>>();
        let grid_node = child(t, "tblGrid");
        let mut grid = grid_node.map(widths).unwrap_or_default();
        // The earlier properties resolve over the earlier table style, as the table's own do over its style. Earlier
        // properties without borders leave the table's borders as they are (FX-REVIEW-TABLES and its rejected twin).
        let now = &tbl.borders;
        let change = self.table_change(child(t, "tblPr"), "tblPrChange", |r, before| {
            let prev = TblProps::parse(before, &r.res.styles.colours);
            let style_id = r.res.styles.table_style(prev.style.as_deref()).map(str::to_string);
            let mut tbl = r.res.styles.table_region(style_id.as_deref(), &[]).tbl.clone();
            tbl.merge(&prev);
            let b = if before.and_then(|n| child(n, "tblBorders")).is_some() { &tbl.borders } else { now };
            Some(PreviousProps::Table {
                style_id,
                width: tbl.width.unwrap_or(Width::Auto),
                jc: tbl.jc.unwrap_or(Jc::Left),
                ind: tbl.ind.map_or(0.0, Width::dxa),
                borders: [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()],
                fixed: tbl.fixed.unwrap_or(false),
                cell_spacing: tbl.cell_spacing.unwrap_or(0.0),
                look: prev.look,
                bands: (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0)),
            })
        });
        let grid_change = grid_node.and_then(|g| child(g, "tblGridChange")).map(|c| {
            let previous = child(c, "tblGrid").map(widths).unwrap_or_default();
            Rc::new(GridChange { mark: self.mark(c, MarkKind::Format), previous })
        });
        let look =
            direct.look.unwrap_or(Look { first_row: true, last_row: false, first_col: true, last_col: false, no_h_band: false, no_v_band: true });
        let mut row_nodes: Vec<(Node, Vec<ControlId>)> = Vec::new();
        let mut row_table = Vec::new();
        for (k, t) in ts.iter().enumerate() {
            let rows = self.rows_of(*t, &[]);
            row_table.extend(std::iter::repeat_n(k, rows.len()));
            row_nodes.extend(rows);
        }
        let nrows = row_nodes.len();
        let row_band = tbl.row_band.unwrap_or(0);
        let headers = row_nodes
            .iter()
            .enumerate()
            .take_while(|(i, (rn, _))| {
                let mut trp = style_region.tr.clone();
                trp.merge(&TrProps::parse(child(*rn, "trPr")));
                trp.header == Some(true) || *i == 0 && look.first_row
            })
            .count();
        let col_band = tbl.col_band.unwrap_or(0);
        let mut rows = Vec::new();
        let mut row_jc: Option<Option<Jc>> = None;
        let mut first_row_jc: Option<Jc> = None;
        let mut first_row_width: Option<Width> = None;
        let mut first_row_ind: Option<Width> = None;
        let mut left_margins: Vec<i64> = Vec::new();
        for (ri, (rn, row_controls)) in row_nodes.iter().enumerate() {
            let mut trp = style_region.tr.clone();
            trp.merge(&TrProps::parse(child(*rn, "trPr")));
            let mut ex = TblProps::parse(child(*rn, "tblPrEx"), &self.res.styles.colours);
            // A later table's rows keep its own alignment and cell margins as their exceptions.
            let mut table_shading = None;
            if row_table[ri] > 0 {
                let own = TblProps::parse(child(ts[row_table[ri]], "tblPr"), &self.res.styles.colours);
                // Its own shading over its style's, which joined tables share (Word shows the style's under one without).
                let shd = own.shd.as_ref().or(style_region.tbl.shd.as_ref());
                table_shading = Some((shd.and_then(Shd::flat), shd.and_then(Shd::hatch)));
                let mut later = TblProps { jc: own.jc, margins: own.margins, ..Default::default() };
                later.merge(&ex);
                ex = later;
            }
            // The row's own shading in place of its table's: one of no colour shows none.
            if ex.shd.as_ref().is_some_and(|s| s.flat().is_none()) {
                table_shading = Some((None, None));
            }
            trp.jc = trp.jc.or(ex.jc);
            // Every row's alignment, or none when they differ.
            row_jc = Some(if ri == 0 || row_jc == Some(trp.jc) { trp.jc } else { None });
            if ri == 0 {
                first_row_jc = trp.jc;
                // The first row's exceptions are the table's, unless the row starts a layout run of its own.
                let own = ex.fixed != Some(true) || tbl.fixed == Some(true);
                first_row_width = ex.width.filter(|_| own);
                first_row_ind = ex.ind.filter(|_| own);
            }
            let mut row_tbl = tbl.clone();
            row_tbl.borders.merge(&ex.borders);
            row_tbl.margins.merge(&ex.margins);
            let unset = if style_id.is_none() { 0.5 } else { 0.0 };
            left_margins.push((row_tbl.margins.left.unwrap_or(unset) * 20.0).round() as i64);
            let cell_nodes: Vec<(Node, Vec<ControlId>)> = self.cells_of(*rn, &[]);
            let ncols = cell_nodes.len();
            let mut cells = Vec::new();
            for (ci, (cn, cell_controls)) in cell_nodes.iter().enumerate() {
                let region = Rc::new(self.res.cell_region(style_id.as_deref(), look, (row_band, col_band), (ri, ci), (nrows, ncols), headers));
                let tm = &row_tbl.margins;
                let rev = self.cell_revisions(child(*cn, "tcPr"), &region.tc, tm, unset);
                let ctx = TableCtx { region: region.clone() };
                let blocks = self.read_blocks(*cn, pkg, Some(&ctx));
                cells.push(self.cell(*cn, &region.tc, tm, unset, blocks, cell_controls.clone(), rev));
            }
            rows.push(self.row(*rn, &trp, &ex, &tbl, &row_tbl, &style_region, cells, row_controls.clone()));
            if let Some(r) = rows.last_mut() {
                r.table_shading = table_shading;
                bound_spans(r);
            }
        }
        let grids: Vec<Vec<f64>> = ts.iter().copied().map(table_grid).collect();
        if grids.iter().any(|g| *g != grid) {
            // Compat ≤ 14: how far a later table's cell edges may move right onto the earlier edges, from the first
            // cells' left pulls: the first table's, less the later one's, plus 5 twips.
            let pull = |ri: usize| {
                let c = rows[ri].cells.first();
                let table = rows[ri].borders.as_ref().map_or(&tbl.borders.left, |b| &b[1]);
                let b = c.and_then(|c| c.left.as_ref()).or(table.as_ref()).filter(|b| b.visible()).map_or(0, |b| (b.width * 20.0).round() as i64);
                ((b + 1) / 2).max(c.map_or(0, |c| (c.margins[1] * 20.0).round() as i64))
            };
            let first_of = |k: usize| row_table.iter().position(|&t| t == k);
            let moves: Vec<i64> = (0..ts.len())
                .map(|k| match (k, first_of(0), first_of(k)) {
                    (1.., Some(a), Some(b)) if self.res.styles.compat == Compat::Legacy => pull(a) - pull(b) + 5,
                    _ => 0,
                })
                .collect();
            grid = merged_grid(&grids, &mut rows, &row_table, &moves);
        }
        // Compat ≤ 14: a joined table whose rows' left cell margins differ from the first row's by more than 5 twips
        // has every column edge 5 twips right, but the left edge of the rows with the largest margin.
        let widest = left_margins.iter().copied().max().unwrap_or(0);
        if ts.len() > 1 && self.res.styles.compat == Compat::Legacy && left_margins.iter().any(|m| (m - left_margins[0]).abs() > 5) {
            for (r, m) in rows.iter_mut().zip(&left_margins) {
                r.nudge = Some(*m == widest);
            }
        }
        let b = &tbl.borders;
        Table {
            style_id,
            width: first_row_width.or(tbl.width).unwrap_or(Width::Auto),
            jc: first_row_jc
                .filter(|_| self.res.styles.compat == Compat::Generic && tbl.floating.is_none())
                .or(tbl.jc)
                .or(row_jc.flatten())
                .unwrap_or(Jc::Left),
            own_jc: tbl.jc,
            ind: first_row_ind.or(tbl.ind).map_or(0.0, Width::dxa),
            ind_pct: first_row_ind.or(tbl.ind).and_then(Width::pct),
            // Set only by a dxa indent found on the table or up its style chain; a pct, auto or nil one is unset.
            ind_set: matches!(first_row_ind.or(tbl.ind), Some(Width::Dxa(_))),
            borders: [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()],
            fixed: tbl.fixed.unwrap_or(false),
            cell_spacing: tbl.cell_spacing.unwrap_or(0.0),
            shd: tbl.shd.clone().filter(|s| s.flat().is_some() || s.hatch().is_some()),
            look: direct.look,
            bands: (row_band, col_band),
            alt: {
                let text = |name: &str| child(t, "tblPr").and_then(|p| child(p, name)).and_then(val).unwrap_or_default().to_string();
                let alt = crate::docx::TableAlt { title: text("tblCaption"), description: text("tblDescription") };
                (alt != crate::docx::TableAlt::default()).then(|| Rc::new(alt))
            },
            floating: tbl.floating,
            bidi_visual: tbl.bidi_visual.unwrap_or(false),
            grid,
            rows,
            src: self.record.then(|| Rc::new(t.range().start..ts[ts.len() - 1].range().end)),
            id: next_para_id(),
            direct: Raw(child(t, "tblPr").map(|n| Rc::from(&n.document().input_text()[n.range()]))),
            controls: Vec::new(),
            change,
            grid_change,
        }
    }

    /// A table cell: its properties over its style region's `base`, the table's cell margins `tm`, and its content.
    #[allow(clippy::too_many_arguments, reason = "the cell, its properties' context and the content read before it")]
    #[inline(never)]
    fn cell(&self, cn: Node, base: &TcProps, tm: &Margins, unset: f64, blocks: Vec<Block>, controls: Vec<ControlId>, rev: CellRevisions) -> Cell {
        let tc_pr = child(cn, "tcPr");
        let mut tcp = base.clone();
        tcp.merge(&TcProps::parse(tc_pr, &self.res.styles.colours));
        let mut c = Cell {
            width: tcp.width,
            span: tcp.grid_span.unwrap_or(1).max(1) as usize,
            v_merge: match tcp.v_merge {
                Some(true) => VMerge::Restart,
                Some(false) => VMerge::Continue,
                None => VMerge::None,
            },
            top: None,
            left: None,
            bottom: None,
            right: None,
            shading: None,
            hatch: None,
            margins: [0.0; 4],
            v_align: String::new(),
            text_direction: String::new(),
            no_wrap: false,
            fit_text: false,
            hide_mark: tc_pr.and_then(|p| child(p, "hideMark")).is_some_and(|n| !matches!(attr(n, "val"), Some("0" | "false" | "off"))),
            diagonals: None,
            direct: Raw(tc_pr.map(|n| Rc::from(&n.document().input_text()[n.range()]))),
            blocks,
            controls,
            rev,
            ids: cell_ids(cn, tc_pr),
        };
        style_cell(&mut c, &tcp, tm, unset);
        c
    }

    /// A table row: its properties `trp` and exceptions `ex` over the table's `tbl` (`row_tbl` with the exceptions),
    /// and its cells.
    #[allow(clippy::too_many_arguments, reason = "the row, the table properties it resolves over and its cells")]
    #[inline(never)]
    fn row(
        &mut self,
        rn: Node,
        trp: &TrProps,
        ex: &TblProps,
        tbl: &TblProps,
        row_tbl: &TblProps,
        style_region: &Region,
        cells: Vec<Cell>,
        controls: Vec<ControlId>,
    ) -> Row {
        let tr_pr = child(rn, "trPr");
        let rev = RowRevisions {
            ins: self.mark_at(tr_pr, "ins", MarkKind::Insert),
            del: self.mark_at(tr_pr, "del", MarkKind::Delete),
            change: self.table_change(tr_pr, "trPrChange", |_, before| {
                let mut tr = style_region.tr.clone();
                tr.merge(&TrProps::parse(before));
                Some(PreviousProps::Row { height: tr.height, cant_split: tr.cant_split.unwrap_or(false), header: tr.header.unwrap_or(false) })
            }),
            ex_change: self.table_change(child(rn, "tblPrEx"), "tblPrExChange", |_, _| None),
        };
        Row {
            height: trp.height.clone(),
            cant_split: trp.cant_split.unwrap_or(false),
            header: trp.header.unwrap_or(false),
            grid_before: trp.grid_before.unwrap_or(0).max(0) as usize,
            grid_after: trp.grid_after.unwrap_or(0).max(0) as usize,
            w_before: trp.w_before,
            w_after: trp.w_after,
            ind: ex.ind.map(Width::dxa).filter(|_| ex.fixed == Some(true) && tbl.fixed != Some(true)),
            jc: trp.jc,
            div: tr_pr
                .and_then(|p| child(p, "divId"))
                .and_then(|n| int(val(n)))
                .map(|id| Rc::new(RowDiv { id, shift: 0.0, lead: 0.0, trail: 0.0, boxes: Rc::from([]) })),
            shading: ex.shading.flatten(),
            hatch: ex.shd.as_ref().and_then(Shd::hatch),
            table_shading: None,
            cell_spacing: trp.cell_spacing.or(ex.cell_spacing).map(|s| s.max(0.0)),
            run: (ex.fixed == Some(true) && tbl.fixed != Some(true)).then(|| ex.width.or(tbl.width).unwrap_or(Width::Auto)),
            nudge: None,
            hidden: tr_pr.and_then(|p| child(p, "hidden")).is_some_and(on_off),
            vanished: tr_pr.and_then(|p| child(p, "hidden")).is_some_and(|n| !matches!(attr(n, "val"), Some("0" | "false" | "off")))
                && cells.iter().all(|c| {
                    c.blocks.iter().all(|b| match b {
                        Block::Para(p) => p.mark.hidden && p.inlines.iter().all(|i| i.run().is_none_or(|r| r.hidden)),
                        Block::Table(_) => false,
                    })
                }),
            borders: (ex.borders != Borders::default()).then(|| {
                let b = &row_tbl.borders;
                Rc::new([b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()])
            }),
            cells,
            controls,
            direct: Raw(tr_pr.filter(|_| self.record).map(raw)),
            exceptions: Raw(child(rn, "tblPrEx").filter(|_| self.record).map(raw)),
            src: self.record.then(|| Rc::new(rn.range())),
            rev,
        }
    }

    /// A cell's tracked insertion, deletion or merge (`w:cellIns`, `w:cellDel`, `w:cellMerge`), and its property
    /// change, its earlier properties resolved over `base` (the cell's style region) and the table's cell margins `tm`.
    fn cell_revisions(&mut self, tc_pr: Option<Node>, base: &TcProps, tm: &Margins, unset: f64) -> CellRevisions {
        let merge = |v: Option<&str>| match v {
            Some("rest") => Some(VMerge::Restart),
            Some("cont") => Some(VMerge::Continue),
            _ => None,
        };
        let found = tc_pr.into_iter().flat_map(|p| p.children()).filter(|c| c.tag_name().namespace() == Some(W)).find_map(|c| {
            let kind = match c.tag_name().name() {
                "cellIns" => CellMarkKind::Insert,
                "cellDel" => CellMarkKind::Delete,
                "cellMerge" => CellMarkKind::Merge { v_merge: merge(attr(c, "vMerge")), v_merge_orig: merge(attr(c, "vMergeOrig")) },
                _ => return None,
            };
            Some((c, kind))
        });
        let structural = found.map(|(e, kind)| {
            let mark_kind = match kind {
                CellMarkKind::Insert => MarkKind::Insert,
                CellMarkKind::Delete => MarkKind::Delete,
                CellMarkKind::Merge { .. } => MarkKind::Format,
            };
            (self.mark(e, mark_kind), kind)
        });
        let change = self.table_change(tc_pr, "tcPrChange", |r, before| {
            let mut tcp = base.clone();
            tcp.merge(&TcProps::parse(before, &r.res.styles.colours));
            let b = &tcp.borders;
            Some(PreviousProps::Cell {
                width: tcp.width,
                borders: [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone()],
                shading: tcp.shading.flatten(),
                margins: cell_margins(&tcp, tm, unset),
                v_align: tcp.v_align.clone().unwrap_or_else(|| "top".into()),
                text_direction: tcp.text_direction.clone().unwrap_or_else(|| "lrTb".into()),
                no_wrap: tcp.no_wrap.unwrap_or(false),
            })
        });
        CellRevisions { structural, change }
    }
}

/// A table's `w:tblGrid` column widths.
fn table_grid(t: Node) -> Vec<f64> {
    child(t, "tblGrid").map(|g| children(g, "gridCol").map(|c| attr_twips(c, "w").unwrap_or(0.0)).collect()).unwrap_or_default()
}

/// The grid of tables joined into one: every table's column edges, from the table's start (to the nearest twip).
/// `rows` (from table `row_table[i]` of `grids`) get their gaps and cells' spans over it; columns a row has past its
/// own grid's end come after the merged grid's. A later table with a move `moves[k] > 0` lays each row of dxa cells
/// out from the table's left edge instead, each cell's end moving right onto the next earlier edge while the row's
/// total move stays within it.
fn merged_grid(grids: &[Vec<f64>], rows: &mut [Row], row_table: &[usize], moves: &[i64]) -> Vec<f64> {
    let mut edges: Vec<Vec<i64>> = grids
        .iter()
        .map(|g| {
            std::iter::once(0)
                .chain(g.iter().scan(0.0, |x, w| {
                    *x += w;
                    Some((*x * 20.0).round() as i64)
                }))
                .collect()
        })
        .collect();
    let mut own: Vec<Option<Vec<i64>>> = vec![None; rows.len()];
    for k in 1..grids.len() {
        let d = moves.get(k).copied().unwrap_or(0);
        let ri: Vec<usize> = (0..rows.len()).filter(|&i| row_table[i] == k).collect();
        let dxa = |r: &Row| r.grid_before == 0 && r.cells.iter().all(|c| matches!(c.width, Some(Width::Dxa(_))));
        if d <= 0 || ri.is_empty() || !ri.iter().all(|&i| dxa(&rows[i])) {
            continue;
        }
        let mut earlier: Vec<i64> = edges[..k].iter().flatten().copied().collect();
        earlier.sort_unstable();
        let mut kept = Vec::new();
        for &i in &ri {
            let (mut x, mut moved) = (0i64, 0i64);
            let mut row = vec![0];
            for c in &rows[i].cells {
                let mut end = x + (c.width.map_or(0.0, Width::dxa) * 20.0).round() as i64;
                if let Some(&a) = earlier.iter().find(|&&a| a >= end)
                    && moved + a - end <= d
                {
                    moved += a - end;
                    end = a;
                }
                x = end;
                row.push(end);
            }
            kept.extend(row.iter().copied());
            own[i] = Some(row);
        }
        edges[k] = kept;
    }
    let mut all: Vec<i64> = edges.iter().flatten().copied().collect();
    all.sort_unstable();
    all.dedup();
    let index = |e: i64| all.binary_search(&e).unwrap_or(0);
    for (r, row) in rows.iter_mut().zip(&own) {
        if let Some(row) = row {
            for (c, w) in r.cells.iter_mut().zip(row.windows(2)) {
                c.span = index(w[1]) - index(w[0]);
            }
            r.grid_after = 0;
        }
    }
    let at = |k: usize, c: usize| match edges[k].get(c) {
        Some(e) => all.binary_search(e).unwrap_or(0),
        None => all.len() + c - edges[k].len(),
    };
    for ((r, &k), moved) in rows.iter_mut().zip(row_table).zip(&own) {
        if moved.is_some() {
            continue;
        }
        let mut c = r.grid_before;
        r.grid_before = at(k, c);
        for cell in &mut r.cells {
            let from = at(k, c);
            c += cell.span;
            cell.span = at(k, c) - from;
        }
        let end = at(k, c);
        r.grid_after = at(k, c + r.grid_after) - end;
    }
    all.windows(2).map(|w| (w[1] - w[0]) as f64 / 20.0).collect()
}

/// A cell's properties a table style can give, from `tcp` (its own over its region's) and the cell margins `tm` its
/// row has (`unset` for a side neither sets).
pub(crate) fn style_cell(c: &mut Cell, tcp: &TcProps, tm: &Margins, unset: f64) {
    let b = &tcp.borders;
    (c.top, c.left, c.bottom, c.right) = (b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone());
    c.shading = tcp.shading.flatten();
    c.hatch = tcp.shd.as_ref().and_then(Shd::hatch);
    c.margins = cell_margins(tcp, tm, unset);
    c.v_align = tcp.v_align.clone().unwrap_or_else(|| "top".into());
    c.text_direction = tcp.text_direction.clone().unwrap_or_else(|| "lrTb".into());
    c.no_wrap = tcp.no_wrap.unwrap_or(false);
    c.fit_text = tcp.fit_text.unwrap_or(false);
    c.diagonals = (b.tl2br.is_some() || b.tr2bl.is_some()).then(|| Rc::new([b.tl2br.clone(), b.tr2bl.clone()]));
}

fn cell_margins(tcp: &TcProps, tm: &Margins, unset: f64) -> [f64; 4] {
    let m = |v: Option<f64>, d: Option<f64>| v.or(d).unwrap_or(0.0);
    let side = |v: Option<f64>, d: Option<f64>| v.or(d).unwrap_or(unset);
    [m(tcp.margins.top, tm.top), side(tcp.margins.left, tm.left), m(tcp.margins.bottom, tm.bottom), side(tcp.margins.right, tm.right)]
}

fn position(p: Option<Node>) -> (String, Option<String>, f64) {
    let Some(p) = p else { return ("column".into(), None, 0.0) };
    let rel = p.attribute("relativeFrom").unwrap_or("column").to_string();
    let align = p.children().find(|c| is(*c, WP, "align")).and_then(|a| a.text()).map(|s| s.trim().to_string());
    // F6: offsets are truncated to whole twips.
    let off = p
        .children()
        .find(|c| is(*c, WP, "posOffset"))
        .and_then(|o| o.text())
        .and_then(|t| t.trim().parse::<f64>().ok())
        .map(|emu| (emu / 635.0).trunc() / 20.0)
        .unwrap_or(0.0);
    (rel, align, off)
}

/// The paragraph's style formatting `style` with `run`'s hiding and revisions.
fn on_style(style: &Run, run: &Run) -> Run {
    Rc::new(RunStyle { hidden: run.hidden, rev: run.rev.clone(), ..(**style).clone() })
}

/// A Word 2016 symbol (`w16se:symEx` in an `mc:AlternateContent`): its character and font.
fn sym_ex(ac: Node) -> Option<(char, String)> {
    const W16SE: &str = "http://schemas.microsoft.com/office/word/2015/wordml/symex";
    let s = ac.descendants().find(|d| is(*d, W16SE, "symEx"))?;
    let ch = s.attribute((W16SE, "char")).and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32)?;
    Some((ch, s.attribute((W16SE, "font")).unwrap_or("").to_string()))
}

fn push_text(out: &mut Vec<Inline>, t: &str, run: &Run) {
    if let Some(Inline::Text(s, r)) = out.last_mut()
        && (Rc::ptr_eq(r, run) || (**r == **run && r.direct.0 == run.direct.0))
    {
        s.push_str(t);
        return;
    }
    out.push(Inline::Text(t.to_string(), run.clone()));
}

/// Takes the range markers `gone` names out of `blocks`, text boxes and table cells included.
fn strip_ranges(blocks: &mut [Block], gone: &[(RangeKind, i64)]) {
    if gone.is_empty() {
        return;
    }
    for b in blocks {
        match b {
            Block::Para(p) => {
                p.inlines.retain(|i| !matches!(i, Inline::Range(m) if gone.contains(&(m.kind, m.id))));
                for i in &mut p.inlines {
                    if let Some(tb) = i.story_mut() {
                        strip_ranges(&mut tb.blocks, gone);
                    }
                }
            }
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).for_each(|c| strip_ranges(&mut c.blocks, gone)),
        }
    }
}

/// Keeps new ids past every `w:id` (revisions, comments and ranges share them), `w14:paraId` and
/// `w16cid:durableId` in a part's XML. Other attributes named `id` only push new ids further on.
fn seed_ids(xml: &str, ids: &mut IdCounter) {
    let b = xml.as_bytes();
    // Each `=` with the attribute name before it: `w:id`, not `rsid`.
    for (eq, _) in xml.match_indices('=') {
        let end = b[..eq].iter().rposition(|c| !c.is_ascii_whitespace()).map_or(0, |k| k + 1);
        let start = b[..end].iter().rposition(|c| !c.is_ascii_alphanumeric()).map_or(0, |k| k + 1);
        if start == 0 || !matches!(b[start - 1], b':' | b' ' | b'\t' | b'\n' | b'\r') {
            continue;
        }
        let para = match &b[start..end] {
            b"id" => false,
            b"paraId" | b"durableId" => true,
            _ => continue,
        };
        let rest = xml[eq + 1..].trim_start();
        let Some(quote) = rest.chars().next().filter(|q| matches!(q, '"' | '\'')) else { continue };
        let v = &rest[1..];
        let Some(v) = v.find(quote).map(|end| v[..end].trim()) else { continue };
        if para {
            if let Ok(id) = u32::from_str_radix(v, 16) {
                ids.saw_para(id);
            }
        } else if let Ok(id) = v.parse::<i64>() {
            ids.saw_revision(id);
        }
    }
}

pub fn is_symbol_font(f: &str) -> bool {
    matches!(f, "Symbol" | "Wingdings" | "Wingdings 2" | "Wingdings 3" | "Webdings")
}

/// A `w:sdtPr` as [`ControlProps`]: what the editor reads and changes, typed; every
/// other child kept as its XML.
fn control_props(pr: Option<Node>, xml: &str) -> ControlProps {
    let mut p = ControlProps::default();
    let Some(pr) = pr else {
        p.kind = ControlKind::RichText;
        return p;
    };
    let mut kind = None;
    for c in pr.children().filter(|c| c.is_element()) {
        let raw = || xml[c.range()].to_string();
        let name = c.tag_name().name();
        match name {
            "tag" => p.tag = attr(c, "val").map(str::to_string),
            "alias" => p.alias = attr(c, "val").map(str::to_string),
            "lock" => p.lock = attr(c, "val").map_or(Lock::None, Lock::parse),
            "showingPlcHdr" => p.showing_placeholder = attr(c, "val").is_none_or(|v| v != "0" && v != "false"),
            "placeholder" => p.placeholder = Some(raw()),
            "dataBinding" => p.binding = Some(raw()),
            "text" => {
                let multi_line = attr(c, "multiLine").is_some_and(|v| v == "1" || v == "true");
                kind = Some((ControlKind::PlainText { multi_line }, raw()));
            }
            "richText" => kind = Some((ControlKind::RichText, raw())),
            "picture" => kind = Some((ControlKind::Picture, raw())),
            "checkbox" if c.tag_name().namespace() == Some(W14) => {
                let state = |name: &str, d: char| {
                    c.children()
                        .find(|s| s.tag_name().name() == name)
                        .and_then(|s| s.attribute((W14, "val")))
                        .and_then(|v| u32::from_str_radix(v, 16).ok())
                        .and_then(char::from_u32)
                        .unwrap_or(d)
                };
                let (checked, unchecked) = (state("checkedState", '\u{2612}'), state("uncheckedState", '\u{2610}'));
                kind = Some((ControlKind::Checkbox { checked, unchecked }, raw()))
            }
            "date" => {
                let format = child(c, "dateFormat").and_then(|f| attr(f, "val")).unwrap_or("").to_string();
                let lid = child(c, "lid").and_then(|l| attr(l, "val")).map(str::to_string);
                kind = Some((ControlKind::Date { format, lid }, raw()));
            }
            "dropDownList" | "comboBox" => kind = Some((ControlKind::DropDown, raw())),
            "group" => kind = Some((ControlKind::Group, raw())),
            "repeatingSection" if c.tag_name().namespace() == Some(W15) => kind = Some((ControlKind::RepeatingSection, raw())),
            "repeatingSectionItem" if c.tag_name().namespace() == Some(W15) => kind = Some((ControlKind::RepeatingItem, raw())),
            // Kinds the editor doesn't know (bibliography, citation, equation, document part galleries, ...).
            "docPartObj" | "docPartList" | "bibliography" | "citation" | "equation" => kind = Some((ControlKind::Other, raw())),
            _ => p.rest.push((name.to_string(), raw())),
        }
    }
    // No kind element: a rich-text control (Word's default).
    let (k, x) = kind.unwrap_or((ControlKind::RichText, String::new()));
    p.kind = k;
    p.kind_xml = (!x.is_empty()).then_some(x);
    p
}

/// The custom XML parts the main part relates to, in relationship order. A part that isn't well-formed
/// is left out (the package keeps it).
fn data_parts(pkg: &mut Package, rels: &HashMap<String, (String, String)>) -> Vec<DataPart> {
    let mut items: Vec<(&String, &String)> = rels.iter().filter(|(_, (k, _))| k == "customXml").map(|(id, (_, t))| (id, t)).collect();
    // By the id's number (rId2 before rId10), as the writer numbers a new part after every other relationship.
    let number = |id: &str| id.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().unwrap_or(u64::MAX);
    items.sort_by(|a, b| (number(a.0), a.0).cmp(&(number(b.0), b.0)));
    let mut out = Vec::new();
    for (_, target) in items {
        let Some(xml) = pkg.read_str(target) else { continue };
        let Ok(dom) = roxmltree::Document::parse(&xml) else { continue };
        let namespace = dom.root_element().tag_name().namespace().map(str::to_string);
        let props = pkg.rels(target).into_values().find(|(k, _)| k == "customXmlProps").map(|(_, t)| t);
        let item_id = props
            .and_then(|p| pkg.read_str(&p))
            .and_then(|x| {
                let d = roxmltree::Document::parse(&x).ok()?;

                d.root_element().attributes().find(|a| a.name() == "itemID").map(|a| a.value().to_string())
            })
            .unwrap_or_default();
        out.push(DataPart { namespace, item_id, xml, src: Some(target.clone()), changed: false });
    }
    out
}

/// The document reader as the shape reader's source: images through the reader's relationships, a SmartArt
/// diagram's stored drawing through its data part's `dsp:dataModelExt/@relId`.
struct DrawingSource<'r, 'p> {
    reader: &'r mut Reader,
    pkg: &'p mut Package,
}

impl crate::drawing::read::Source for DrawingSource<'_, '_> {
    fn image(&mut self, rid: &str) -> Option<usize> {
        self.reader.image(rid, self.pkg)
    }

    fn diagram(&mut self, dm: &str) -> Option<crate::drawing::Shape> {
        let (_, data) = self.reader.rels.get(dm)?.clone();
        let xml = self.pkg.read_str(&data)?;
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let rel = doc.descendants().find(|n| n.tag_name().name() == "dataModelExt")?.attribute("relId")?.to_string();
        let (_, drawing) = self.reader.rels.get(&rel)?.clone();
        let xml = self.pkg.read_str(&drawing)?;
        // The drawing's pictures are relative to its own part: they are not looked up.
        struct NoImages;
        impl crate::drawing::read::Source for NoImages {
            fn image(&mut self, _: &str) -> Option<usize> {
                None
            }
            fn diagram(&mut self, _: &str) -> Option<crate::drawing::Shape> {
                None
            }
        }
        let mut ctx = crate::drawing::read::Ctx { source: &mut NoImages, unknown_presets: Vec::new() };
        crate::drawing::read::diagram_drawing(&xml, &mut ctx)
    }
}

/// A VML shape's effect extents, points: its bounds with its line (`pen`, its width in twips, `None` unstroked) on the
/// 96-dpi pixel grid of its position frame, past its box (`[l, t, r, b]`, twips, in that frame) on each side. Half the
/// pen, at least half a pixel, goes out on each side, the bottom a pixel further; an unstroked shape's edges go to the
/// nearest pixel.
// ponytail: an aligned shape gets none; its frame position is known only once it is placed.
fn vml_effect(pen: Option<f64>, rect: Option<[f64; 4]>) -> [f64; 4] {
    let Some([l, t, r, b]) = rect else { return [0.0; 4] };
    let px = |v: f64| v / 15.0;
    let bounds = match pen {
        Some(w) => {
            let h = (w / 30.0).max(0.5);
            [(px(l) - h).floor(), (px(t) - h).floor(), (px(r) + h).ceil(), (px(b) + h).ceil()]
        }
        None => [px(l).round(), px(t).round(), px(r).round(), px(b).round()],
    }
    .map(|v| v * 15.0);
    [l - bounds[0], t - bounds[1], bounds[2] - r, bounds[3] - b].map(|e| e.max(0.0) / 20.0)
}

/// The header cells a cell lists, at most this many are read (a write keeps the others as read).
pub(crate) const MAX_CELL_HEADERS: usize = 1024;

/// The header cells a `w:headers` lists, at most [`MAX_CELL_HEADERS`].
pub(crate) fn header_list(headers: Node) -> Vec<String> {
    children(headers, "header").filter_map(|h| val(h).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)).take(MAX_CELL_HEADERS).collect()
}

/// A cell's identifier (`w:tc/@w:id`) and its header cells' (`tcPr/w:headers`), when it has either.
fn cell_ids(tc: Node, tc_pr: Option<Node>) -> Option<Rc<crate::docx::CellIds>> {
    let id = attr(tc, "id").map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let headers = tc_pr.and_then(|p| child(p, "headers")).map(header_list).unwrap_or_default();
    (id.is_some() || !headers.is_empty()).then(|| Rc::new(crate::docx::CellIds { id, headers }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_part_refused_as_too_large_still_spends_what_it_decompressed() {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file("big.html", zip::write::SimpleFileOptions::default()).expect("starts");
        z.write_all(&[b'x'; 100]).expect("writes");
        let zip = zip::ZipArchive::new(std::io::Cursor::new(z.finish().expect("finishes").into_inner())).expect("a zip");
        let mut pkg = super::Package { zip, budget: 1000 };
        assert!(pkg.read_at_most("big.html", 10).is_none());
        assert_eq!(pkg.budget, 1000 - 11, "the part's first bytes, read to tell it is too large");
        assert!(pkg.read_at_most("big.html", 100).is_some());
        assert_eq!(pkg.budget, 1000 - 11 - 100);
    }

    #[test]
    fn a_bound_control_shows_its_data_in_its_own_run_over_its_paragraphs() {
        let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        // Bold over the paragraph's Big style, in the body and in a header with a prefix of its own.
        let control = |p: &str| {
            format!(
                r#"<{p}p><{p}pPr><{p}pStyle {p}val="Big"/></{p}pPr><{p}sdt><{p}sdtPr><{p}rPr><{p}b/></{p}rPr><{p}showingPlcHdr/><{p}dataBinding {p}prefixMappings="xmlns:d='urn:d'" {p}xpath="/d:data[1]/d:name[1]"/><{p}text/></{p}sdtPr><{p}sdtContent><{p}r><{p}t>Click</{p}t></{p}r></{p}sdtContent></{p}sdt></{p}p>"#
            )
        };
        let files = [
            (
                "[Content_Types].xml".to_string(),
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string(),
            ),
            (
                "_rels/.rels".to_string(),
                format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}/officeDocument" Target="word/document.xml"/></Relationships>"#),
            ),
            (
                "word/_rels/document.xml.rels".to_string(),
                format!(
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}/styles" Target="styles.xml"/><Relationship Id="rId2" Type="{rel}/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId3" Type="{rel}/header" Target="header1.xml"/></Relationships>"#
                ),
            ),
            (
                "word/styles.xml".to_string(),
                format!(r#"<w:styles xmlns:w="{w}"><w:style w:type="paragraph" w:styleId="Big"><w:name w:val="Big"/><w:rPr><w:sz w:val="40"/></w:rPr></w:style></w:styles>"#),
            ),
            ("customXml/item1.xml".to_string(), r#"<data xmlns="urn:d"><name>Ada</name></data>"#.to_string()),
            ("word/header1.xml".to_string(), format!(r#"<h:hdr xmlns:h="{w}">{}</h:hdr>"#, control("h:"))),
            (
                "word/document.xml".to_string(),
                format!(
                    r#"<w:document xmlns:w="{w}" xmlns:r="{rel}"><w:body>{}<w:sectPr><w:headerReference w:type="default" r:id="rId3"/></w:sectPr></w:body></w:document>"#,
                    control("w:")
                ),
            ),
        ];
        let files: Vec<(&str, &str)> = files.iter().map(|(n, x)| (n.as_str(), x.as_str())).collect();
        let bytes = crate::layout::tests::package_bytes(&files).expect("a package");
        for doc in [crate::docx::read(&bytes).expect("reads"), crate::docx::read_editable(&bytes).expect("reads for editing")] {
            let header = doc.stories.values().next().expect("the header");
            for (part, blocks) in [("body", &doc.sections[0].blocks), ("header", header)] {
                let Some(crate::docx::Block::Para(p)) = blocks.first() else { panic!("a paragraph") };
                let run = p.inlines.iter().find_map(|i| if let crate::docx::Inline::Text(t, r) = i { (t == "Ada").then_some(r) } else { None });
                let run = run.unwrap_or_else(|| panic!("{part}: the data shows: {:?}", p.inlines));
                assert!(run.bold && (run.size - 20.0).abs() < 1e-9, "{part}: bold at the style's size: {} {}", run.bold, run.size);
            }
        }
    }

    #[test]
    fn ids_are_seeded_from_whole_attribute_names() {
        let mut ids = super::IdCounter::default();
        let xml = r#"<w:ins w:id = '41' w:rsid="99"><w:p w14:paraId="0000001F" w16cid:durableId="7FFFFFFE x"/><w:c id="7"/></w:ins>"#;
        super::seed_ids(xml, &mut ids);
        assert_eq!(ids.next_revision_id(), 42, "w:id with spaces and single quotes; not rsid");
        assert_eq!(ids.next_para_id(), 0x20, "paraId; a durableId that isn't hex is skipped");
    }

    #[test]
    fn a_vml_shapes_effect_extents_reach_its_lines_pixel_bounds() {
        let tw = |e: [f64; 4]| e.map(|v| (v * 20.0 * 1000.0).round() / 1000.0);
        assert_eq!(tw(super::vml_effect(Some(15.0), Some([2002.0, 0.0, 3442.0, 800.0]))), [22.0, 15.0, 8.0, 10.0]);
        assert_eq!(tw(super::vml_effect(Some(15.0), Some([2007.0, 0.0, 3459.0, 800.0])))[2], 21.0);
        assert_eq!(tw(super::vml_effect(Some(60.0), Some([2000.0, 0.0, 3440.0, 800.0])))[2], 40.0);
        // A 3pt line whose bottom edge lands on the pixel grid: rounded up to it, not a pixel past.
        assert_eq!(tw(super::vml_effect(Some(60.0), Some([2000.0, 0.0, 3440.0, 795.0])))[3], 30.0);
        assert_eq!(tw(super::vml_effect(None, Some([2002.0, 0.0, 3442.0, 800.0]))), [7.0, 0.0, 0.0, 0.0]);
        assert_eq!(super::vml_effect(Some(15.0), None), [0.0; 4]);
    }

    #[test]
    fn a_hyperlinks_target_and_anchor_keep_their_backslashes() {
        let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let w = format!(r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="{rel}""#);
        let target = r"file:///\\server\share\a\";
        let link = |id: &str| format!(r#"<w:p><w:hyperlink r:id="{id}" w:anchor="b\&quot;"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>"#);
        let doc = crate::layout::tests::package(&[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                &format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}/officeDocument" Target="word/document.xml"/></Relationships>"#),
            ),
            (
                "word/_rels/document.xml.rels",
                &format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{rel}/hyperlink" Target="{target}" TargetMode="External"/></Relationships>"#),
            ),
            ("word/document.xml", &format!("<w:document {w}><w:body>{}</w:body></w:document>", link("rId9"))),
        ])
        .expect("reads");
        let Some(crate::docx::Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let span = *crate::docx::fields::spans(&p.inlines).first().expect("the link's field");
        let code = crate::docx::fields::parse(&crate::docx::fields::code_text(&p.inlines, span));
        assert_eq!(code.args, [target]);
        assert_eq!(code.switches.iter().find(|s| s.flag == 'l').and_then(|s| s.arg.as_deref()), Some("b\\\""));
    }
}
