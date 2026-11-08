//! Saving reviewed documents: revisions and comments are written from the model, and what an edit didn't touch keeps
//! its bytes. Each review fixture is opened, one paragraph is edited, and the document is saved: every other paragraph
//! and part is byte-identical, and the review reads back the same. Revisions, comments and settings made in code are
//! written in their markup.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::docx::model::{Block, Document, Inline, Paragraph, Raw, Run, RunStyle};
use crate::docx::review::{
    AuthorId, CellMarkKind, CellRevisions, Comment, CommentId, FormatChange, GridChange, Identity, MarkKind, ParaChange, RangeKind, RangeMark,
    RevisionId, RevisionMark, RowRevisions, RunRevisions, SectionChange, TableChange, Timestamp,
};
use crate::docx::{read_editable, write};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn fixture(name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn review_fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).expect("the review fixtures").flatten().map(|e| e.path()).collect();
    v.retain(|p| p.extension().is_some_and(|x| x == "docx"));
    v.sort();
    assert!(!v.is_empty(), "no review fixtures in {}", dir.display());
    v
}

/// Every entry of a package, by name.
fn entries(package: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(package)).expect("a package");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("an entry");
            let mut b = Vec::new();
            f.read_to_end(&mut b).expect("readable");
            (f.name().expect("a UTF-8 name").to_string(), b)
        })
        .collect()
}

fn part(package: &[u8], name: &str) -> Option<String> {
    entries(package).into_iter().find(|(n, _)| n == name).map(|(_, b)| String::from_utf8(b).expect("UTF-8"))
}

fn main_part(doc: &Document, saved: &[u8]) -> String {
    let main = &doc.source.as_ref().expect("read for editing").main;
    part(saved, main).expect("a main part")
}

/// The XML of every `w:p` of a part, in document order.
fn paragraphs(xml: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let dom = roxmltree::Document::parse(xml).expect("well-formed XML");
    dom.descendants()
        .filter(|n| n.tag_name().namespace() == Some(W) && n.tag_name().name() == "p")
        .map(|n| (n.range(), xml[n.range()].to_string()))
        .collect()
}

/// Revision and comment markup in a part: how many of each element.
fn markup_counts(xml: &str) -> Vec<(&'static str, usize)> {
    const NAMES: [&str; 18] = [
        "ins",
        "del",
        "moveFrom",
        "moveTo",
        "delText",
        "rPrChange",
        "pPrChange",
        "sectPrChange",
        "tblPrChange",
        "trPrChange",
        "tcPrChange",
        "tblGridChange",
        "tblPrExChange",
        "cellIns",
        "cellDel",
        "cellMerge",
        "commentRangeStart",
        "commentReference",
    ];
    let dom = roxmltree::Document::parse(xml).expect("well-formed XML");
    NAMES.iter().map(|n| (*n, dom.descendants().filter(|d| d.tag_name().namespace() == Some(W) && d.tag_name().name() == *n).count())).collect()
}

/// The body paragraph to edit (section, block): the first whose source has no review markup, else the first with text.
fn paragraph_to_edit(doc: &Document) -> (usize, usize) {
    let src = doc.source.as_ref().expect("read for editing");
    let mut with_text = None;
    for (s, sec) in doc.sections.iter().enumerate() {
        for (b, block) in sec.blocks.iter().enumerate() {
            let Block::Para(p) = block else { continue };
            let (Some(ps), true) = (&p.src, p.inlines.iter().any(|i| matches!(i, Inline::Text(..)))) else { continue };
            let xml = &src.xml[ps.ranges[0].start..ps.ranges[ps.ranges.len() - 1].end];
            let marked = ["<w:ins ", "<w:del ", "<w:moveFrom ", "<w:moveTo ", "PrChange", "commentRange", "commentReference", "<w:sectPr"]
                .iter()
                .any(|m| xml.contains(m));
            if !marked {
                return (s, b);
            }
            with_text.get_or_insert((s, b));
        }
    }
    with_text.expect("a body paragraph with text")
}

fn para_mut(doc: &mut Document, (s, b): (usize, usize)) -> &mut Paragraph {
    match &mut doc.sections[s].blocks[b] {
        Block::Para(p) => p,
        Block::Table(_) => panic!("a paragraph"),
    }
}

/// Prefixes the paragraph's first text with "Edited ", in its run.
fn edit(doc: &mut Document, at: (usize, usize)) {
    let p = para_mut(doc, at);
    let k = p.inlines.iter().position(|i| matches!(i, Inline::Text(..))).expect("text");
    if let Inline::Text(s, run) = &p.inlines[k] {
        p.inlines[k] = Inline::Text(format!("Edited {s}"), run.clone());
    }
}

#[test]
fn an_unrelated_edit_keeps_every_other_paragraph_and_part() {
    for path in review_fixtures() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let bytes = std::fs::read(&path).expect("a fixture");
        let mut doc = read_editable(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let at = paragraph_to_edit(&doc);
        let edited = match &doc.sections[at.0].blocks[at.1] {
            Block::Para(p) => p.src.as_ref().map(|s| s.ranges[0].start..s.ranges[s.ranges.len() - 1].end).expect("a source"),
            Block::Table(_) => unreachable!("a paragraph was chosen"),
        };
        edit(&mut doc, at);
        let saved = write::save(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let src = doc.source.as_ref().expect("read for editing");
        let out = main_part(&doc, &saved);
        let (was, now) = (paragraphs(&src.xml), paragraphs(&out));
        assert_eq!(was.len(), now.len(), "{name}: paragraph count");
        let mut changed = 0;
        for ((r, a), (_, b)) in was.iter().zip(&now) {
            if r.start < edited.end && edited.start < r.end {
                changed += 1;
                continue;
            }
            assert_eq!(a, b, "{name}: an unedited paragraph changed");
        }
        assert!(changed >= 1, "{name}: the edited paragraph");
        assert!(out.contains("Edited "), "{name}: the edit was saved");
        assert_eq!(markup_counts(&src.xml), markup_counts(&out), "{name}: review markup");
        // Every other part is as read.
        let before = entries(&bytes);
        for (n, b) in entries(&saved) {
            if n == src.main {
                continue;
            }
            let a = before.iter().find(|(m, _)| *m == n).map(|(_, a)| a);
            assert_eq!(a, Some(&b), "{name}: {n} changed");
        }
    }
}

// ---------------- the review, compared ----------------

fn mark_key(doc: &Document, m: &Option<Rc<RevisionMark>>) -> String {
    m.as_ref().map_or(String::new(), |m| {
        let who = doc.people.get(m.author).map_or("?", |p| &p.name);
        format!("{:?}#{} {who} {:?} {:?}", m.kind, m.id.0, m.date.as_ref().map(|d| d.raw.clone()), m.move_name)
    })
}

fn run_key(doc: &Document, r: &Run) -> String {
    let fmt = r
        .rev
        .fmt
        .as_ref()
        .map_or(String::new(), |f| format!("fmt {} was b{} i{}", mark_key(doc, &Some(f.mark.clone())), f.previous.bold, f.previous.italic));
    format!("[{}|{}|{fmt}]", mark_key(doc, &r.rev.ins), mark_key(doc, &r.rev.del))
}

/// A story's review: each paragraph's text in spans of equal revisions, its markers, its mark's and properties'
/// revisions; tables' and rows' and cells' revisions; text boxes inside.
fn blocks_review(doc: &Document, blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                let mut spans: Vec<(String, String)> = Vec::new();
                for i in &p.inlines {
                    match i {
                        Inline::Text(s, r) => {
                            let k = run_key(doc, r);
                            match spans.last_mut() {
                                Some((t, key)) if *key == k => t.push_str(s),
                                _ => spans.push((s.clone(), k)),
                            }
                        }
                        Inline::Range(r) => spans.push((format!("<{:?} {} {:?}>", r.kind, r.id, r.name), String::new())),
                        Inline::CommentRef { id, .. } => spans.push((format!("<ref {}>", id.0), String::new())),
                        Inline::Float(f, _) => {
                            if let Some(tb) = &f.text_box {
                                blocks_review(doc, &tb.blocks, out);
                            }
                        }
                        _ => {}
                    }
                }
                let change = p.style.change.as_ref().map_or(String::new(), |c| mark_key(doc, &Some(c.mark.clone())));
                out.push(format!("p mark {} {change} {spans:?}", run_key(doc, &p.mark)));
            }
            Block::Table(t) => {
                let change = |c: &Option<Rc<TableChange>>| c.as_ref().map_or(String::new(), |c| mark_key(doc, &Some(c.mark.clone())));
                let grid = t.grid_change.as_ref().map_or(String::new(), |g| format!("{} {:?}", mark_key(doc, &Some(g.mark.clone())), g.previous));
                out.push(format!("table {} {grid}", change(&t.change)));
                for r in &t.rows {
                    out.push(format!(
                        "row {} {} {} {}",
                        mark_key(doc, &r.rev.ins),
                        mark_key(doc, &r.rev.del),
                        change(&r.rev.change),
                        change(&r.rev.ex_change)
                    ));
                    for c in &r.cells {
                        let s = c.rev.structural.as_ref().map_or(String::new(), |(m, k)| format!("{} {k:?}", mark_key(doc, &Some(m.clone()))));
                        out.push(format!("cell {s} {}", change(&c.rev.change)));
                        blocks_review(doc, &c.blocks, out);
                    }
                }
            }
        }
    }
}

/// Everything review about a document: revisions in every story, comments with their bodies and threads, people,
/// settings.
fn review(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for s in &doc.sections {
        out.push(format!("section {}", s.change.as_ref().map_or(String::new(), |c| mark_key(doc, &Some(c.mark.clone())))));
        blocks_review(doc, &s.blocks, &mut out);
    }
    let mut stories: Vec<&String> = doc.stories.keys().collect();
    stories.sort();
    for n in stories {
        out.push(format!("story {n}"));
        blocks_review(doc, &doc.stories[n], &mut out);
    }
    for (kind, notes) in [("footnote", &doc.footnotes), ("endnote", &doc.endnotes)] {
        let mut ids: Vec<&i64> = notes.keys().collect();
        ids.sort();
        for id in ids {
            out.push(format!("{kind} {id}"));
            blocks_review(doc, &notes[id], &mut out);
        }
    }
    for c in doc.comments.by_id.values() {
        let who = doc.people.get(c.author).map_or("?", |p| &p.name);
        let parent = c.parent.map(|p| p.0);
        out.push(format!(
            "comment {} {who} {:?} {:?} {:?} {:x} {:?} {parent:?} {}",
            c.id.0,
            c.initials,
            c.date.as_ref().map(|d| d.raw.clone()),
            c.date_utc.as_ref().map(|d| d.raw.clone()),
            c.para_id,
            c.durable_id,
            c.done
        ));
        blocks_review(doc, &c.body, &mut out);
    }
    out.push(format!("threads {:?}", doc.comments.threads()));
    for p in &doc.people.persons {
        out.push(format!("person {} {:?}", p.name, p.identity));
    }
    out.push(format!("tracking {} {:?}", doc.settings.track_revisions, doc.settings.protection.as_ref().map(|p| (p.edit, p.enforced))));
    out
}

#[test]
fn the_review_reads_back_after_an_unrelated_edit() {
    for path in review_fixtures().into_iter().filter(|p| !p.to_string_lossy().contains("MALFORMED")) {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut doc = read_editable(&std::fs::read(&path).expect("a fixture")).unwrap_or_else(|e| panic!("{name}: {e}"));
        let at = paragraph_to_edit(&doc);
        edit(&mut doc, at);
        let saved = write::save(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let again = read_editable(&saved).unwrap_or_else(|e| panic!("{name}: reads back: {e}"));
        let (a, b) = (review(&doc), review(&again));
        assert_eq!(a.len(), b.len(), "{name}: {a:#?}\n{b:#?}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x, y, "{name}");
        }
    }
}

// ---------------- revisions made in code ----------------

fn mark(doc: &mut Document, id: i64, who: &str, kind: MarkKind) -> Rc<RevisionMark> {
    let author = doc.people.author(who, None);
    Rc::new(RevisionMark { id: RevisionId(id), author, date: Some(Timestamp::parse("2026-03-01T10:00:00Z")), kind, move_name: None })
}

fn with_rev(run: &Run, f: impl FnOnce(&mut RunRevisions)) -> Run {
    let mut s = (**run).clone();
    f(&mut s.rev);
    Rc::new(s)
}

/// FX-TEXT's "Short paragraph." (section 0, block 1) and its run.
fn short_paragraph(doc: &Document) -> ((usize, usize), Run) {
    let at = (0, 1);
    let Block::Para(p) = &doc.sections[at.0].blocks[at.1] else { panic!("a paragraph") };
    let Some(Inline::Text(_, run)) = p.inlines.first() else { panic!("text first") };
    (at, run.clone())
}

/// The element holding the `w:t` or `w:delText` with this text, walking up `up` levels.
fn holder<'a, 'i>(dom: &'a roxmltree::Document<'i>, text: &str, up: usize) -> roxmltree::Node<'a, 'i> {
    let t = dom
        .descendants()
        .find(|n| matches!(n.tag_name().name(), "t" | "delText") && n.text() == Some(text))
        .unwrap_or_else(|| panic!("{text:?} is written"));
    (0..up).fold(t, |n, _| n.parent().expect("a parent"))
}

/// Text in an insertion, a deletion, a deleted insertion and a formatting change, written into a paragraph.
fn tracked_runs(doc: &mut Document) -> Vec<u8> {
    let (at, base) = short_paragraph(doc);
    let (ins, del) = (mark(doc, 9001, "Author A", MarkKind::Insert), mark(doc, 9002, "Author B", MarkKind::Delete));
    let (ins2, del2) = (mark(doc, 9003, "Author A", MarkKind::Insert), mark(doc, 9004, "Author B", MarkKind::Delete));
    let fmt = mark(doc, 9005, "Author C", MarkKind::Format);
    let inserted = with_rev(&base, |r| r.ins = Some(ins.clone()));
    let mut bold = (*inserted).clone();
    bold.direct = Raw(Some("<w:rPr><w:b/></w:rPr>".into()));
    let bold = Rc::new(bold);
    let deleted = with_rev(&base, |r| r.del = Some(del));
    let both = with_rev(&base, |r| {
        r.ins = Some(ins2);
        r.del = Some(del2);
    });
    let mut formatted = (*with_rev(&base, |r| {
        let previous = Rc::new(RunStyle { italic: true, ..(*base).clone() });
        r.fmt = Some(Rc::new(FormatChange { mark: fmt, previous, previous_raw: Raw(Some("<w:rPr><w:i/></w:rPr>".into())) }))
    }))
    .clone();
    formatted.direct = Raw(Some("<w:rPr><w:b/></w:rPr>".into()));
    let formatted = Rc::new(formatted);
    let p = para_mut(doc, at);
    p.inlines = vec![
        Inline::Text("Short ".into(), base.clone()),
        Inline::Text("inserted ".into(), inserted),
        Inline::Text("inserted bold ".into(), bold),
        Inline::Text("deleted ".into(), deleted),
        Inline::Text("both ".into(), both),
        Inline::Text("formatted".into(), formatted),
        Inline::Text(" paragraph.".into(), base),
    ];
    write::save(doc).expect("saves")
}

#[test]
fn tracked_runs_are_written_in_their_wrappers() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let saved = tracked_runs(&mut doc);
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    let ins = holder(&dom, "inserted ", 2);
    assert_eq!((ins.tag_name().name(), ins.attribute((W, "id")), ins.attribute((W, "author"))), ("ins", Some("9001"), Some("Author A")));
    assert_eq!(ins.attribute((W, "date")), Some("2026-03-01T10:00:00Z"));
    // Runs with the same marks share one wrapper, whatever their formatting.
    assert_eq!(holder(&dom, "inserted bold ", 2), ins);
    let del = holder(&dom, "deleted ", 2);
    assert_eq!((del.tag_name().name(), del.attribute((W, "id"))), ("del", Some("9002")));
    let inner = holder(&dom, "both ", 2);
    let outer = inner.parent().expect("a parent");
    assert_eq!((inner.tag_name().name(), inner.attribute((W, "id"))), ("del", Some("9004")));
    assert_eq!((outer.tag_name().name(), outer.attribute((W, "id"))), ("ins", Some("9003")));
    assert_eq!(holder(&dom, "both ", 0).tag_name().name(), "delText");
    let rpr = holder(&dom, "formatted", 1).children().find(|c| c.tag_name().name() == "rPr").expect("properties");
    let last = rpr.last_element_child().expect("a change");
    assert_eq!((last.tag_name().name(), last.attribute((W, "id"))), ("rPrChange", Some("9005")));
    assert_eq!(&xml[last.range()].matches("<w:i/>").count(), &1, "the earlier properties");
    // Untracked text sits outside every wrapper.
    assert_eq!(holder(&dom, "Short ", 2).tag_name().name(), "p");
}

#[test]
fn tracked_runs_read_back() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let saved = tracked_runs(&mut doc);
    let again = read_editable(&saved).expect("reads back");
    let at = (0, 1);
    let (Block::Para(a), Block::Para(b)) = (&doc.sections[at.0].blocks[at.1], &again.sections[at.0].blocks[at.1]) else { panic!("paragraphs") };
    let mut x = Vec::new();
    blocks_review(&doc, &[Block::Para(a.clone())], &mut x);
    let mut y = Vec::new();
    blocks_review(&again, &[Block::Para(b.clone())], &mut y);
    assert_eq!(x, y);
}

#[test]
fn text_typed_into_an_insertion_without_its_mark_goes_outside_it() {
    let mut doc = read_editable(&fixture("review/FX-REVIEW-RUNS.docx")).expect("reads");
    let (s, b) = (0, 0);
    let p = para_mut(&mut doc, (s, b));
    let k = p
        .inlines
        .iter()
        .position(|i| matches!(i, Inline::Text(t, _) if t.contains("Inventory relevant group relief results region ledger brief usage. ")))
        .expect("the inserted text");
    let Inline::Text(text, run) = p.inlines[k].clone() else { unreachable!("found as text") };
    let at = text.find("Inventory relevant").expect("there") + "Inventory ".len();
    let plain = with_rev(&run, |r| *r = RunRevisions::default());
    p.inlines.splice(
        k..=k,
        [Inline::Text(text[..at].to_string(), run.clone()), Inline::Text("TYPED ".into(), plain), Inline::Text(text[at..].to_string(), run)],
    );
    let saved = write::save(&doc).expect("saves");
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    let typed = holder(&dom, "TYPED ", 2);
    assert_eq!(typed.tag_name().name(), "p", "{}", &xml[typed.range()]);
    assert_eq!(
        markup_counts(&doc.source.as_ref().expect("source").xml),
        markup_counts(&xml).iter().map(|&(n, c)| (n, if n == "ins" { c - 1 } else { c })).collect::<Vec<_>>()
    );
}

#[test]
fn paragraph_marks_and_property_changes_are_written_in_schema_order() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let (at, _) = short_paragraph(&doc);
    let ins = mark(&mut doc, 9101, "Author A", MarkKind::Insert);
    let change = mark(&mut doc, 9102, "Author B", MarkKind::Insert);
    let p = para_mut(&mut doc, at);
    p.mark = with_rev(&p.mark, |r| r.ins = Some(ins));
    let mut style = (*p.style).clone();
    style.direct = Raw(Some("<w:pPr><w:jc w:val=\"center\"/></w:pPr>".into()));
    style.change = Some(Rc::new(ParaChange {
        mark: change,
        previous: p.style.clone(),
        previous_raw: Raw(Some("<w:pPr><w:jc w:val=\"left\"/></w:pPr>".into())),
        numbering_change: Raw(None),
    }));
    p.style = Rc::new(style);
    let saved = write::save(&doc).expect("saves");
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    let ppr = holder(&dom, "Short paragraph.", 2).children().find(|c| c.tag_name().name() == "pPr").expect("properties");
    let kids: Vec<&str> = ppr.children().filter(|c| c.is_element()).map(|c| c.tag_name().name()).collect();
    assert_eq!(kids, ["jc", "rPr", "pPrChange"], "{}", &xml[ppr.range()]);
    let rpr = ppr.children().find(|c| c.tag_name().name() == "rPr").expect("mark properties");
    let first = rpr.first_element_child().expect("a mark");
    assert_eq!((first.tag_name().name(), first.attribute((W, "id"))), ("ins", Some("9101")));
    let pc = ppr.children().find(|c| c.tag_name().name() == "pPrChange").expect("a change");
    assert_eq!(pc.attribute((W, "author")), Some("Author B"));
    assert!(xml[pc.range()].contains("<w:jc w:val=\"left\"/>"));
}

#[test]
fn table_row_and_cell_revisions_are_written() {
    let mut doc = read_editable(&fixture("FX-TBL-BASIC.docx")).expect("reads");
    let (row, cell, grid, tbl) = (
        mark(&mut doc, 9201, "Author A", MarkKind::Insert),
        mark(&mut doc, 9202, "Author B", MarkKind::Delete),
        mark(&mut doc, 9203, "Author C", MarkKind::Insert),
        mark(&mut doc, 9204, "Author C", MarkKind::Insert),
    );
    let Some(Block::Table(t)) = doc.sections[0].blocks.first_mut() else { panic!("FX-TBL-BASIC starts with a table") };
    t.grid_change = Some(Rc::new(GridChange { mark: grid, previous: vec![100.0; t.grid.len()] }));
    t.change = Some(Rc::new(TableChange {
        mark: tbl,
        previous_raw: Raw(Some("<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr>".into())),
        previous: None,
    }));
    t.rows[1].rev = RowRevisions { ins: Some(row), ..Default::default() };
    t.rows[0].cells[2].rev = CellRevisions { structural: Some((cell, CellMarkKind::Delete)), change: None };
    let saved = write::save(&doc).expect("saves");
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    let el = |name: &str| dom.descendants().find(|n| n.tag_name().name() == name).unwrap_or_else(|| panic!("{name} is written"));
    let g = el("tblGridChange");
    assert_eq!((g.parent().map(|p| p.tag_name().name()), g.attribute((W, "id"))), (Some("tblGrid"), Some("9203")));
    assert!(xml[g.range()].contains("<w:gridCol w:w=\"2000\"/>"));
    let tc = el("tblPrChange");
    assert_eq!(tc.parent().and_then(|p| p.last_element_child()), Some(tc), "last in the table properties");
    let ins = el("ins");
    assert_eq!((ins.parent().map(|p| p.tag_name().name()), ins.attribute((W, "id"))), (Some("trPr"), Some("9201")));
    let del = el("cellDel");
    assert_eq!((del.parent().map(|p| p.tag_name().name()), del.attribute((W, "author"))), (Some("tcPr"), Some("Author B")));
    assert!(xml[del.parent().expect("tcPr").parent().expect("tc").range()].contains("C1"), "the third cell of the first row");
}

#[test]
fn section_changes_are_written_last() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let m = mark(&mut doc, 9301, "Author A", MarkKind::Insert);
    let previous = doc.sections[0].page();
    doc.sections[0].left += 36.0;
    doc.sections[0].change = Some(Rc::new(SectionChange {
        mark: m,
        previous,
        previous_raw: Raw(Some("<w:sectPr><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\"/></w:sectPr>".into())),
    }));
    doc.edited_sections.insert(0);
    let saved = write::save(&doc).expect("saves");
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    let body = dom.descendants().find(|n| n.tag_name().name() == "body").expect("a body");
    let sect = body.children().rfind(|n| n.tag_name().name() == "sectPr").expect("a section");
    let last = sect.last_element_child().expect("children");
    assert_eq!((last.tag_name().name(), last.attribute((W, "id"))), ("sectPrChange", Some("9301")));
    assert_eq!(sect.children().filter(|c| c.tag_name().name() == "pgMar").count(), 1);
}

/// A comment on "Short paragraph." by a person with an identity, with its range, its reference and a reply.
fn commented(doc: &mut Document) -> Vec<u8> {
    let (at, base) = short_paragraph(doc);
    let author = doc.people.author("Author A", Some(&Identity { provider: "AD".into(), user_id: "a@example.com".into() }));
    let body = |text: &str| {
        let mut p = match &doc.sections[at.0].blocks[at.1] {
            Block::Para(p) => p.clone(),
            Block::Table(_) => unreachable!("a paragraph"),
        };
        p.src = None;
        p.inlines = vec![Inline::Text(text.into(), base.clone())];
        vec![Block::Para(p)]
    };
    let c = |id: i64, para: u32, parent: Option<i64>, text: &str| Comment {
        id: CommentId(id),
        author,
        initials: Some("AA".into()),
        date: Some(Timestamp::parse("2026-03-02T09:00:00Z")),
        date_utc: Some(Timestamp::parse("2026-03-02T09:00:00Z")),
        body: body(text),
        para_id: para,
        durable_id: Some(0x5A00_0000 + para),
        parent: parent.map(CommentId),
        done: false,
        extensible: Raw(None),
    };
    for x in [c(40, 0x40, None, "A new comment."), c(41, 0x41, Some(40), "A reply.")] {
        doc.comments.by_id.insert(x.id, x);
    }
    let p = para_mut(doc, at);
    let range = |kind| Inline::Range(RangeMark { kind, id: 40, name: None });
    p.inlines.insert(0, range(RangeKind::CommentStart));
    p.inlines.push(range(RangeKind::CommentEnd));
    p.inlines.push(Inline::CommentRef { id: CommentId(40), run: base });
    write::save(doc).expect("saves")
}

#[test]
fn new_comments_are_written_with_their_parts() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let saved = commented(&mut doc);
    let xml = main_part(&doc, &saved);
    let p = paragraphs(&xml).into_iter().map(|(_, x)| x).find(|x| x.contains("Short paragraph.")).expect("the paragraph");
    let (start, end, reference) =
        (p.find("<w:commentRangeStart w:id=\"40\"/>"), p.find("<w:commentRangeEnd w:id=\"40\"/>"), p.find("<w:commentReference w:id=\"40\"/>"));
    assert!(start < end && end < reference && start.is_some(), "{p}");
    let comments = part(&saved, "word/comments.xml").expect("a comments part");
    assert!(comments.contains("<w:comment w:id=\"40\" w:author=\"Author A\" w:date=\"2026-03-02T09:00:00Z\" w:initials=\"AA\">"), "{comments}");
    assert!(comments.contains("w14:paraId=\"00000041\""), "{comments}");
    let ex = part(&saved, "word/commentsExtended.xml").expect("a commentsExtended part");
    assert!(ex.contains("w15:paraId=\"00000041\" w15:paraIdParent=\"00000040\""), "{ex}");
    assert!(part(&saved, "word/commentsIds.xml").expect("ids").contains("w16cid:durableId=\"5A000040\""));
    assert!(part(&saved, "word/commentsExtensible.xml").expect("extensible").contains("w16cex:dateUtc=\"2026-03-02T09:00:00Z\""));
    assert!(part(&saved, "word/people.xml").expect("people").contains("w15:userId=\"a@example.com\""));
    let types = part(&saved, "[Content_Types].xml").expect("types");
    let rels = part(&saved, "word/_rels/document.xml.rels").expect("relationships");
    for f in ["comments", "commentsExtended", "commentsIds", "commentsExtensible", "people"] {
        assert!(types.contains(&format!("PartName=\"/word/{f}.xml\"")), "{f} type");
        assert!(rels.contains(&format!("Target=\"{f}.xml\"")), "{f} relationship");
    }
    for (n, x) in entries(&saved) {
        if n.ends_with(".xml") || n.ends_with(".rels") {
            roxmltree::Document::parse(std::str::from_utf8(&x).expect("UTF-8")).unwrap_or_else(|e| panic!("{n}: {e}"));
        }
    }
}

#[test]
fn new_comments_read_back() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let saved = commented(&mut doc);
    let again = read_editable(&saved).expect("reads back");
    assert_eq!(review(&doc), review(&again));
}

#[test]
fn changed_comments_are_written_and_others_kept() {
    let bytes = fixture("review/FX-REVIEW-COMMENTS.docx");
    let mut doc = read_editable(&bytes).expect("reads");
    // Unchanged: every part as read.
    assert_eq!(write::save(&doc).expect("saves"), bytes);
    let reply = Comment {
        id: CommentId(77),
        author: AuthorId(0),
        initials: None,
        date: None,
        date_utc: None,
        body: Vec::new(),
        para_id: 0x77,
        durable_id: None,
        parent: None,
        done: true,
        extensible: Raw(None),
    };
    doc.people.author("Author A", None);
    doc.comments.by_id.insert(reply.id, reply);
    let saved = write::save(&doc).expect("saves");
    let comments = part(&saved, "word/comments.xml").expect("comments");
    assert!(comments.contains("<w:comment w:id=\"77\""), "{comments}");
    assert!(part(&saved, "word/commentsExtended.xml").expect("extended").contains("w15:paraId=\"00000077\" w15:done=\"1\""));
    assert_eq!(part(&saved, "word/document.xml"), part(&bytes, "word/document.xml"), "the body is untouched");
}

#[test]
fn tracking_is_turned_on_in_the_settings() {
    let bytes = fixture("FX-TEXT.docx");
    let mut doc = read_editable(&bytes).expect("reads");
    doc.settings.track_revisions = true;
    let saved = write::save(&doc).expect("saves");
    let settings = part(&saved, "word/settings.xml").expect("settings");
    assert!(settings.contains("<w:trackRevisions/><w:defaultTabStop"), "{settings}");
    let before = part(&bytes, "word/settings.xml").expect("settings");
    assert_eq!(settings.replace("<w:trackRevisions/>", ""), before, "nothing else changed");
}

#[test]
fn tracking_is_turned_off_and_protection_kept() {
    let mut doc = read_editable(&fixture("review/FX-REVIEW-PROTECTED.docx")).expect("reads");
    assert!(doc.settings.track_revisions, "read on");
    doc.settings.track_revisions = false;
    let saved = write::save(&doc).expect("saves");
    let settings = part(&saved, "word/settings.xml").expect("settings");
    assert!(!settings.contains("trackRevisions"), "{settings}");
    assert!(settings.contains("<w:documentProtection w:edit=\"trackedChanges\" w:enforcement=\"1\"/>"), "{settings}");
}

/// A comment range added in a paragraph with revisions: the paragraph is written from the model, with its revisions
/// and markers.
#[test]
fn a_paragraph_written_from_the_model_keeps_its_review() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    tracked_runs(&mut doc);
    let saved = commented(&mut doc);
    let xml = main_part(&doc, &saved);
    let dom = roxmltree::Document::parse(&xml).expect("well-formed");
    assert_eq!(holder(&dom, "inserted ", 2).attribute((W, "id")), Some("9001"));
    assert_eq!(holder(&dom, "both ", 2).parent().and_then(|p| p.attribute((W, "id"))), Some("9003"));
    let p = paragraphs(&xml).into_iter().map(|(_, x)| x).find(|x| x.contains("inserted ")).expect("the paragraph");
    assert!(p.starts_with("<w:p>") && p.contains("<w:commentRangeStart w:id=\"40\"/>") && p.contains("<w:commentReference w:id=\"40\"/>"), "{p}");
    assert!(p.find("<w:commentRangeStart").expect("a start") < p.find("Short").expect("the text"), "{p}");
}

#[test]
fn moved_text_is_written_with_its_range() {
    let mut doc = read_editable(&fixture("FX-TEXT.docx")).expect("reads");
    let (at, base) = short_paragraph(&doc);
    let mut to = (*mark(&mut doc, 9401, "Author A", MarkKind::MoveTo)).clone();
    to.move_name = Some("move1".into());
    let moved = with_rev(&base, |r| r.ins = Some(Rc::new(to)));
    let range = |kind| Inline::Range(RangeMark { kind, id: 9400, name: Some("move1".into()) });
    let p = para_mut(&mut doc, at);
    p.inlines = vec![range(RangeKind::MoveToStart), Inline::Text("Moved here.".into(), moved), range(RangeKind::MoveToEnd)];
    let saved = write::save(&doc).expect("saves");
    let xml = main_part(&doc, &saved);
    let p = paragraphs(&xml).into_iter().map(|(_, x)| x).find(|x| x.contains("Moved here.")).expect("the paragraph");
    assert!(
        p.starts_with(
            r#"<w:p><w:moveToRangeStart w:id="9400" w:author="Author A" w:date="2026-03-01T10:00:00Z" w:name="move1"/><w:moveTo w:id="9401""#
        ),
        "{p}"
    );
    assert!(p.ends_with(r#"</w:moveTo><w:moveToRangeEnd w:id="9400"/></w:p>"#), "{p}");
}

#[test]
fn a_reviewed_paragraph_rewritten_from_the_model_reads_back() {
    let mut doc = read_editable(&fixture("review/FX-REVIEW-MOVES.docx")).expect("reads");
    // A new comment range forces the moved paragraph to be written from the model.
    let at = doc.sections[0]
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Text(_, r) if r.rev.del.is_some()))))
        .expect("a moved paragraph");
    let author = doc.people.author("Author A", None);
    let Block::Para(body) = &doc.sections[0].blocks[0] else { panic!("a paragraph") };
    let body = vec![Block::Para(Paragraph { src: None, ..body.clone() })];
    let comment = Comment {
        id: CommentId(500),
        author,
        initials: None,
        date: Some(Timestamp::parse("2026-03-02T09:00:00Z")),
        date_utc: None,
        body,
        para_id: 0x500,
        durable_id: None,
        parent: None,
        done: false,
        extensible: Raw(None),
    };
    doc.comments.by_id.insert(comment.id, comment);
    let p = para_mut(&mut doc, (0, at));
    let run = p.mark.clone();
    // After the move range start read from between the paragraphs: markers at one place keep the order they are
    // written in, and that one stays outside the paragraph.
    let first = p.inlines.iter().take_while(|i| matches!(i, Inline::Range(_))).count();
    p.inlines.insert(first, Inline::Range(RangeMark { kind: RangeKind::CommentStart, id: 500, name: None }));
    p.inlines.push(Inline::Range(RangeMark { kind: RangeKind::CommentEnd, id: 500, name: None }));
    p.inlines.push(Inline::CommentRef { id: CommentId(500), run });
    let saved = write::save(&doc).expect("saves");
    let again = read_editable(&saved).expect("reads back");
    let (a, b) = (review(&doc), review(&again));
    assert_eq!(a.iter().filter(|l| l.starts_with("p ")).collect::<Vec<_>>(), b.iter().filter(|l| l.starts_with("p ")).collect::<Vec<_>>());
}
