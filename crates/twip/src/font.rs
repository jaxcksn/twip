//! Fonts: a catalog over font files (our own matching over the name/OS/2 tables, so the same files give the same faces on
//! every platform), vertical metrics (FONT-1) and HarfBuzz shaping (harfrust; FONT-2–FONT-4).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag};

#[derive(Clone, Debug)]
struct Entry {
    family: String,
    weight: u16,
    width: u16,
    italic: bool,
    path: Option<String>,
    data: Option<Arc<Vec<u8>>>,
    index: u32,
}

/// One face, with metrics in font units.
pub struct Face {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub sim_bold: bool,
    pub sim_italic: bool,
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    pub upem: f64,
    /// FONT-1: ascent A, descent D, external leading E.
    pub ascent: f64,
    pub descent: f64,
    pub leading: f64,
    pub sup_size: f64,
    pub sup_offset: f64,
    pub sub_size: f64,
    pub sub_offset: f64,
    pub underline_pos: f64,
    pub underline_thick: f64,
    pub strike_pos: f64,
    pub strike_thick: f64,
    /// An East Asian font: OS/2 code pages include Japanese, Chinese or Korean (FONT-12).
    pub east_asian: bool,
    /// OS/2 xAvgCharWidth, em fraction (0 without OS/2): balanced space runs (FONT-14).
    pub avg_width: f64,
    font: harfrust::Font,
    /// Memos: character → glyph, glyph → advance, and shape plans by (kern, ligatures, script). Building a plan
    /// collects the font's GSUB/GPOS lookups, far more work than shaping a word.
    glyphs: RefCell<HashMap<char, Option<u32>>>,
    advances: RefCell<HashMap<u32, f64>>,
    plans: RefCell<HashMap<(bool, u8, Option<harfrust::Script>), Rc<harfrust::ShapePlan>>>,
}

/// A shaped glyph: advances and offsets in font units, `cluster` = byte offset in the shaped text.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub id: u32,
    pub cluster: u32,
    pub advance: f64,
    pub dx: f64,
    pub dy: f64,
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
}
fn i16_at(d: &[u8], o: usize) -> i16 {
    u16_at(d, o) as i16
}
fn u32_at(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Table offsets of face `index` (a TTC holds several).
fn tables(d: &[u8], index: u32) -> HashMap<[u8; 4], (usize, usize)> {
    let mut out = HashMap::new();
    let off = if d.starts_with(b"ttcf") { u32_at(d, 12 + 4 * index as usize) as usize } else { 0 };
    let n = u16_at(d, off + 4) as usize;
    for i in 0..n {
        let r = off + 12 + 16 * i;
        let Some(tag) = d.get(r..r + 4) else { break };
        out.insert([tag[0], tag[1], tag[2], tag[3]], (u32_at(d, r + 8) as usize, u32_at(d, r + 12) as usize));
    }
    out
}

impl Face {
    fn load(e: &Entry, data: Arc<Vec<u8>>, sim_bold: bool, sim_italic: bool) -> Option<Face> {
        let d = &data[..];
        let t = tables(d, e.index);
        let head = t.get(b"head")?.0;
        let upem = (u16_at(d, head + 18).max(16)) as f64;
        let (mut asc, mut desc, mut gap) = (0.0, 0.0, 0.0);
        let (h_asc, h_desc, h_gap) = match t.get(b"hhea") {
            Some(&(o, _)) => (i16_at(d, o + 4) as f64, i16_at(d, o + 6) as f64, i16_at(d, o + 8) as f64),
            None => (0.0, 0.0, 0.0),
        };
        let mut f = Face {
            family: e.family.clone(),
            weight: e.weight,
            italic: e.italic,
            sim_bold,
            sim_italic,
            data: data.clone(),
            index: e.index,
            upem,
            ascent: 0.0,
            descent: 0.0,
            leading: 0.0,
            sup_size: upem * 0.65,
            sup_offset: upem * 0.35,
            sub_size: upem * 0.65,
            sub_offset: upem * 0.14,
            underline_pos: upem * 0.1,
            underline_thick: upem * 0.05,
            strike_pos: upem * 0.26,
            strike_thick: upem * 0.05,
            east_asian: false,
            avg_width: 0.0,
            font: harfrust::Font::new(Arc::clone(&data) as Arc<dyn AsRef<[u8]> + Send + Sync>, e.index)?,
            glyphs: RefCell::new(HashMap::new()),
            advances: RefCell::new(HashMap::new()),
            plans: RefCell::new(HashMap::new()),
        };
        if let Some(&(o, len)) = t.get(b"OS/2") {
            if len >= 78 {
                if u16_at(d, o + 62) & 0x80 != 0 {
                    asc = i16_at(d, o + 68) as f64;
                    desc = -(i16_at(d, o + 70) as f64);
                    gap = (i16_at(d, o + 72) as f64).max(0.0);
                } else {
                    asc = u16_at(d, o + 74) as f64;
                    desc = u16_at(d, o + 76) as f64;
                    gap = ((h_asc - h_desc + h_gap) - (asc + desc)).max(0.0);
                }
            }
            f.avg_width = i16_at(d, o + 2) as f64 / upem;
            f.sub_size = i16_at(d, o + 12) as f64;
            f.sub_offset = i16_at(d, o + 16) as f64;
            f.sup_size = i16_at(d, o + 20) as f64;
            f.sup_offset = i16_at(d, o + 24) as f64;
            f.strike_thick = i16_at(d, o + 26) as f64;
            f.strike_pos = i16_at(d, o + 28) as f64;
            f.east_asian = len >= 86 && u32_at(d, o + 78) & (0x1F << 17) != 0;

            if f.east_asian {
                // LINE-14: an East Asian font's line is 1.3 × its win height W, half the extra above, half below,
                // and no line gap (MS Gothic 6–72pt: +0.30 × size, split evenly; Yu Gothic 12pt: baseline 14.16 =
                // 11.82 + 0.15 × 15.44; SimSun as MS Gothic, its line gap unused). Line spacing scales this height.
                let (wa, wd) = (u16_at(d, o + 74) as f64, u16_at(d, o + 76) as f64);
                let extra = 0.15 * (wa + wd);
                (asc, desc, gap) = (wa + extra, wd + extra, 0.0);
            }
        } else {
            asc = h_asc;
            desc = -h_desc;
            gap = h_gap.max(0.0);
        }
        if let Some(&(o, _)) = t.get(b"post") {
            f.underline_pos = -(i16_at(d, o + 8) as f64);
            f.underline_thick = i16_at(d, o + 10) as f64;
        }
        f.ascent = asc;
        f.descent = desc;
        f.leading = gap;
        Some(f)
    }

    /// Points per font unit at `size`.
    pub fn scale(&self, size: f64) -> f64 {
        size / self.upem
    }

    pub fn glyph(&self, c: char) -> Option<u32> {
        if let Some(&g) = self.glyphs.borrow().get(&c) {
            return g;
        }
        let g = ShaperFont::new(&self.font).nominal_glyph(c as u32).map(|g| g.to_u32()).filter(|&g| g != 0);
        self.glyphs.borrow_mut().insert(c, g);
        g
    }

    pub fn has_glyph(&self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    pub fn advance(&self, glyph: u32) -> f64 {
        if let Some(&a) = self.advances.borrow().get(&glyph) {
            return a;
        }
        let a = ShaperFont::new(&self.font).glyph_h_advance(harfrust::GlyphId::new(glyph)) as f64;
        self.advances.borrow_mut().insert(glyph, a);
        a
    }

    /// A glyph's ink box horizontally, (left bearing, width) in font units; None for a glyph without ink (a space).
    pub fn ink(&self, glyph: u32) -> Option<(f64, f64)> {
        let e = ShaperFont::new(&self.font).glyph_extents(harfrust::GlyphId::new(glyph))?;
        (e.width != 0).then_some((e.x_bearing as f64, e.width as f64))
    }

    /// A glyph's ink top above the baseline, in font units (`None` for a glyph without ink).
    pub fn ink_top(&self, glyph: u32) -> Option<f64> {
        let e = ShaperFont::new(&self.font).glyph_extents(harfrust::GlyphId::new(glyph))?;
        (e.height != 0).then_some(e.y_bearing as f64)
    }

    /// Shapes `text` with HarfBuzz (font units, unrounded: FONT-2). `kern` = pair kerning on (FONT-3); `ligatures` = the
    /// `w14:ligatures` bits (FONT-4).
    pub fn shape(&self, text: &str, kern: bool, ligatures: u8) -> Vec<Glyph> {
        crate::prof::time("shape", || self.shape_uncached(text, kern, ligatures))
    }

    fn shape_uncached(&self, text: &str, kern: bool, ligatures: u8) -> Vec<Glyph> {
        let sf = ShaperFont::new(&self.font);
        let mut buf = Buffer::new();
        buf.push_str(text);
        buf.set_direction(Direction::LeftToRight);
        buf.guess_segment_properties();
        let on = |b: bool| if b { 1 } else { 0 };
        let feats = [
            Feature::new(Tag::new(b"kern"), on(kern), ..),
            Feature::new(Tag::new(b"liga"), on(ligatures & 1 != 0), ..),
            Feature::new(Tag::new(b"clig"), on(ligatures & 1 != 0), ..),
            Feature::new(Tag::new(b"calt"), on(ligatures & 2 != 0), ..),
            Feature::new(Tag::new(b"hlig"), on(ligatures & 4 != 0), ..),
            Feature::new(Tag::new(b"dlig"), on(ligatures & 8 != 0), ..),
        ];
        let key = (kern, ligatures, buf.script());
        let cached = self.plans.borrow().get(&key).cloned();
        let plan = cached.unwrap_or_else(|| {
            let plan = Rc::new(harfrust::ShapePlan::new(&self.font, buf.direction(), buf.script(), None, &feats));
            self.plans.borrow_mut().insert(key, plan.clone());
            plan
        });
        if harfrust::shape(&sf, &mut buf, ShapeOptions::new().plan(Some(&plan)).features(&feats)).is_err() {
            return Vec::new();
        }
        buf.glyph_infos()
            .iter()
            .zip(buf.glyph_positions())
            .map(|(i, p)| Glyph { id: i.glyph_id, cluster: i.cluster, advance: p.x_advance as f64, dx: p.x_offset as f64, dy: p.y_offset as f64 })
            .collect()
    }
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Face({} {}{}{})",
            self.family,
            self.weight,
            if self.italic { " italic" } else { "" },
            if self.sim_bold || self.sim_italic { " synthetic" } else { "" }
        )
    }
}

/// The fonts layout may use, and how document font names map to faces. Cloned when a font is added to a catalog
/// several engines share (`Rc::make_mut`, host::Engine::add_font).
#[derive(Default, Clone)]
pub struct Fonts {
    entries: Vec<Entry>,
    by_name: HashMap<String, Vec<usize>>,
    files: RefCell<HashMap<String, Arc<Vec<u8>>>>,
    faces: RefCell<HashMap<(String, bool, bool), Option<Rc<Face>>>>,
    resolved: RefCell<HashMap<String, String>>,
    fallbacks: RefCell<HashMap<(char, bool, bool), Option<Rc<Face>>>>,
    /// Bumped whenever faces are added: part of every layout cache key that used fonts.
    revision: u64,
}

/// Office cloud fonts often missing outside Office, and metric-compatible open fonts.
const SUBSTITUTES: &[(&str, &str)] = &[
    ("Calibri Light", "Calibri"),
    ("Aptos", "Calibri"),
    ("Aptos Display", "Calibri"),
    ("Aptos Narrow", "Arial Narrow"),
    ("Helvetica", "Arial"),
    ("Helvetica Neue", "Arial"),
    ("Times", "Times New Roman"),
    ("Courier", "Courier New"),
    ("Liberation Serif", "Times New Roman"),
    ("Liberation Sans", "Arial"),
    ("Liberation Mono", "Courier New"),
    ("DejaVu Sans", "Verdana"),
    ("Arial Unicode MS", "Segoe UI"),
    ("MS Mincho", "MS Gothic"),
    ("MS Gothic", "Yu Gothic"),
    ("Calibri", "Carlito"),
    ("Cambria", "Caladea"),
    ("Arial", "Liberation Sans"),
    ("Times New Roman", "Liberation Serif"),
    ("Courier New", "Liberation Mono"),
];

const FALLBACK_FAMILIES: &[&str] = &[
    "Segoe UI Symbol",
    "Segoe UI",
    "Arial",
    "Cambria Math",
    "MS Gothic",
    "Yu Gothic",
    "Microsoft YaHei",
    "SimSun",
    "Malgun Gothic",
    "Nirmala UI",
    "Segoe UI Emoji",
    "Noto Sans",
    "DejaVu Sans",
];

impl Fonts {
    /// The operating system's installed fonts, plus Office's cloud font folder (where Aptos and friends are installed).
    pub fn system() -> Fonts {
        let mut f = Fonts::default();
        if let Ok(win) = std::env::var("WINDIR") {
            f.add_dir(&format!("{win}\\Fonts"), false);
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            f.add_dir(&format!("{local}\\Microsoft\\Windows\\Fonts"), false);
            f.add_dir(&format!("{local}\\Microsoft\\FontCache\\4\\CloudFonts"), true);
        }
        // Linux and macOS keep fonts in nested folders, system-wide and per user.
        if cfg!(unix) {
            let home = std::env::var("HOME").unwrap_or_default();
            for dir in ["/usr/share/fonts", "/usr/local/share/fonts", "/System/Library/Fonts", "/Library/Fonts"] {
                f.add_dir(dir, true);
            }
            if !home.is_empty() {
                for dir in [".local/share/fonts", ".fonts", "Library/Fonts"] {
                    f.add_dir(&format!("{home}/{dir}"), true);
                }
            }
        }
        f
    }

    pub fn add_dir(&mut self, dir: &str, recursive: bool) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if recursive {
                    self.add_dir(&p.to_string_lossy(), true);
                }
                continue;
            }
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
                self.add_file(&p.to_string_lossy());
            }
        }
    }

    pub fn add_file(&mut self, path: &str) {
        // Only the table directories and the name/OS/2/head tables are read here; the file loads on first use.
        if let Some(sparse) = read_sparse(path) {
            self.scan(&sparse, Some(path.to_string()), None);
        }
    }

    pub fn add_bytes(&mut self, data: Vec<u8>) {
        let data = Arc::new(data);
        self.scan(&data.clone(), None, Some(data));
    }

    /// The catalog's revision: it changes whenever faces are added, so cached layouts made with the old catalog are
    /// recomputed. Unique in the process, so two catalogs (which may hold different face objects) never share one.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn scan(&mut self, d: &[u8], path: Option<String>, data: Option<Arc<Vec<u8>>>) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.revision = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let count = if d.starts_with(b"ttcf") { u32_at(d, 8) } else { 1 };
        for index in 0..count.min(64) {
            let t = tables(d, index);
            let (Some(&(no, nl)), true) = (t.get(b"name"), t.contains_key(b"cmap")) else { continue };
            let names = read_names(d.get(no..no + nl).unwrap_or(&[]));
            let Some(fam) = names.get(&1).and_then(|v| v.first()).cloned() else { continue };
            let (mut weight, mut width, mut italic) = (400u16, 5u16, false);
            if let Some(&(o, len)) = t.get(b"OS/2")
                && len >= 64
            {
                weight = u16_at(d, o + 4);
                width = u16_at(d, o + 6);
                italic = u16_at(d, o + 62) & 1 != 0;
                if (1..10).contains(&weight) {
                    weight *= 100;
                }
            }
            if let Some(&(o, _)) = t.get(b"head") {
                let mac = u16_at(d, o + 44);
                italic |= mac & 2 != 0;
                if !t.contains_key(b"OS/2") && mac & 1 != 0 {
                    weight = 700;
                }
            }
            let mut all: Vec<String> = Vec::new();
            for id in [1, 16, 21] {
                for n in names.get(&id).into_iter().flatten() {
                    if !all.iter().any(|a| a.eq_ignore_ascii_case(n)) {
                        all.push(n.clone());
                    }
                }
            }
            let i = self.entries.len();
            for n in &all {
                self.by_name.entry(n.to_lowercase()).or_default().push(i);
            }
            self.entries.push(Entry {
                family: fam,
                weight: if weight == 0 { 400 } else { weight },
                width: if (1..=9).contains(&width) { width } else { 5 },
                italic,
                path: path.clone(),
                data: data.clone(),
                index,
            });
        }
        self.faces.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        self.fallbacks.borrow_mut().clear();
    }

    /// The installed family names, sorted (for a font chooser).
    pub fn families(&self) -> Vec<String> {
        let mut out: Vec<String> = self.entries.iter().map(|e| e.family.clone()).collect();
        out.sort_by_key(|f| f.to_lowercase());
        out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        out
    }

    pub fn is_installed(&self, family: &str) -> bool {
        self.by_name.contains_key(&family.to_lowercase())
    }

    /// The installed family that renders `name`.
    pub fn resolve(&self, name: &str) -> String {
        if let Some(r) = self.resolved.borrow().get(name) {
            return r.clone();
        }
        let sub = |n: &str| SUBSTITUTES.iter().find(|(a, _)| a.eq_ignore_ascii_case(n)).map(|(_, b)| *b);
        let r = if self.is_installed(name) {
            name.to_string()
        } else if let Some(s) = sub(name).filter(|s| self.is_installed(s)) {
            s.to_string()
        } else if let Some(s2) = sub(name).and_then(sub).filter(|s| self.is_installed(s)) {
            s2.to_string()
        } else {
            ["Calibri", "Carlito", "Arial", "Liberation Sans"]
                .iter()
                .find(|f| self.is_installed(f))
                .map(|s| s.to_string())
                .or_else(|| self.entries.first().map(|e| e.family.clone()))
                .unwrap_or_else(|| name.to_string())
        };
        self.resolved.borrow_mut().insert(name.to_string(), r.clone());
        r
    }

    fn matches(&self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        let list = self.by_name.get(&family.to_lowercase())?;
        let target = if bold { 700 } else { 400 };
        list.iter().copied().min_by_key(|&i| {
            let e = &self.entries[i];
            let w = e.weight as i32;
            let wd = if bold {
                if w >= 600 { (w - target).abs() } else { 400 + target - w }
            } else if (400..=500).contains(&w) {
                w - 400
            } else if w < 400 {
                400 - w + 100
            } else {
                w - 500 + 200
            };
            let exact = e.family.eq_ignore_ascii_case(family);
            (if e.italic != italic { 10000 } else { 0 }) + (e.width as i32 - 5).abs() * 1000 + wd + if exact { 0 } else { 1 }
        })
    }

    fn data(&self, e: &Entry) -> Option<Arc<Vec<u8>>> {
        if let Some(d) = &e.data {
            return Some(d.clone());
        }
        let path = e.path.as_ref()?;
        if let Some(d) = self.files.borrow().get(path) {
            return Some(d.clone());
        }
        let d = Arc::new(std::fs::read(path).ok()?);
        self.files.borrow_mut().insert(path.clone(), d.clone());
        Some(d)
    }

    fn load(&self, i: usize, bold: bool, italic: bool) -> Option<Rc<Face>> {
        let e = &self.entries[i];
        let data = self.data(e)?;
        Face::load(e, data, bold && e.weight < 600, italic && !e.italic).map(Rc::new)
    }

    /// The face for a family and style (bold/italic synthesized when the family has no such face).
    pub fn face(&self, family: &str, bold: bool, italic: bool) -> Option<Rc<Face>> {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(f) = self.faces.borrow().get(&key) {
            return f.clone();
        }
        let i = self.matches(family, bold, italic).or_else(|| self.matches(&self.resolve(family), bold, italic));
        let f = match i {
            Some(i) => self.load(i, bold, italic),
            None => (!self.entries.is_empty()).then(|| self.load(0, bold, italic)).flatten(),
        };
        self.faces.borrow_mut().insert(key, f.clone());
        f
    }

    /// A face with a glyph for `c` when the run's font has none.
    pub fn fallback(&self, c: char, bold: bool, italic: bool) -> Option<Rc<Face>> {
        if let Some(f) = self.fallbacks.borrow().get(&(c, bold, italic)) {
            return f.clone();
        }
        let f = FALLBACK_FAMILIES
            .iter()
            .filter_map(|fam| self.matches(fam, bold, italic))
            .filter_map(|i| self.load(i, bold, italic))
            .find(|f| f.has_glyph(c));
        self.fallbacks.borrow_mut().insert((c, bold, italic), f.clone());
        f
    }
}

/// A font file's bytes with only the parts [`Fonts::scan`] reads filled in (headers, directories, name/OS/2/head).
fn read_sparse(path: &str) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;
    let mut out = vec![0u8; len];
    let fill = |f: &mut std::fs::File, out: &mut Vec<u8>, off: usize, n: usize| -> Option<()> {
        let end = off.checked_add(n)?.min(out.len());
        if off >= end {
            return Some(());
        }
        f.seek(SeekFrom::Start(off as u64)).ok()?;
        f.read_exact(&mut out[off..end]).ok()
    };
    fill(&mut f, &mut out, 0, 12)?;
    let offsets: Vec<usize> = if out.starts_with(b"ttcf") {
        let n = (u32_at(&out, 8) as usize).min(64);
        fill(&mut f, &mut out, 12, 4 * n)?;
        (0..n).map(|i| u32_at(&out, 12 + 4 * i) as usize).collect()
    } else {
        vec![0]
    };
    for off in offsets {
        fill(&mut f, &mut out, off, 12)?;
        let n = u16_at(&out, off + 4) as usize;
        fill(&mut f, &mut out, off + 12, 16 * n)?;
        for i in 0..n {
            let r = off + 12 + 16 * i;
            if matches!(out.get(r..r + 4), Some(b"name") | Some(b"OS/2") | Some(b"head")) {
                let (to, tl) = (u32_at(&out, r + 8) as usize, u32_at(&out, r + 12) as usize);
                fill(&mut f, &mut out, to, tl)?;
            }
        }
    }
    Some(out)
}

/// Name records by id: English (US) Windows names first, then other Windows languages, then Mac Roman.
fn read_names(t: &[u8]) -> HashMap<u16, Vec<String>> {
    let mut by_id: HashMap<u16, Vec<(u8, String)>> = HashMap::new();
    let count = u16_at(t, 2) as usize;
    let strings = u16_at(t, 4) as usize;
    for i in 0..count {
        let r = 6 + 12 * i;
        if r + 12 > t.len() {
            break;
        }
        let (platform, enc, lang, id) = (u16_at(t, r), u16_at(t, r + 2), u16_at(t, r + 4), u16_at(t, r + 6));
        let (len, off) = (u16_at(t, r + 8) as usize, strings + u16_at(t, r + 10) as usize);
        if !matches!(id, 1 | 6 | 16 | 21) || off + len > t.len() {
            continue;
        }
        let bytes = &t[off..off + len];
        let (name, rank) = if platform == 3 && matches!(enc, 0 | 1 | 10) {
            let u: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            (String::from_utf16_lossy(&u), if lang == 0x409 { 0 } else { 1 })
        } else if platform == 1 && enc == 0 {
            (bytes.iter().map(|&b| b as char).collect(), 2)
        } else {
            continue;
        };
        let name = name.trim_matches(|c| c == '\0' || c == ' ').to_string();
        if !name.is_empty() {
            by_id.entry(id).or_default().push((rank, name));
        }
    }
    by_id
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by_key(|x| x.0);
            let mut out: Vec<String> = Vec::new();
            for (_, n) in v {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            (k, out)
        })
        .collect()
}
