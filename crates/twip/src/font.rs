//! Fonts: a catalog over font files (our own matching over the name/OS/2 tables, so the same files give the same faces on
//! every platform), vertical metrics (FONT-1) and HarfBuzz shaping (harfrust; FONT-2–FONT-4).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag};

use crate::api::error::Error;
use crate::api::fonts::{FaceInfo, SubstitutionRule};

#[derive(Clone, Debug)]
struct Entry {
    family: String,
    weight: u16,
    width: u16,
    italic: bool,
    path: Option<String>,
    data: Option<Arc<Vec<u8>>>,
    index: u32,
    /// A variable font's named instance (FONT-17): its number from 1, and its axis values (user space). 0 and none
    /// for a static face.
    instance: u32,
    variations: Vec<([u8; 4], f32)>,
}

/// One face, with metrics in font units.
pub struct Face {
    /// The family layout asked for (the document's, before substitution).
    pub requested: String,
    /// The family drawn.
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub sim_bold: bool,
    pub sim_italic: bool,
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    /// A variable font's named instance, from 1 (0 for a static face), and its axis values: what the face draws
    /// with (FONT-17).
    pub instance: u32,
    pub variations: Vec<([u8; 4], f32)>,
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
    fn load(e: &Entry, requested: &str, data: Arc<Vec<u8>>, sim_bold: bool, sim_italic: bool) -> Option<Face> {
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
            requested: requested.to_string(),
            family: e.family.clone(),
            weight: e.weight,
            italic: e.italic,
            sim_bold,
            sim_italic,
            data: data.clone(),
            index: e.index,
            instance: e.instance,
            variations: e.variations.clone(),
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
            font: {
                let font = harfrust::Font::new(Arc::clone(&data) as Arc<dyn AsRef<[u8]> + Send + Sync>, e.index)?;
                if e.variations.is_empty() {
                    font
                } else {
                    // shortcut: vertical metrics stay the default instance's (from OS/2 and hhea, not MVAR); fonts
                    // rarely vary them.
                    font.instance_builder().variations(e.variations.iter().map(|&(t, v)| (Tag::new(&t), v))).build()
                }
            },
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

/// The fonts layout may use, and how document font names map to faces: the substitution rules first, then the
/// installed families, then the built-in substitutes, then the default family.
#[derive(Default, Clone)]
pub struct Fonts {
    entries: Vec<Entry>,
    by_name: HashMap<String, Vec<usize>>,
    rules: Vec<SubstitutionRule>,
    default: Option<String>,
    files: RefCell<HashMap<String, Arc<Vec<u8>>>>,
    faces: RefCell<HashMap<(String, bool, bool), Option<Rc<Face>>>>,
    resolved: RefCell<HashMap<(String, bool, bool), String>>,
    fallbacks: RefCell<HashMap<(char, bool, bool), Option<Rc<Face>>>>,
    /// Bumped whenever faces, rules or the default change: part of every layout cache key that used fonts.
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
    // As Word draws them without a font table entry.
    ("CourierPS", "Courier New"),
    ("Courier New CE", "Courier New"),
    ("Myriad Pro", "Segoe UI"),
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

    /// Adds a font file's faces: TrueType, OpenType, a collection, or WOFF and WOFF2 (decoded to TrueType or
    /// OpenType first). Fails, adding nothing, for data that isn't one or has no usable face.
    pub fn add_bytes(&mut self, data: Vec<u8>) -> Result<Vec<FaceInfo>, Error> {
        let data = match data.get(..4) {
            Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf") => data,
            Some(b"wOFF") => wuff::decompress_woff1(&data).map_err(|e| Error::invalid(format!("not a valid WOFF font: {e:?}")))?,
            Some(b"wOF2") => wuff::decompress_woff2(&data).map_err(|e| Error::invalid(format!("not a valid WOFF2 font: {e:?}")))?,
            _ => return Err(Error::invalid("not a TrueType, OpenType or WOFF font")),
        };
        let from = self.entries.len();
        let data = Arc::new(data);
        self.scan(&data.clone(), None, Some(data));
        let faces: Vec<FaceInfo> =
            self.entries[from..].iter().map(|e| FaceInfo { family: e.family.clone(), bold: e.weight >= 600, italic: e.italic }).collect();
        if faces.is_empty() {
            return Err(Error::invalid("the font file has no usable faces"));
        }
        Ok(faces)
    }

    /// The catalog's revision: it changes whenever faces, rules or the default do, so cached layouts made with the old
    /// catalog are recomputed. Unique in the process, so two catalogs (which may hold different face objects) never
    /// share one.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A new revision, and the memos made with the old one dropped.
    fn changed(&mut self) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.revision = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.faces.borrow_mut().clear();
        self.resolved.borrow_mut().clear();
        self.fallbacks.borrow_mut().clear();
    }

    /// The substitution rules, replacing earlier ones.
    pub fn set_rules(&mut self, rules: Vec<SubstitutionRule>) {
        self.rules = rules;
        self.changed();
    }

    /// The family drawn when nothing else resolves a name. Until one is set: Calibri or a metric-compatible family
    /// when installed, else the first font added.
    pub fn set_default(&mut self, family: &str) {
        self.default = Some(family.to_string());
        self.changed();
    }

    fn scan(&mut self, d: &[u8], path: Option<String>, data: Option<Arc<Vec<u8>>>) {
        let before = self.entries.len();
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
            let entry = Entry {
                family: fam,
                weight: if weight == 0 { 400 } else { weight },
                width: if (1..=9).contains(&width) { width } else { 5 },
                italic,
                path: path.clone(),
                data: data.clone(),
                index,
                instance: 0,
                variations: Vec::new(),
            };
            let table = |tag: &[u8; 4]| t.get(tag).and_then(|&(o, len)| d.get(o..o + len));
            let instances = table(b"fvar").map(|fvar| named_instances(fvar, table(b"STAT"), &names, &entry)).unwrap_or_default();
            for (entry, aliases) in if instances.is_empty() { vec![(entry, Vec::new())] } else { instances } {
                // Added bytes are loaded here as layout would load them, so a face that can't be drawn isn't added.
                // Files load on first use.
                if let Some(data) = &data
                    && Face::load(&entry, &entry.family, data.clone(), false, false).is_none()
                {
                    continue;
                }
                let i = self.entries.len();
                for n in all.iter().chain(&aliases) {
                    let list = self.by_name.entry(n.to_lowercase()).or_default();
                    if !list.contains(&i) {
                        list.push(i);
                    }
                }
                self.entries.push(entry);
            }
        }
        if self.entries.len() > before {
            self.changed();
        }
    }

    /// How many faces the catalog has.
    pub fn face_count(&self) -> usize {
        self.entries.len()
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

    /// The family a substitution rule draws `name` in, else `name`. Matching is case-insensitive, and a rule with
    /// `bold` or `italic` set applies only to that style.
    pub fn render_family<'a>(&'a self, name: &'a str, bold: bool, italic: bool) -> &'a str {
        self.rules
            .iter()
            .find(|r| r.from.eq_ignore_ascii_case(name) && r.bold.is_none_or(|b| b == bold) && r.italic.is_none_or(|i| i == italic))
            .map_or(name, |r| r.to.as_str())
    }

    /// The installed family that draws `name`: itself, or a built-in substitute.
    fn installed_for(&self, name: &str) -> Option<String> {
        let sub = |n: &str| SUBSTITUTES.iter().find(|(a, _)| a.eq_ignore_ascii_case(n)).map(|(_, b)| *b);
        if self.is_installed(name) {
            Some(name.to_string())
        } else if let Some(s) = sub(name).filter(|s| self.is_installed(s)) {
            Some(s.to_string())
        } else {
            sub(name).and_then(sub).filter(|s| self.is_installed(s)).map(str::to_string)
        }
    }

    /// Whether `name` in this style draws without falling back to the default: its rule's family, or else `name`, is
    /// installed or has an installed built-in substitute.
    pub fn resolves(&self, name: &str, bold: bool, italic: bool) -> bool {
        self.installed_for(self.render_family(name, bold, italic)).is_some()
    }

    /// The installed family that renders `name` in this style: its rule's family, `name` or a built-in substitute,
    /// then the default.
    fn resolve(&self, name: &str, bold: bool, italic: bool) -> String {
        let key = (name.to_string(), bold, italic);
        if let Some(r) = self.resolved.borrow().get(&key) {
            return r.clone();
        }
        let target = self.render_family(name, bold, italic);
        let r = self
            .installed_for(target)
            // A rule's family not added yet: the document's family draws meanwhile.
            .or_else(|| if target != name { self.installed_for(name) } else { None })
            .or_else(|| self.default.clone().filter(|d| self.is_installed(d)))
            .or_else(|| ["Calibri", "Carlito", "Arial", "Liberation Sans"].iter().find(|f| self.is_installed(f)).map(|s| s.to_string()))
            .or_else(|| self.entries.first().map(|e| e.family.clone()))
            .unwrap_or_else(|| name.to_string());
        self.resolved.borrow_mut().insert(key, r.clone());
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

    fn load(&self, i: usize, requested: &str, bold: bool, italic: bool) -> Option<Rc<Face>> {
        let e = &self.entries[i];
        let data = self.data(e)?;
        Face::load(e, requested, data, bold && e.weight < 600, italic && !e.italic).map(Rc::new)
    }

    /// The face for a family and style (bold/italic synthesized when the family has no such face).
    pub fn face(&self, family: &str, bold: bool, italic: bool) -> Option<Rc<Face>> {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(f) = self.faces.borrow().get(&key) {
            return f.clone();
        }
        let f = match self.matches(&self.resolve(family, bold, italic), bold, italic) {
            Some(i) => self.load(i, family, bold, italic),
            None => (!self.entries.is_empty()).then(|| self.load(0, family, bold, italic)).flatten(),
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
            .filter_map(|fam| Some((fam, self.matches(fam, bold, italic)?)))
            .filter_map(|(fam, i)| self.load(i, fam, bold, italic))
            .find(|f| f.has_glyph(c));
        self.fallbacks.borrow_mut().insert((c, bold, italic), f.clone());
        f
    }
}

/// A variable font's named instances as faces (FONT-17), from its `fvar` table, or none. Each is named as Windows'
/// GDI names it, the name Word writes in documents: the typographic family, then the names of the instance's axis
/// values from `STAT` in its axis order, without elidable ones and Regular, Bold, Italic and Oblique (Bold and Italic
/// are styles), cut at 31 characters ("Bahnschrift Light Condensed", "Segoe UI Variable Display Semib"; the Bold
/// instance is "Bahnschrift" bold). Without `STAT`, the subfamily's words stand for the axis values. The weight,
/// width and italic come from the instance's `wght`, `wdth`, `ital` and `slnt` values, else `base`'s. Each face's
/// names: its GDI name, the name before the cut, and the subfamily form.
fn named_instances(fvar: &[u8], stat: Option<&[u8]>, names: &HashMap<u16, Vec<String>>, base: &Entry) -> Vec<(Entry, Vec<String>)> {
    let fixed = |o: usize| u32_at(fvar, o) as i32 as f32 / 65536.0;
    let (axes, axis_count, axis_size) = (u16_at(fvar, 4) as usize, u16_at(fvar, 8) as usize, u16_at(fvar, 10) as usize);
    let (count, size) = (u16_at(fvar, 12) as usize, u16_at(fvar, 14) as usize);
    let tags: Vec<[u8; 4]> = (0..axis_count).filter_map(|a| fvar.get(axes + a * axis_size..axes + a * axis_size + 4)?.try_into().ok()).collect();
    let start = axes + axis_count * axis_size;
    if tags.len() < axis_count || size < 4 + 4 * axis_count || fvar.len() < start + count * size {
        return Vec::new();
    }
    let family = names.get(&16).or(names.get(&1)).and_then(|v| v.first()).unwrap_or(&base.family);
    let ribbi = |w: &str| ["regular", "bold", "italic", "oblique", "bold italic"].contains(&w.to_lowercase().as_str());
    let named = |words: Vec<String>| if words.is_empty() { family.clone() } else { format!("{family} {}", words.join(" ")) };
    (0..count)
        .filter_map(|i| {
            let r = start + i * size;
            let sub = names.get(&u16_at(fvar, r))?.first()?;
            let variations: Vec<([u8; 4], f32)> = tags.iter().enumerate().map(|(a, &t)| (t, fixed(r + 4 + 4 * a))).collect();
            let value = |tag: &[u8; 4]| variations.iter().find(|(t, _)| t == tag).map(|&(_, v)| v);
            let from_sub = named(sub.split_whitespace().filter(|w| !ribbi(w)).map(str::to_string).collect());
            let full =
                stat.and_then(|s| stat_names(s, names, &value)).map_or(from_sub.clone(), |w| named(w.into_iter().filter(|w| !ribbi(w)).collect()));
            let gdi = String::from_utf16_lossy(&full.encode_utf16().take(31).collect::<Vec<u16>>()).trim_end().to_string();
            let slanted = sub.split_whitespace().any(|w| w.eq_ignore_ascii_case("italic") || w.eq_ignore_ascii_case("oblique"));
            // wdth is a percentage of normal; OS/2 width classes 1-9 are 50, 62.5, 75, 87.5, 100, 112.5, 125, 150, 200.
            let classes = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];
            let width = value(b"wdth").map(|w| classes.iter().rposition(|&c| w >= c - 0.01).map_or(1, |c| c as u16 + 1));
            let entry = Entry {
                family: gdi.clone(),
                weight: value(b"wght").map_or(base.weight, |w| w.round().clamp(1.0, 1000.0) as u16),
                width: width.unwrap_or(base.width),
                italic: base.italic || slanted || value(b"ital").is_some_and(|v| v >= 0.5) || value(b"slnt").is_some_and(|v| v != 0.0),
                path: base.path.clone(),
                data: base.data.clone(),
                index: base.index,
                instance: i as u32 + 1,
                variations,
            };
            Some((entry, vec![gdi, full, from_sub]))
        })
        .collect()
}

/// The names of the `STAT` axis values an instance has (`value` is its coordinate on an axis), in the table's axis
/// order, without elidable ones. A multi-axis value (format 4) names every axis it matches; an axis this font doesn't
/// vary has one value, which applies.
fn stat_names(stat: &[u8], names: &HashMap<u16, Vec<String>>, value: &dyn Fn(&[u8; 4]) -> Option<f32>) -> Option<Vec<String>> {
    let fixed = |o: usize| u32_at(stat, o) as i32 as f32 / 65536.0;
    let (axis_size, axis_count, axes) = (u16_at(stat, 4) as usize, u16_at(stat, 6) as usize, u32_at(stat, 8) as usize);
    let (value_count, offsets) = (u16_at(stat, 12) as usize, u32_at(stat, 14) as usize);
    if axis_count == 0 || stat.len() < axes + axis_count * axis_size || stat.len() < offsets + 2 * value_count {
        return None;
    }
    let axis: Vec<([u8; 4], u16)> = (0..axis_count)
        .filter_map(|a| Some((stat.get(axes + a * axis_size..axes + a * axis_size + 4)?.try_into().ok()?, u16_at(stat, axes + a * axis_size + 6))))
        .collect();
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    let at = |a: usize| axis.get(a).and_then(|(t, _)| value(t));
    // (axis order, name id, elidable) per axis named.
    let mut named: Vec<Option<(u16, u16, bool)>> = vec![None; axis_count];
    let records: Vec<usize> = (0..value_count).map(|v| offsets + u16_at(stat, offsets + 2 * v) as usize).filter(|&o| o + 12 <= stat.len()).collect();
    for &o in &records {
        let (format, flags, name) = (u16_at(stat, o), u16_at(stat, o + 4), u16_at(stat, o + 6));
        if format != 4 {
            continue;
        }
        let pairs: Vec<(usize, f32)> =
            (0..u16_at(stat, o + 2) as usize).map(|k| (u16_at(stat, o + 8 + 6 * k) as usize, fixed(o + 10 + 6 * k))).collect();
        if !pairs.is_empty() && pairs.iter().all(|&(a, v)| a < axis_count && named[a].is_none() && at(a).is_some_and(|c| close(c, v))) {
            let order = pairs.iter().map(|&(a, _)| axis[a].1).min().unwrap_or(0);
            // The name once, on its first axis; its other axes are taken, with nothing to show.
            for (k, &(a, _)) in pairs.iter().enumerate() {
                named[a] = Some((order, if k == 0 { name } else { 0 }, flags & 2 != 0 || k > 0));
            }
        }
    }
    for &o in &records {
        let (format, a, flags, name) = (u16_at(stat, o), u16_at(stat, o + 2) as usize, u16_at(stat, o + 4), u16_at(stat, o + 6));
        if a >= axis_count || named[a].is_some() {
            continue;
        }
        let v = fixed(o + 8);
        let hit = match (format, at(a)) {
            (1 | 3, Some(c)) => close(c, v),
            (2, Some(c)) => fixed(o + 12) <= c && c <= fixed(o + 16),
            (1..=3, None) => true,
            _ => false,
        };
        if hit {
            named[a] = Some((axis[a].1, name, flags & 2 != 0));
        }
    }
    let mut found: Vec<(u16, String)> = named
        .into_iter()
        .flatten()
        .filter(|&(_, _, elidable)| !elidable)
        .filter_map(|(order, id, _)| Some((order, names.get(&id)?.first()?.clone())))
        .collect();
    found.sort_by_key(|f| f.0);
    Some(found.into_iter().map(|f| f.1).collect())
}

/// A font file's bytes with only the parts [`Fonts::scan`] reads filled in (headers, directories, name/OS/2/head/fvar/STAT).
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
            if matches!(out.get(r..r + 4), Some(b"name") | Some(b"OS/2") | Some(b"head") | Some(b"fvar") | Some(b"STAT")) {
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
        if !(matches!(id, 1 | 2 | 6 | 16 | 17 | 21) || id >= 256) || off + len > t.len() {
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::api::engine::{Engine, SessionOptions};
    use crate::api::error::ErrorCode;
    use crate::api::fonts::{FontLibrary, FontRequest};

    const SERIF: &[u8] = include_bytes!("../tests/fixtures/editor/fonts/stand-in-serif.ttf");
    const ZZYZX: &[u8] = include_bytes!("../tests/fixtures/editor/fonts/zzyzx-unknown.ttf");
    /// A weight axis from 100 to 900 and six named instances; the box glyph's advance is 300 at 100, 500 at 400 and
    /// 800 at 900 (make-fixtures.py).
    const VARIABLE: &[u8] = include_bytes!("../tests/fixtures/editor/fonts/stand-in-variable.ttf");

    /// The font's tables in tag order.
    fn sfnt_tables(d: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut t: Vec<_> = tables(d, 0).into_iter().map(|(tag, (o, n))| (tag, &d[o..o + n])).collect();
        t.sort_by_key(|x| x.0);
        t
    }

    fn sfnt_size(t: &[([u8; 4], &[u8])]) -> u32 {
        (12 + 16 * t.len() + t.iter().map(|x| x.1.len().next_multiple_of(4)).sum::<usize>()) as u32
    }

    /// A WOFF file holding `sfnt`'s tables uncompressed.
    fn woff1(sfnt: &[u8]) -> Vec<u8> {
        let t = sfnt_tables(sfnt);
        let mut out = b"wOFF".to_vec();
        out.extend_from_slice(&sfnt[..4]);
        out.extend_from_slice(&[0; 4]); // length, below
        out.extend_from_slice(&(t.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 2]);
        out.extend_from_slice(&sfnt_size(&t).to_be_bytes());
        out.extend_from_slice(&[0, 1, 0, 0]);
        out.extend_from_slice(&[0; 20]);
        let mut offset = 44 + 20 * t.len();
        for (tag, data) in &t {
            out.extend_from_slice(tag);
            for v in [offset, data.len(), data.len(), 0] {
                out.extend_from_slice(&(v as u32).to_be_bytes());
            }
            offset += data.len().next_multiple_of(4);
        }
        for (_, data) in &t {
            out.extend_from_slice(data);
            out.resize(out.len().next_multiple_of(4), 0);
        }
        let len = (out.len() as u32).to_be_bytes();
        out[8..12].copy_from_slice(&len);
        out
    }

    /// A WOFF2 file holding `sfnt`'s tables untransformed, in a Brotli stream of one uncompressed meta-block.
    fn woff2(sfnt: &[u8]) -> Vec<u8> {
        let t = sfnt_tables(sfnt);
        let raw: Vec<u8> = t.iter().flat_map(|x| x.1.iter().copied()).collect();
        assert!(!raw.is_empty() && raw.len() <= 1 << 16);
        // WBITS 16 (one 0 bit), then ISLAST 0, four nibbles of MLEN - 1, ISUNCOMPRESSED 1; the empty last meta-block.
        let head = ((raw.len() as u32 - 1) << 4) | 1 << 20;
        let mut brotli = head.to_le_bytes()[..3].to_vec();
        brotli.extend_from_slice(&raw);
        brotli.push(0b11);
        let mut out = b"wOF2".to_vec();
        out.extend_from_slice(&sfnt[..4]);
        out.extend_from_slice(&[0; 4]); // length, below
        out.extend_from_slice(&(t.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 2]);
        out.extend_from_slice(&sfnt_size(&t).to_be_bytes());
        out.extend_from_slice(&(brotli.len() as u32).to_be_bytes());
        out.extend_from_slice(&[0, 1, 0, 0]);
        out.extend_from_slice(&[0; 20]);
        for (tag, data) in &t {
            // Tag 63 (spelled out); glyf and loca untransformed is version 3, every other table's is version 0.
            out.push(if matches!(tag, b"glyf" | b"loca") { 0xFF } else { 0x3F });
            out.extend_from_slice(tag);
            let n = data.len() as u32;
            let mut digits: Vec<u8> = (0..5).rev().map(|i| (n >> (7 * i)) as u8 & 0x7F).skip_while(|&d| d == 0).collect();
            if digits.is_empty() {
                digits.push(0);
            }
            let last = digits.len() - 1;
            out.extend(digits.iter().enumerate().map(|(i, d)| if i < last { d | 0x80 } else { *d }));
        }
        out.extend_from_slice(&brotli);
        out.resize(out.len().next_multiple_of(4), 0);
        let len = (out.len() as u32).to_be_bytes();
        out[8..12].copy_from_slice(&len);
        out
    }

    #[test]
    fn a_session_sees_fonts_added_after_it_was_created() {
        let mut engine = Engine::new(FontLibrary::empty());
        let id = engine.create_session(SessionOptions::default());
        let before = engine.session(id).unwrap().fonts().generation();
        engine.fonts().add(SERIF).unwrap();
        let s = engine.session(id).unwrap();
        assert!(s.fonts().generation() > before);
        assert_eq!(s.fonts().families(), ["Stand-in Serif"]);
        assert_eq!(s.fonts().fonts().face("Stand-in Serif", false, false).map(|f| f.family.clone()).as_deref(), Some("Stand-in Serif"));
    }

    #[test]
    fn woff_and_woff2_decode_to_faces() {
        for file in [woff1(SERIF), woff2(SERIF)] {
            let lib = FontLibrary::empty();
            let faces = lib.add(&file).unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(&file[..4])));
            assert_eq!(faces.iter().map(|f| f.family.as_str()).collect::<Vec<_>>(), ["Stand-in Serif"]);
            assert!(lib.fonts().face("Stand-in Serif", false, false).is_some_and(|f| f.has_glyph('A')));
        }
    }

    #[test]
    fn invalid_bytes_add_no_face() {
        let lib = FontLibrary::empty();
        let mut broken = woff2(SERIF);
        broken.truncate(60);
        let no_tables = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        for bytes in [&b"not a font at all"[..], b"wOFF", &broken, &no_tables, &SERIF[..200]] {
            assert_eq!(lib.add(bytes).map_err(|e| e.code), Err(ErrorCode::InvalidArgument), "{bytes:?}");
        }
        assert_eq!((lib.families().len(), lib.generation()), (0, 0));
    }

    #[test]
    fn a_variable_fonts_named_instances_are_faces_named_as_windows_names_them() {
        let mut f = Fonts::default();
        let faces = f.add_bytes(VARIABLE.to_vec()).unwrap();
        let got: Vec<(&str, bool)> = faces.iter().map(|i| (i.family.as_str(), i.bold)).collect();
        let family = "Stand-in Variable";
        let names = [("Stand-in Variable Thin", false), ("Stand-in Variable Light", false), (family, false)];
        let bold = [("Stand-in Variable SemiBold", true), (family, true), ("Stand-in Variable Black", true)];
        assert_eq!(got, [names, bold].concat());
        assert_eq!(
            f.families(),
            ["Stand-in Variable", "Stand-in Variable Black", "Stand-in Variable Light", "Stand-in Variable SemiBold", "Stand-in Variable Thin"]
        );
    }

    #[test]
    fn a_variable_face_shapes_and_measures_at_its_instance() {
        let mut f = Fonts::default();
        f.add_bytes(VARIABLE.to_vec()).unwrap();
        let advance = |family: &str, bold: bool| {
            let face = f.face(family, bold, false).unwrap();
            (face.weight, face.sim_bold, face.shape("A", true, 0)[0].advance, face.advance(face.glyph('A').unwrap()))
        };
        assert_eq!(advance("Stand-in Variable", false), (400, false, 500.0, 500.0));
        // Bold is the Bold instance, not a synthesized one.
        assert_eq!(advance("Stand-in Variable", true), (700, false, 680.0, 680.0));
        assert_eq!(advance("Stand-in Variable Thin", false), (100, false, 300.0, 300.0));
        assert_eq!(advance("stand-in variable black", false), (900, false, 800.0, 800.0));
    }

    #[test]
    fn a_bold_rule_applies_only_to_bold() {
        let mut f = Fonts::default();
        f.add_bytes(SERIF.to_vec()).unwrap();
        f.add_bytes(ZZYZX.to_vec()).unwrap();
        f.set_rules(vec![SubstitutionRule { from: "Doc Family".into(), bold: Some(true), italic: None, to: "Zzyzx Unknown".into() }]);
        let bold = f.face("doc family", true, false).unwrap();
        assert_eq!((bold.requested.as_str(), bold.family.as_str()), ("doc family", "Zzyzx Unknown"));
        assert!(f.resolves("DOC FAMILY", true, true));
        assert!(!f.resolves("Doc Family", false, false));
        // Unresolved, it falls back to the first font, until a default is set.
        assert_eq!(f.face("Doc Family", false, false).unwrap().family, "Stand-in Serif");
        f.set_default("Zzyzx Unknown");
        assert_eq!(f.face("Doc Family", false, false).unwrap().family, "Zzyzx Unknown");
    }

    #[test]
    fn each_face_is_requested_once() {
        let lib = FontLibrary::empty();
        lib.add(SERIF).unwrap();
        let asked = Rc::new(RefCell::new(Vec::new()));
        let sink = asked.clone();
        lib.on_request(move |r: &FontRequest| sink.borrow_mut().push((r.family.clone(), r.bold, r.italic)));
        let faces =
            [("Zzyzx Unknown", false, false), ("zzyzx unknown", false, false), ("Zzyzx Unknown", true, false), ("Stand-in Serif", false, false)];
        for _ in 0..2 {
            let state = lib.check(faces);
            assert_eq!((state.pending, state.unresolved), (vec!["Zzyzx Unknown".to_string()], vec![]));
        }
        assert_eq!(asked.borrow().len(), 2, "{:?}", asked.borrow());
        // Answered with nothing: unresolved, and not asked again. A face added later resolves it.
        for bold in [false, true] {
            lib.mark_unresolved(&FontRequest { family: "Zzyzx Unknown".into(), bold, italic: false });
        }
        let state = lib.check(faces);
        assert_eq!((state.pending, state.unresolved), (vec![], vec!["Zzyzx Unknown".to_string()]));
        lib.add(ZZYZX).unwrap();
        assert_eq!(lib.check(faces), Default::default());
        assert_eq!(asked.borrow().len(), 2);
    }
}
