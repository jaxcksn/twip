//! Robustness

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::docx::model::next_para_id;
use crate::docx::props::Width;
use crate::docx::{Block, Document, Inline, NoteKind, Paragraph, Picture, Table};
use crate::font::Fonts;
use crate::layout::layout;

use super::{doc, fixture, fixtures, fonts};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/editor/FX-OBJ.docx");

fn try_layout(d: &Document, fonts: &Fonts) -> Result<(), String> {
    match catch_unwind(AssertUnwindSafe(|| layout(d, fonts))) {
        Err(_) => Err("panicked".into()),
        Ok(s) => match s.pages().iter().position(|p| !(p.w > 0.0 && p.h > 0.0)) {
            Some(i) => Err(format!("page {} has size {} x {}", i + 1, s.pages()[i].w, s.pages()[i].h)),
            None => Ok(()),
        },
    }
}

fn first_para(d: &Document) -> Paragraph {
    d.sections.iter().flat_map(|s| &s.blocks).find_map(|b| if let Block::Para(p) = b { Some(p.clone()) } else { None }).expect("a paragraph")
}

fn para_with(template: &Paragraph, inlines: Vec<Inline>) -> Block {
    let mut p = template.clone();
    p.inlines = inlines;
    p.id = next_para_id();
    p.src = None;
    Block::Para(p)
}

fn first_table(d: &Document) -> Table {
    d.sections.iter().flat_map(|s| &s.blocks).find_map(|b| if let Block::Table(t) = b { Some(t.clone()) } else { None }).expect("a table")
}

#[test]
fn odd_documents_lay_out() {
    let (Some(sample), Some(tables)) = (doc(Path::new(SAMPLE)), fixture(r"T1\T1a_tiny.docx")) else { return };
    let p = first_para(&sample);
    let run = p.mark.clone();
    let sized = |size: f64| Rc::new(crate::docx::model::RunStyle { size, ..(*run).clone() });
    let text = |s: &str, r: &crate::docx::Run| para_with(&p, vec![Inline::Text(s.into(), r.clone())]);
    let mut cases: Vec<(&str, Document)> = Vec::new();
    let with = |base: &Document, f: &dyn Fn(&mut Document)| {
        let mut d = base.clone();
        f(&mut d);
        d
    };
    cases.push(("no blocks", with(&sample, &|d| d.sections.iter_mut().for_each(|s| s.blocks.clear()))));
    cases.push((
        "an empty section",
        with(&sample, &|d| {
            let mut s = d.sections[0].clone();
            s.blocks.clear();
            d.sections.push(s);
        }),
    ));
    cases.push(("page narrower than its margins", with(&sample, &|d| d.sections.iter_mut().for_each(|s| s.page_w = s.left + s.right - 20.0))));
    cases.push((
        "negative margins",
        with(&sample, &|d| d.sections.iter_mut().for_each(|s| (s.left, s.right, s.top, s.bottom) = (-40.0, -40.0, -30.0, -30.0))),
    ));
    cases.push((
        "zero margins",
        with(&sample, &|d| {
            d.sections.iter_mut().for_each(|s| (s.left, s.right, s.top, s.bottom, s.header, s.footer) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0))
        }),
    ));
    let table_case = |f: &dyn Fn(&mut Table)| {
        let mut t = first_table(&tables);
        f(&mut t);
        with(&tables, &|d| d.sections[0].blocks = vec![Block::Table(t.clone())])
    };
    cases.push(("a 0-row table", table_case(&|t| t.rows.clear())));
    cases.push(("a row with no cells", table_case(&|t| t.rows[0].cells.clear())));
    cases.push((
        "a cell wider than the page",
        table_case(&|t| {
            t.grid = vec![5000.0; t.grid.len().max(1)];
            t.fixed = true;
            for c in &mut t.rows[0].cells {
                c.width = Some(Width::Dxa(5000.0));
            }
        }),
    ));
    cases.push(("4000 characters without a break", with(&sample, &|d| d.sections[0].blocks.insert(0, text(&"x".repeat(4000), &run)))));
    cases.push(("1pt text", with(&sample, &|d| d.sections[0].blocks.insert(0, text("tiny text", &sized(1.0))))));
    cases.push(("1638pt text", with(&sample, &|d| d.sections[0].blocks.insert(0, text("Huge", &sized(1638.0))))));
    let missing =
        Rc::new(crate::docx::model::RunStyle { ascii: "No Such Font Family".into(), h_ansi: "No Such Font Family".into(), ..(*run).clone() });
    cases.push(("a font that isn't installed", with(&sample, &|d| d.sections[0].blocks.insert(0, text("Missing font", &missing)))));
    let picture = para_with(
        &p,
        vec![Inline::Picture(
            Picture { w: 0.0, h: 0.0, image: None, alt: "".into(), lock_aspect: false, effect: [0.0; 4], shape: None, text: None },
            run.clone(),
        )],
    );
    cases.push(("a zero-size picture", with(&sample, &|d| d.sections[0].blocks.insert(0, picture.clone()))));
    let dangling = para_with(
        &p,
        vec![Inline::Text("See".into(), run.clone()), Inline::NoteRef { kind: NoteKind::Foot, id: 424242, custom: false, run: run.clone() }],
    );
    cases.push(("a reference to a missing footnote", with(&sample, &|d| d.sections[0].blocks.insert(0, dangling.clone()))));
    let mut nested = first_table(&tables);
    for _ in 0..50 {
        let mut outer = first_table(&tables);
        outer.id = next_para_id();
        outer.rows.truncate(1);
        outer.rows[0].cells.truncate(1);
        outer.rows[0].cells[0].blocks = vec![Block::Table(nested), para_with(&p, Vec::new())];
        nested = outer;
    }
    cases.push(("50 nested tables", with(&tables, &|d| d.sections[0].blocks = vec![Block::Table(nested.clone())])));
    let fonts = fonts();
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(name, d)| {
            eprintln!("odd document: {name}");
            try_layout(d, &fonts).err().map(|e| format!("{name}: {e}"))
        })
        .collect();
    assert!(failures.is_empty(), "odd documents that didn't lay out:\n{}", failures.join("\n"));
}

fn fixture_paths(n: usize) -> Vec<PathBuf> {
    let Ok(dirs) = std::fs::read_dir(fixtures()) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = dirs.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(files) = std::fs::read_dir(&d) else { continue };
        let mut files: Vec<PathBuf> = files.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "docx")).collect();
        files.sort();
        out.extend(files.into_iter().take(1));
        if out.len() == n {
            break;
        }
    }
    out
}

#[test]
fn truncated_files_never_panic() {
    let fonts = fonts();
    let mut failures = Vec::new();
    for path in fixture_paths(20) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let mut variants: Vec<(String, Vec<u8>)> =
            (1..=10).map(|k| (format!("truncated at {k}/11"), bytes[..bytes.len() * k / 11].to_vec())).collect();
        let name = b"word/document.xml";
        if let Some(at) = bytes.windows(name.len()).position(|w| w == name) {
            for k in 0..5 {
                let mut b = bytes.clone();
                let i = (at + name.len() + 40 + k * 97).min(b.len() - 1);
                b[i] ^= 0x5A;
                variants.push((format!("byte {i} flipped"), b));
            }
        }
        for (what, b) in variants {
            let read = catch_unwind(|| crate::docx::read(&b).ok());
            match read {
                Err(_) => failures.push(format!("{} {what}: docx::read panicked", path.display())),
                Ok(Some(d)) => {
                    if let Err(e) = try_layout(&d, &fonts) {
                        failures.push(format!("{} {what}: {e}", path.display()));
                    }
                }
                Ok(None) => {}
            }
        }
    }
    assert!(failures.is_empty(), "damaged files that panicked:\n{}", failures.join("\n"));
}

/// A package of just `word/document.xml` with `body`, plus `extra` parts.
fn hostile(body: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let doc = format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#);
    let mut parts = vec![("word/document.xml", doc.as_str())];
    parts.extend_from_slice(extra);
    super::package_bytes(&parts).expect("a package")
}

/// Reads and lays out `bytes` on a thread with a 1 MB stack (a browser's, or a host thread's): `Err` is a refusal,
/// and the test fails if the thread doesn't finish.
fn read_on_small_stack(bytes: Vec<u8>) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let d = crate::docx::read(&bytes)?;
            try_layout(&d, &fonts())
        })
        .expect("a thread")
        .join()
        .expect("finished without overflowing its stack")
}

#[test]
fn hostile_packages_are_refused_or_read() {
    let tables = |n: usize| format!("{}<w:p/>{}", "<w:tbl><w:tr><w:tc>".repeat(n), "<w:p/></w:tc></w:tr></w:tbl>".repeat(n));
    // Nested tables to the depth limit lay out; one more level is refused.
    assert_eq!(read_on_small_stack(hostile(&tables(31), &[])), Ok(()));
    assert!(read_on_small_stack(hostile(&tables(32), &[])).is_err());
    let controls = format!("{}<w:p/>{}", "<w:sdt><w:sdtContent>".repeat(20_000), "</w:sdtContent></w:sdt>".repeat(20_000));
    assert!(read_on_small_stack(hostile(&controls, &[])).is_err());
    // A list starting at the smallest number there is.
    let numbering = r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="-9223372036854775808"/><w:numFmt w:val="cardinalText"/><w:lvlText w:val="%1."/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;
    let item = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#;
    let listed = hostile(&item.repeat(2), &[("word/numbering.xml", numbering), ("word/_rels/document.xml.rels", rels)]);
    assert_eq!(read_on_small_stack(listed), Ok(()));
}

#[test]
fn hostile_values_in_a_package_are_bounded() {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/numbering" Target="numbering.xml"/><Relationship Id="rId2" Type="{REL}/settings" Target="settings.xml"/></Relationships>"#
    );
    let settings = format!(
        r#"<w:settings xmlns:w="{W}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#
    );
    let read = |body: &str, numbering: &str| {
        let numbering = format!(r#"<w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#);
        read_on_small_stack(hostile(
            body,
            &[("word/numbering.xml", &numbering), ("word/settings.xml", &settings), ("word/_rels/document.xml.rels", &rels)],
        ))
    };
    // A chicago list starting at the largest number there is: one symbol a repeat would be exabytes.
    let numbering = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="9223372036854775807"/><w:numFmt w:val="chicago"/><w:lvlText w:val="%1"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;
    let item = r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#;
    assert_eq!(read(item, numbering), Ok(()), "chicago");
    // Cells spanning, and rows skipping, two billion grid columns.
    let table = r#"<w:tbl><w:tr><w:trPr><w:gridBefore w:val="2000000000"/><w:gridAfter w:val="9223372036854775807"/></w:trPr><w:tc><w:tcPr><w:gridSpan w:val="2000000000"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:gridSpan w:val="9223372036854775807"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>"#;
    assert_eq!(read(table, ""), Ok(()), "grid spans");
    let anchor = |allow: u8, size: u32, h: &str, extra: &str| {
        format!(
            concat!(
                r#"<w:p><w:r><w:drawing xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
                r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing">"#,
                r#"<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="{allow}">"#,
                r#"<wp:simplePos x="0" y="0"/>{h}<wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#,
                r#"<wp:extent cx="{size}" cy="{size}"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="x"/>"#,
                r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill/><pic:spPr><a:xfrm><a:off x="9223372036854775807" y="-9223372036854775808"/><a:ext cx="127000" cy="127000"/></a:xfrm></pic:spPr></pic:pic></a:graphicData></a:graphic>{extra}</wp:anchor></w:drawing></w:r></w:p>"#
            ),
            allow = allow,
            size = size,
            h = h,
            extra = extra,
        )
    };
    // Percentage positions and sizes of the largest value there is, and an offset past any page.
    let pct = anchor(
        1,
        127_000,
        r#"<wp:positionH relativeFrom="page"><wp14:pctPosHOffset>9223372036854775807</wp14:pctPosHOffset></wp:positionH>"#,
        r#"<wp14:sizeRelH relativeFrom="page"><wp14:pctWidth>9223372036854775807</wp14:pctWidth></wp14:sizeRelH><wp14:sizeRelV relativeFrom="page"><wp14:pctHeight>-9223372036854775808</wp14:pctHeight></wp14:sizeRelV>"#,
    );
    assert_eq!(read(&pct, ""), Ok(()), "wp14 percentages");
    // More pictures that may not overlap than a page holds, all asked for in one place: compat 15 searches for room
    // for each, and those that find none try every place.
    // Each a different size, so that no two edges are the same place to try: 600 took minutes without a bound.
    let at = r#"<wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH>"#;
    let crowd: String = (0..600).map(|i| anchor(0, 127_000 + i * 1_270, at, "")).collect::<String>().replace("</w:r></w:p><w:p><w:r>", "</w:r><w:r>");
    let started = std::time::Instant::now();
    assert_eq!(read(&crowd, ""), Ok(()), "a crowd of floats");
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "laid out in {:?}", started.elapsed());
}
