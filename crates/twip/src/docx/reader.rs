//! DOCX package → [`Document`]: resolves styles (STYLE), numbering (LIST) and fields (FIELD) while reading.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::rc::Rc;

use roxmltree::Node;

use super::model::*;
use super::numbering::{Counters, Numbering};
use super::props::*;
use super::resolve::{ParaResolved, Resolver};
use super::styles::{Region, Styles, ThemeFonts};
use super::xml::*;

struct Package {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
}

impl Package {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let mut f = self.zip.by_name(path.trim_start_matches('/')).ok()?;
        let mut out = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut out).ok()?;
        Some(out)
    }

    fn read_str(&mut self, path: &str) -> Option<String> {
        let b = self.read(path)?;
        let s = String::from_utf8(b).ok()?;
        Some(s.trim_start_matches('\u{feff}').to_string())
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
    read_with(bytes, false)
}

/// As [`read`], keeping what saving needs: the package, the main part's XML, where each body paragraph and table came
/// from ([`Paragraph::src`], [`Table::src`]) and each run's direct formatting ([`RunStyle::direct`]).
pub fn read_editable(bytes: &[u8]) -> Result<Document, String> {
    read_with(bytes, true)
}

/// The XML text of an element as written.
fn raw(n: Node) -> Rc<str> {
    n.document().input_text()[n.range()].into()
}

fn read_with(bytes: &[u8], editable: bool) -> Result<Document, String> {
    let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|e| e.to_string())?;
    let mut pkg = Package { zip };
    let root_rels = pkg.rels("");
    let main = root_rels.values().find(|(k, _)| k == "officeDocument").map(|(_, t)| t.clone()).unwrap_or_else(|| "word/document.xml".into());
    let rels = pkg.rels(&main);
    let part_of = |kind: &str| rels.values().find(|(k, _)| k == kind).map(|(_, t)| t.clone());

    let styles = Styles::parse(part_of("styles").and_then(|p| pkg.read_str(&p)).as_deref());
    let numbering = Numbering::parse(part_of("numbering").and_then(|p| pkg.read_str(&p)).as_deref());
    let theme = ThemeFonts::parse(part_of("theme").and_then(|p| pkg.read_str(&p)).as_deref());
    let settings = read_settings(part_of("settings").and_then(|p| pkg.read_str(&p)).as_deref());

    let mut r = Reader {
        res: Resolver::new(styles, numbering, theme),
        settings,
        counters: Counters::default(),
        images: Vec::new(),
        image_index: HashMap::new(),
        fields: Vec::new(),
        para_no: 0,
        para_count: 0,
        float_order: 0,
        pending_hidden: None,
        part: main.clone(),
        rels: rels.clone(),
        note_id: 0,
        record: false,
        segs: Vec::new(),
        lossy: false,
        controls: HashMap::new(),
    };

    let mut doc = Document::default();
    let xml = pkg.read_str(&main).ok_or("no main document part")?;
    {
        let dom = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
        let body = child(dom.root_element(), "body").ok_or("no body")?;
        r.record = editable;
        doc.sections = r.read_body(body, &mut pkg);
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
        if let Some(blocks) = r.read_story(&path, &mut pkg, |root| Some(root)) {
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
        r.enter_part(&path, &mut pkg);
        // Editable reads keep where each note's paragraphs came from, as the body's.
        r.record = editable;
        if editable {
            note_parts.push((path.clone(), xml.clone(), tag == "footnote"));
        }
        for n in children(dom.root_element(), if tag == "footnote" { "footnote" } else { "endnote" }) {
            let id = int(attr(n, "id")).unwrap_or(0);
            r.note_id = id;
            let blocks = r.read_blocks(n, &mut pkg, None);
            match attr(n, "type") {
                Some(t @ ("separator" | "continuationSeparator" | "continuationNotice")) => {
                    doc.separators.insert(format!("{tag}:{t}"), blocks);
                }
                _ => {
                    map.insert(id, blocks);
                }
            }
        }
        r.record = false;
    }
    // LNUM-1: the LineNumber character style if present, else the default paragraph font.
    doc.line_number_run = Some({
        let mut rp = r.res.styles.doc_rpr.clone();
        if r.res.styles.get("LineNumber").is_some() {
            rp.merge_level(&r.res.styles.chain_rpr(Some("LineNumber")));
        }
        r.res.run_style(&rp)
    });
    let mut image_parts = vec![String::new(); r.images.len()];
    for (part, &i) in &r.image_index {
        image_parts[i] = part.clone();
    }
    doc.images = r.images;
    doc.controls = std::mem::take(&mut r.controls);
    doc.data_parts = data_parts(&mut pkg, &rels);
    doc.settings = r.settings;
    doc.resolver = Some(Rc::new(r.res));
    super::fields::evaluate(&mut doc);
    super::frames::convert(&mut doc);
    doc.derive_image_sets();
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
        }));
    }
    Ok(doc)
}

fn read_settings(xml: Option<&str>) -> Settings {
    let mut s = Settings { compat_mode: 12, default_tab: 36.0, hyphenation_zone: 18.0, ..Default::default() };
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
    s.auto_hyphenation = flag(root, "autoHyphenation").unwrap_or(false);
    s.do_not_hyphenate_caps = flag(root, "doNotHyphenateCaps").unwrap_or(false);
    s.borders_do_not_surround_header = flag(root, "bordersDoNotSurroundHeader").unwrap_or(false);
    s.borders_do_not_surround_footer = flag(root, "bordersDoNotSurroundFooter").unwrap_or(false);
    if let Some(z) = child(root, "hyphenationZone").and_then(|n| twips(val(n))) {
        s.hyphenation_zone = z;
    }
    s.consecutive_hyphen_limit = child(root, "consecutiveHyphenLimit").and_then(|n| int(val(n))).unwrap_or(0);
    if let Some(c) = child(root, "compat") {
        for e in c.children().filter(|e| e.is_element()) {
            if is_w(e, "compatSetting") {
                if attr(e, "name") == Some("compatibilityMode")
                    && let Some(m) = int(attr(e, "val"))
                {
                    s.compat_mode = m;
                }
            } else if on_off(e) {
                s.compat.insert(e.tag_name().name().to_string());
            }
        }
    }
    s
}

/// The table context a paragraph is read in: the cell's table-style region (STYLE-4).
#[derive(Clone)]
struct TableCtx {
    region: Rc<Region>,
}

/// An open complex field.
struct OpenField {
    id: FieldId,
    in_result: bool,
    first_code_run: Option<Run>,
    first_result_run: Option<Run>,
    /// The paragraph it began in ([`Reader::para_no`]), and its begin marker's index in that paragraph's inlines.
    para: u64,
    begin: usize,
    /// It began in an earlier paragraph than the one being read: a field across paragraphs (a table of contents).
    /// Its markers and code were taken out at that paragraph's end; its result reads as plain text.
    spanning: bool,
    /// A TOC field (known at its separator): its result doesn't show the Hyperlink character style.
    toc: bool,
}

struct Reader {
    res: Resolver,
    settings: Settings,
    counters: Counters,
    images: Vec<Vec<u8>>,
    image_index: HashMap<String, usize>,
    fields: Vec<OpenField>,
    /// The paragraph being read (a number unique in the read), and the last number given.
    para_no: u64,
    para_count: u64,
    float_order: usize,
    /// A paragraph whose mark is hidden, waiting to join the next one (STYLE-7).
    pending_hidden: Option<Paragraph>,
    part: String,
    rels: HashMap<String, (String, String)>,
    /// The note being read (for its `w:footnoteRef` mark).
    note_id: i64,
    /// Reading the main body for editing: paragraphs and tables get their sources, runs their direct formatting.
    record: bool,
    /// The paragraph being read: elements that produced units ([`ParaSrc::segs`]), and whether any can't be mapped.
    segs: Vec<(std::ops::Range<usize>, usize)>,
    lossy: bool,
    /// [`Document::controls`].
    controls: HashMap<ControlId, Control>,
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
        let blocks = self.read_blocks(root, pkg, None);
        (self.part, self.rels) = saved;
        Some(blocks)
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
        sec.blocks = blocks;
        sections.push(sec);
        sections
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
        for n in container.children().filter(|c| c.is_element()) {
            if is_w(n, "p") {
                let sect = child(n, "pPr").and_then(|p| child(p, "sectPr"));
                let para = self.read_paragraph(n, pkg, table);
                out.push((para.map(Block::Para), sect));
            } else if is_w(n, "tbl") {
                if let Some(p) = self.pending_hidden.take() {
                    out.push((Some(Block::Para(p)), None));
                }
                out.push((Some(Block::Table(self.read_table(n, pkg))), None));
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
                && let Some(c) = n.children().find(|c| is(*c, MC, "Choice")).or_else(|| n.children().find(|c| is(*c, MC, "Fallback")))
            {
                out.extend(self.walk_blocks(c, pkg, table));
            }
        }
        out
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
            line_numbers: None,
            v_align: "top".into(),
            page_borders: None,
            blocks: Vec::new(),
        }
    }

    fn read_sect(&self, s: Node) -> Section {
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
            let num = int(attr(c, "num")).unwrap_or(1).max(1) as usize;
            let space = attr_twips(c, "space").unwrap_or(36.0);
            let equal = attr(c, "equalWidth").map(|v| on_off_str(Some(v))).unwrap_or(true);
            let explicit: Vec<(f64, f64)> =
                children(c, "col").map(|col| (attr_twips(col, "w").unwrap_or(0.0), attr_twips(col, "space").unwrap_or(0.0))).collect();
            sec.columns = Columns {
                num: if !equal && !explicit.is_empty() { explicit.len() } else { num },
                cols: if equal { Vec::new() } else { explicit },
                equal: equal || children(c, "col").next().is_none(),
                space,
                separator: attr(c, "sep").map(|v| on_off_str(Some(v))).unwrap_or(false),
            };
        }
        sec.title_pg = flag(s, "titlePg").unwrap_or(false);
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
        }
        if let Some(l) = child(s, "lnNumType") {
            sec.line_numbers = Some(LineNumbering {
                count_by: int(attr(l, "countBy")).unwrap_or(1).max(1),
                start: int(attr(l, "start")).unwrap_or(0),
                distance: attr_twips(l, "distance").unwrap_or(0.0),
                restart: attr(l, "restart").unwrap_or("newPage").to_string(),
            });
        }
        if let Some(v) = child(s, "vAlign").and_then(val) {
            sec.v_align = v.to_string();
        }
        if let Some(b) = child(s, "pgBorders") {
            let side = |n| child(b, n).map(Border::parse);
            sec.page_borders = Some(PageBorders {
                top: side("top"),
                left: side("left"),
                bottom: side("bottom"),
                right: side("right"),
                offset_from_page: attr(b, "offsetFrom") == Some("page"),
                display: attr(b, "display").unwrap_or("allPages").to_string(),
                surround_header: !self.settings.borders_do_not_surround_header,
                surround_footer: !self.settings.borders_do_not_surround_footer,
            });
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
        let direct = ParaProps::parse(ppr_node);
        let ParaResolved { style: mut para_style, base, level, num_id, ilvl, from_style, style_id } =
            self.res.paragraph(&direct, table.map(|t| &*t.region));
        if self.record {
            para_style.direct = Raw(ppr_node.map(raw));
        }
        let para_style = Rc::new(para_style);

        let mark_rpr_node = ppr_node.and_then(|p| child(p, "rPr"));
        let mark_props = self.run_props(&base, mark_rpr_node);
        let mark = self.with_direct(self.run_style(&mark_props), mark_rpr_node);

        // List label (LIST-6): mark formatting, then the level's rPr.
        let list = match (&level, num_id) {
            (Some(l), Some(id)) => {
                if self.res.label_shows(id, ilvl, from_style, style_id.as_deref()) {
                    self.counters.next(&self.res.numbering, &self.res.styles, id, ilvl).map(|text| {
                        let mut lp = mark_props.clone();
                        lp.merge(&l.rpr);
                        ListLabel { text, run: self.res.run_style(&lp), jc: l.jc.clone(), suff: l.suff.clone() }
                    })
                } else {
                    None
                }
            }
            _ => None,
        };

        let mut inlines = Vec::new();
        let outer = (std::mem::take(&mut self.segs), std::mem::replace(&mut self.lossy, false));
        self.para_count += 1;
        let outer_para = std::mem::replace(&mut self.para_no, self.para_count);
        self.read_inlines(n, &base, pkg, &mut inlines);
        self.cut_spanning_fields(&mut inlines);
        self.para_no = outer_para;
        let (segs, lossy) = (std::mem::replace(&mut self.segs, outer.0), std::mem::replace(&mut self.lossy, outer.1));
        let src = self.record.then(|| Rc::new(ParaSrc { ranges: vec![n.range()], segs, lossy }));
        let para = Paragraph { style: para_style, mark: mark.clone(), list, inlines, src, id: next_para_id(), controls: Vec::new() };

        // STYLE-7: a hidden paragraph mark joins this paragraph to the next visible one.
        let mut para = match self.pending_hidden.take() {
            Some(mut prev) => {
                prev.inlines.extend(para.inlines);
                let src = match (prev.src, para.src) {
                    (Some(a), Some(b)) => Some(Rc::new(ParaSrc {
                        ranges: [a.ranges.clone(), b.ranges.clone()].concat(),
                        segs: [a.segs.clone(), b.segs.clone()].concat(),
                        lossy: a.lossy || b.lossy,
                    })),
                    (a, b) => a.or(b),
                };
                // shortcut: the joined paragraph keeps the second (visible) mark's paragraph properties and the first's
                // list label; STYLE-7 doesn't say which paragraph's properties win.
                Paragraph {
                    style: para.style,
                    mark: para.mark,
                    list: prev.list.or(para.list),
                    inlines: prev.inlines,
                    src,
                    id: prev.id,
                    controls: para.controls,
                }
            }
            None => para,
        };
        if mark.hidden {
            para.mark = mark;
            self.pending_hidden = Some(para);
            return None;
        }
        Some(para)
    }

    /// A run's properties over a base: its character style chain (a toggle level), then direct formatting.
    fn run_props(&self, base: &RunProps, rpr: Option<Node>) -> RunProps {
        self.res.run_props(base, &RunProps::parse(rpr))
    }

    fn run_style(&self, p: &RunProps) -> Run {
        self.res.run_style(p)
    }

    fn read_inlines(&mut self, container: Node, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        for e in container.children().filter(|c| c.is_element()) {
            if e.tag_name().namespace() == Some(W) {
                match e.tag_name().name() {
                    "r" => self.read_run(e, base, pkg, out),
                    "fldSimple" if !self.in_code() => self.read_simple_field(e, base, pkg, out),
                    // An inline custom XML wrapper: a control of kind Other around its content.
                    "customXml" if !self.in_code() => {
                        let id = self.control(e);
                        out.push(Inline::ControlStart(id));
                        self.read_inlines(e, base, pkg, out);
                        out.push(Inline::ControlEnd(id));
                    }
                    "hyperlink" | "ins" | "moveTo" | "smartTag" | "customXml" | "bdo" | "dir" | "fldSimple" => {
                        self.read_inlines(e, base, pkg, out);
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
                    "bookmarkStart" => {
                        if let Some(name) = attr(e, "name") {
                            out.push(Inline::Bookmark(name.to_string()));
                        }
                    }
                    // w:del, w:moveFrom, comments, proofing marks: not shown.
                    _ => {}
                }
            } else if is(e, MC, "AlternateContent")
                && let Some(c) = e.children().find(|c| is(*c, MC, "Choice"))
            {
                self.read_inlines(c, base, pkg, out);
            }
        }
    }

    fn read_run(&mut self, r: Node, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        let mut direct = RunProps::parse(child(r, "rPr"));
        // FIELD-5: a table of contents' result shows its entries without the Hyperlink character style;
        // other character styles and direct formatting show. The source keeps it.
        let toc = self.fields.iter().any(|f| f.toc && f.in_result);
        if toc && direct.style.as_deref() == Some("Hyperlink") {
            direct.style = None;
        }
        let props = self.res.run_props(base, &direct);
        let mut run = self.with_direct(self.run_style(&props), child(r, "rPr"));
        if toc {
            Rc::make_mut(&mut run).toc = true;
        }
        // The child being read and the units before it: what it added becomes a seg when the next one starts.
        let mut prev: Option<(std::ops::Range<usize>, (usize, usize))> = None;
        for c in r.children().filter(|c| c.is_element()) {
            if self.record {
                self.note_seg(out, prev.take());
                prev = Some((c.range(), units_mark(out)));
            }
            if is(c, MC, "AlternateContent") {
                if let Some(ch) = c.children().find(|x| is(*x, MC, "Choice")) {
                    for d in ch.children().filter(|d| is_w(*d, "drawing")) {
                        self.read_drawing(d, &run, pkg, out);
                    }
                }
                continue;
            }
            if c.tag_name().namespace() != Some(W) {
                continue;
            }
            let name = c.tag_name().name();
            // Complex fields (FIELD-1): each `fldChar` is a marker, the code's `instrText` its text.
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
                            first_result_run: None,
                            para: self.para_no,
                            begin: out.len(),
                            spanning: false,
                            toc: false,
                        });
                        let (lock, dirty) = (flag("fldLock"), flag("dirty"));
                        out.push(Inline::FieldBegin {
                            id,
                            kind: None,
                            lock,
                            dirty,
                            view: FieldView::Result,
                            label: None,
                            value: None,
                            run: run.clone(),
                        });
                    }
                    Some("separate") => {
                        if let (Some(id), Some(f)) = (id, self.fields.last_mut())
                            && !f.in_result
                        {
                            f.in_result = true;
                            if !f.spanning {
                                let code: String =
                                    out[f.begin..].iter().filter_map(|i| if let Inline::Text(t, _) = i { Some(t.as_str()) } else { None }).collect();
                                f.toc = code.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("TOC"));
                                out.push(Inline::FieldSep { id, run: run.clone() });
                            }
                        }
                    }
                    Some("end") => {
                        if let (Some(id), Some(f)) = (id, self.fields.pop())
                            && !f.spanning
                        {
                            out.push(Inline::FieldEnd { id, run: run.clone() });
                            self.close_field(out, f, &run);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(f) = self.fields.last_mut() {
                if !f.in_result {
                    if name == "instrText" && !f.spanning {
                        f.first_code_run.get_or_insert(run.clone());
                        if let Some(t) = c.text().filter(|t| !t.is_empty()) {
                            push_text(out, t, &run);
                        }
                    }
                    continue;
                }
                if name == "t" {
                    f.first_result_run.get_or_insert(run.clone());
                }
            }
            if run.hidden {
                // STYLE-7: hidden runs take no space.
                continue;
            }
            match name {
                "t" => {
                    if let Some(t) = c.text().filter(|t| !t.is_empty()) {
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
                    let kind = match attr(c, "type") {
                        Some("page") => BreakKind::Page,
                        Some("column") => BreakKind::Column,
                        _ => BreakKind::Line,
                    };
                    out.push(Inline::Break(kind, run.clone()));
                }
                "cr" => out.push(Inline::Break(BreakKind::Line, run.clone())),
                "noBreakHyphen" => push_text(out, "\u{2011}", &run),
                "softHyphen" => push_text(out, "\u{AD}", &run),
                "sym" => {
                    let font = attr(c, "font").unwrap_or("Symbol").to_string();
                    let code = attr(c, "char").and_then(|h| u32::from_str_radix(h, 16).ok()).unwrap_or(0x3F);
                    // Symbol fonts map their characters to the private-use F000 page.
                    let code = if code < 0x100 && is_symbol_font(&font) { code + 0xF000 } else { code };
                    out.push(Inline::Sym { ch: char::from_u32(code).unwrap_or('?'), font, run: run.clone(), control: None });
                }
                "drawing" => self.read_drawing(c, &run, pkg, out),
                "footnoteReference" | "endnoteReference" => out.push(Inline::NoteRef {
                    kind: if name == "footnoteReference" { NoteKind::Foot } else { NoteKind::End },
                    id: int(attr(c, "id")).unwrap_or(0),
                    custom: attr(c, "customMarkFollows").map(|v| on_off_str(Some(v))).unwrap_or(false),
                    run: run.clone(),
                }),
                "footnoteRef" => out.push(Inline::NoteMark(NoteKind::Foot, self.note_id, run.clone())),
                "endnoteRef" => out.push(Inline::NoteMark(NoteKind::End, self.note_id, run.clone())),
                "separator" => out.push(Inline::Separator { continuation: false, run: run.clone() }),
                "continuationSeparator" => out.push(Inline::Separator { continuation: true, run: run.clone() }),
                _ => {}
            }
        }
        if self.record {
            self.note_seg(out, prev);
        }
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
        // Whether its face goes through the East Asian slot (FONT-12): the hint is all of its fonts the symbol's run keeps.
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
        out.push(Inline::Sym { ch, font, run: Rc::new(run), control: Some(id) });
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

    /// A field's markers are all in: its kind from its code, and (an evaluated kind) the run its value is drawn in
    /// (FIELD-4: the result's first run with `\* MERGEFORMAT`, else the code's).
    fn close_field(&mut self, out: &mut [Inline], f: OpenField, end_run: &Run) {
        let Some(span) = super::fields::span_at(out, f.begin) else { return };
        let code = super::fields::code_text(out, span);
        let parsed = super::fields::parse(&code);
        let kind = parsed.kind();
        let mergeformat = parsed.switches.iter().any(|s| s.flag == '*' && s.arg.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("MERGEFORMAT")));
        let fmt = if mergeformat { f.first_result_run.or(f.first_code_run.clone()) } else { f.first_code_run.clone() };
        if let Inline::FieldBegin { kind: k, run, .. } = &mut out[f.begin] {
            if kind.is_some() {
                *run = fmt.unwrap_or_else(|| end_run.clone());
            }
            *k = kind;
        }
    }

    /// `w:fldSimple`: the same markers as a complex field, its `w:instr` the code. Editable reads map the element as
    /// one seg (its units all from it), so an unchanged field is copied as it was.
    fn read_simple_field(&mut self, e: Node, base: &RunProps, pkg: &mut Package, out: &mut Vec<Inline>) {
        let code = attr(e, "instr").unwrap_or("").to_string();
        let first = e.children().find(|c| is_w(*c, "r"));
        let rpr = first.and_then(|r| child(r, "rPr"));
        let run = self.with_direct(self.run_style(&self.run_props(base, rpr)), rpr);
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
            run: run.clone(),
        });
        if !code.is_empty() {
            out.push(Inline::Text(code, run.clone()));
        }
        out.push(Inline::FieldSep { id, run: run.clone() });
        self.fields.push(OpenField {
            id,
            in_result: true,
            first_code_run: Some(run.clone()),
            first_result_run: None,
            para: self.para_no,
            begin,
            spanning: false,
            toc: false,
        });
        self.read_inlines(e, base, pkg, out);
        let f = self.fields.pop().expect("the simple field pushed above");
        out.push(Inline::FieldEnd { id, run: run.clone() });
        let fmt = f.first_result_run.clone();
        self.close_field(out, OpenField { first_result_run: fmt, ..f }, &run);
        if self.record {
            self.segs.truncate(segs);
            self.segs.push((e.range(), added_units(out, mark)));
        }
    }

    /// At a paragraph's end: a field begun in it and still open spans paragraphs. Its markers and code (with the
    /// fields in it) come out of the paragraph, and its result reads as plain text from here on.
    fn cut_spanning_fields(&mut self, out: &mut Vec<Inline>) {
        let no = self.para_no;
        let Some(first) = self.fields.iter().position(|f| f.para == no && !f.spanning) else { return };
        let begin = self.fields[first].begin;
        let id = self.fields[first].id;
        // Its code ends at its separator if it is in this paragraph, else at the paragraph's end.
        let stop = out[begin..].iter().position(|i| matches!(i, Inline::FieldSep { id: s, .. } if *s == id)).map_or(out.len(), |k| begin + k + 1);
        let units_before: usize = out[..begin].iter().map(Inline::units).sum();
        let cut: usize = out[begin..stop].iter().map(Inline::units).sum();
        out.drain(begin..stop);
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

    fn read_drawing(&mut self, d: Node, run: &Run, pkg: &mut Package, out: &mut Vec<Inline>) {
        let Some(item) = d.children().find(|c| is(*c, WP, "inline") || is(*c, WP, "anchor")) else { return };
        let extent = item.children().find(|c| is(*c, WP, "extent"));
        let w = extent.and_then(|e| emu(e.attribute("cx"))).unwrap_or(0.0);
        let h = extent.and_then(|e| emu(e.attribute("cy"))).unwrap_or(0.0);
        let blip = item.descendants().find(|n| is(*n, A, "blip"));
        let image = blip.and_then(|b| b.attribute((R, "embed"))).map(str::to_string).and_then(|rid| self.image(&rid, pkg));
        let txbx = item.descendants().find(|n| is_w(*n, "txbxContent"));
        let is_pic = item.descendants().any(|n| n.tag_name().name() == "pic" && n.tag_name().namespace().is_some_and(|ns| ns.ends_with("/picture")));
        if !is_pic && txbx.is_none() {
            return; // other shapes are out of scope
        }
        let alt: Rc<str> = item
            .children()
            .find(|c| is(*c, WP, "docPr"))
            .and_then(|p| p.attribute("descr").filter(|s| !s.is_empty()).or(p.attribute("title")))
            .unwrap_or("")
            .into();
        let lock_aspect = item
            .descendants()
            .any(|n| matches!(n.tag_name().name(), "graphicFrameLocks" | "picLocks") && matches!(n.attribute("noChangeAspect"), Some("1" | "true")));
        if is(item, WP, "inline") {
            out.push(Inline::Picture(Picture { w, h, image, alt, lock_aspect }, run.clone()));
            return;
        }
        let pos = |name: &str| item.children().find(|c| is(*c, WP, name));
        let (h_rel, h_align, h_off) = position(pos("positionH"));
        let (v_rel, v_align, v_off) = position(pos("positionV"));
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
        let wrap_text = wrap_el.and_then(|w| w.attribute("wrapText")).unwrap_or("bothSides").to_string();
        let dist = |n: &str| emu(item.attribute(n)).unwrap_or(0.0);
        let text_box = txbx.map(|t| {
            let body = item.descendants().find(|n| n.tag_name().name() == "bodyPr");
            let ins = |n: &str, dflt: f64| body.and_then(|b| emu(b.attribute(n))).unwrap_or(dflt);
            let ln = item.descendants().find(|n| is(*n, A, "ln"));
            let has_line = ln.is_some_and(|l| !l.children().any(|c| is(c, A, "noFill")));
            let outline = if has_line { ln.and_then(|l| emu(l.attribute("w"))).unwrap_or(0.75) } else { 0.0 };
            let auto_fit = body.is_some_and(|b| b.children().any(|c| is(c, A, "spAutoFit")));
            let saved = std::mem::take(&mut self.fields);
            let blocks = self.read_blocks(t, pkg, None);
            self.fields = saved;
            TextBox { blocks, insets: [ins("lIns", 7.2), ins("tIns", 3.6), ins("rIns", 7.2), ins("bIns", 3.6)], outline, auto_fit }
        });
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
                behind: item.attribute("behindDoc").is_some_and(|v| on_off_str(Some(v))),
                allow_overlap: item.attribute("allowOverlap").map(|v| on_off_str(Some(v))).unwrap_or(true),
                layout_in_cell: item.attribute("layoutInCell").map(|v| on_off_str(Some(v))).unwrap_or(true),
                order: self.float_order,
                src: Some(d.range()),
                locked: item.attribute("locked").is_some_and(|v| on_off_str(Some(v))),
                frame_src: None,
            }),
            run.clone(),
        ));
    }

    // ---------------- tables ----------------

    fn read_table(&mut self, t: Node, pkg: &mut Package) -> Table {
        let direct = TblProps::parse(child(t, "tblPr"));
        let style_id = self.res.styles.table_style(direct.style.as_deref()).map(str::to_string);
        let style_region = self.res.styles.table_region(style_id.as_deref(), &[]);
        let mut tbl = style_region.tbl.clone();
        tbl.merge(&direct);
        let grid: Vec<f64> =
            child(t, "tblGrid").map(|g| children(g, "gridCol").map(|c| attr_twips(c, "w").unwrap_or(0.0)).collect()).unwrap_or_default();
        let look = tbl.look.unwrap_or(Look { first_row: true, last_row: false, first_col: true, last_col: false, no_h_band: false, no_v_band: true });
        let row_nodes: Vec<(Node, Vec<ControlId>)> = self.rows_of(t, &[]);
        let nrows = row_nodes.len();
        let row_band = tbl.row_band.unwrap_or(0);
        let col_band = tbl.col_band.unwrap_or(0);
        let mut rows = Vec::new();
        for (ri, (rn, row_controls)) in row_nodes.iter().enumerate() {
            let mut trp = style_region.tr.clone();
            trp.merge(&TrProps::parse(child(*rn, "trPr")));
            let cell_nodes: Vec<(Node, Vec<ControlId>)> = self.cells_of(*rn, &[]);
            let ncols = cell_nodes.len();
            let mut cells = Vec::new();
            for (ci, (cn, cell_controls)) in cell_nodes.iter().enumerate() {
                let region = Rc::new(self.res.cell_region(style_id.as_deref(), look, (row_band, col_band), (ri, ci), (nrows, ncols)));
                let mut tcp = region.tc.clone();
                tcp.merge(&TcProps::parse(child(*cn, "tcPr")));
                let ctx = TableCtx { region: region.clone() };
                let blocks = self.read_blocks(*cn, pkg, Some(&ctx));
                let m = |v: Option<f64>, d: Option<f64>| v.or(d).unwrap_or(0.0);
                // TABLE-2: left and right margins of a table with no table style at all (no tblStyle and no default
                // "Normal Table") and no tcMar or tblCellMar are 10 twips, not 0; with a table style, even one that sets no
                // margins, they stay 0.
                let unset = if style_id.is_none() { 0.5 } else { 0.0 };
                let side = |v: Option<f64>, d: Option<f64>| v.or(d).unwrap_or(unset);
                let tm = &tbl.margins;
                let bv = |b: &Option<Border>| b.clone();
                cells.push(Cell {
                    width: tcp.width,
                    span: tcp.grid_span.unwrap_or(1).max(1) as usize,
                    v_merge: match tcp.v_merge {
                        Some(true) => VMerge::Restart,
                        Some(false) => VMerge::Continue,
                        None => VMerge::None,
                    },
                    top: bv(&tcp.borders.top),
                    left: bv(&tcp.borders.left),
                    bottom: bv(&tcp.borders.bottom),
                    right: bv(&tcp.borders.right),
                    shading: tcp.shading.flatten(),
                    margins: [
                        m(tcp.margins.top, tm.top),
                        side(tcp.margins.left, tm.left),
                        m(tcp.margins.bottom, tm.bottom),
                        side(tcp.margins.right, tm.right),
                    ],
                    v_align: tcp.v_align.clone().unwrap_or_else(|| "top".into()),
                    text_direction: tcp.text_direction.clone().unwrap_or_else(|| "lrTb".into()),
                    no_wrap: tcp.no_wrap.unwrap_or(false),
                    direct: Raw(child(*cn, "tcPr").map(|n| Rc::from(&n.document().input_text()[n.range()]))),
                    blocks,
                    controls: cell_controls.clone(),
                });
            }
            rows.push(Row {
                height: trp.height.clone(),
                cant_split: trp.cant_split.unwrap_or(false),
                header: trp.header.unwrap_or(false),
                grid_before: trp.grid_before.unwrap_or(0).max(0) as usize,
                grid_after: trp.grid_after.unwrap_or(0).max(0) as usize,
                w_before: trp.w_before.unwrap_or(0.0).max(0.0),
                w_after: trp.w_after.unwrap_or(0.0).max(0.0),
                cells,
                controls: row_controls.clone(),
                src: self.record.then(|| Rc::new(rn.range())),
            });
        }
        let b = &tbl.borders;
        Table {
            style_id,
            width: tbl.width.unwrap_or(Width::Auto),
            jc: tbl.jc.unwrap_or(Jc::Left),
            ind: tbl.ind.unwrap_or(0.0),
            borders: [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()],
            fixed: tbl.fixed.unwrap_or(false),
            cell_spacing: tbl.cell_spacing.unwrap_or(0.0),
            look: tbl.look,
            bands: (row_band, col_band),
            floating: tbl.floating,
            grid,
            rows,
            src: self.record.then(|| Rc::new(t.range())),
            id: next_para_id(),
            direct: Raw(child(t, "tblPr").map(|n| Rc::from(&n.document().input_text()[n.range()]))),
            controls: Vec::new(),
        }
    }
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

fn push_text(out: &mut Vec<Inline>, t: &str, run: &Run) {
    if let Some(Inline::Text(s, r)) = out.last_mut()
        && (Rc::ptr_eq(r, run) || **r == **run)
    {
        s.push_str(t);
        return;
    }
    out.push(Inline::Text(t.to_string(), run.clone()));
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
                kind = Some((ControlKind::Checkbox { checked: '\u{2612}', unchecked: '\u{2610}' }, raw()))
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
