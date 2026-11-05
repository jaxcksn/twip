//! The engine's benchmarks: `cargo bench -p twip [-- <filter>]`, e.g. `cargo bench -p twip -- editing::typing`.
//!
//! Each benchmark times one input, layout or frame on a committed fixture; setup between samples (placing the caret,
//! draining background layout) isn't timed. The allocation counts beside each time are Divan's `AllocProfiler`.

mod drags;
mod editing;
mod frames;
mod layout;
mod pdf;
mod review;

use std::fmt::Write as _;

use twip::internal::docx::Block;
use twip::internal::layout::ParaId;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

/// A fixture's bytes, by its path under `tests/fixtures`.
/// A Letter page of body text with 100 theme-styled shapes of mixed presets in a 10 x 10 grid over it, page
/// relative: one in five wraps square, one in ten has text, one in ten a shadow and a glow. The first is anchored at
/// the first paragraph's start (column and paragraph, offset 0), 90 x 60 pt, no wrapping: a drag's target.
fn shapes_doc() -> Vec<u8> {
    use std::io::Write;
    const PRESETS: [&str; 10] =
        ["rect", "roundRect", "ellipse", "star5", "wedgeRectCallout", "rightArrow", "hexagon", "cloud", "heart", "flowChartProcess"];
    let emu = |pt: f64| (pt * 12700.0).round() as i64;
    let style = r#"<wps:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="15000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style>"#;
    let mut runs = String::new();
    for k in 0..100usize {
        let (w, h) = if k == 0 { (90.0, 60.0) } else { (40.0, 30.0) };
        let (x, y) = (60.0 + (k % 10) as f64 * 50.0, 60.0 + (k / 10) as f64 * 66.0);
        let (h_pos, v_pos) = if k == 0 {
            (
                "<wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH>".to_string(),
                "<wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV>".to_string(),
            )
        } else {
            (
                format!("<wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>", emu(x)),
                format!("<wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV>", emu(y)),
            )
        };
        let wrap = if k % 5 == 4 { "<wp:wrapSquare wrapText=\"bothSides\"/>" } else { "<wp:wrapNone/>" };
        let effects = if k % 10 == 3 {
            r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw><a:glow rad="63500"><a:srgbClr val="FFC000"/></a:glow></a:effectLst>"#
        } else {
            ""
        };
        let text = if k % 10 == 7 { "<wps:txbx><w:txbxContent><w:p><w:r><w:t>Box</w:t></w:r></w:p></w:txbxContent></wps:txbx>" } else { "" };
        let _ = write!(
            runs,
            concat!(
                r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{z}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>{h_pos}{v_pos}"#,
                r#"<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}<wp:docPr id="{id}" name="Shape {id}"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">"#,
                r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom>{effects}</wps:spPr>{style}{text}<wps:bodyPr anchor="ctr"/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
            ),
            z = k + 1,
            h_pos = h_pos,
            v_pos = v_pos,
            cx = emu(w),
            cy = emu(h),
            wrap = wrap,
            id = k + 1,
            prst = PRESETS[k % 10],
            effects = effects,
            style = style,
            text = text,
        );
    }
    let words = "The committee reviewed the figures for the second quarter and agreed that they should be checked again. ";
    let para = |t: &str| format!("<w:p><w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r></w:p>");
    let mut body = format!("<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r>{runs}</w:p>", words.repeat(3));
    for _ in 0..12 {
        body.push_str(&para(&words.repeat(4)));
    }
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document {ns}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, xml) in [("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", document.as_str())] {
        z.start_file(name, zip::write::SimpleFileOptions::default()).expect("a part");
        z.write_all(xml.as_bytes()).expect("written");
    }
    z.finish().expect("a package").into_inner()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// With `--features prof`, a diagnostic run: the stage times and the reuse report of a benchmark's timed parts,
/// printed once after its samples. Untimed setup is discarded (`skip`); without the feature everything is empty and
/// nothing prints.
#[derive(Default)]
struct Prof {
    times: Vec<(&'static str, f64, u64)>,
    reuse: [[u64; 3]; 3],
    samples: usize,
}

impl Prof {
    /// Drops what untimed setup counted.
    fn skip() {
        twip::internal::prof::take();
        twip::internal::prof::take_reuse();
    }

    /// Adds what a timed sample counted.
    fn add(&mut self) {
        for (name, ms, calls) in twip::internal::prof::take() {
            match self.times.iter_mut().find(|t| t.0 == name) {
                Some(t) => (t.1, t.2) = (t.1 + ms, t.2 + calls),
                None => self.times.push((name, ms, calls)),
            }
        }
        let r = twip::internal::prof::take_reuse();
        for (row, add) in self.reuse.iter_mut().zip(r) {
            for (c, a) in row.iter_mut().zip(add) {
                *c += a;
            }
        }
        self.samples += 1;
    }

    fn print(&mut self, name: &str) {
        if self.times.is_empty() && self.reuse == [[0; 3]; 3] {
            return;
        }
        let n = self.samples.max(1) as f64;
        self.times.sort_by(|a, b| b.1.total_cmp(&a.1));
        let stages: Vec<String> = self.times.iter().map(|(s, ms, calls)| format!("{s} {:.3} ms x{:.1}", ms / n, *calls as f64 / n)).collect();
        let per = |row: [u64; 3]| row.map(|c| format!("{:.1}", c as f64 / n)).join("/");
        eprintln!(
            "prof {name}: {} | reused/moved/laid: para {} row {} page {}",
            stages.join(", "),
            per(self.reuse[0]),
            per(self.reuse[1]),
            per(self.reuse[2])
        );
    }
}

/// A xorshift generator: the same places on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % n.max(1) as u64) as usize
    }
}

/// Body paragraphs with their length in units and their table nesting depth, in document order.
fn paragraphs(blocks: &[Block], depth: usize, out: &mut Vec<(ParaId, u32, usize)>) {
    for b in blocks {
        match b {
            Block::Para(p) => out.push((ParaId(p.id), p.inlines.iter().map(|i| i.units() as u32).sum(), depth)),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| paragraphs(&c.blocks, depth + 1, out)),
        }
    }
}
