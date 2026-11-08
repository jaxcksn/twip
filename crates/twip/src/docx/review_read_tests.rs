//! Reading tracked revisions and comments: the generated review fixtures (`FX-REVIEW-*`) hold in the model what
//! their builder wrote.

use std::path::Path;

use super::diagnostics::WarningCode;
use super::review::{CellMarkKind, CommentId, MarkKind, ProtectionKind, RangeKind, RevisionMark};
use super::*;

fn read(name: &str) -> Document {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review").join(format!("{name}.docx"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    read_editable(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn paras(blocks: &[Block]) -> Vec<&Paragraph> {
    blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).collect()
}

fn body(doc: &Document) -> Vec<&Paragraph> {
    doc.sections.iter().flat_map(|s| paras(&s.blocks)).collect()
}

fn tables(doc: &Document) -> Vec<&Table> {
    doc.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).collect()
}

fn who(doc: &Document, m: &RevisionMark) -> String {
    doc.people.get(m.author).map(|p| p.name.to_string()).unwrap_or_default()
}

/// A mark as (kind, author name).
fn mark(doc: &Document, m: Option<&std::rc::Rc<RevisionMark>>) -> Option<(MarkKind, String)> {
    m.map(|m| (m.kind, who(doc, m)))
}

/// Each text run of a paragraph: (text, insertion, deletion, formatting change's author).
type Tracked = (String, Option<(MarkKind, String)>, Option<(MarkKind, String)>, Option<String>);

fn tracked(doc: &Document, p: &Paragraph) -> Vec<Tracked> {
    p.inlines
        .iter()
        .filter_map(|i| if let Inline::Text(t, r) = i { Some((t, r)) } else { None })
        .map(|(t, r)| (t.clone(), mark(doc, r.rev.ins.as_ref()), mark(doc, r.rev.del.as_ref()), r.rev.fmt.as_ref().map(|f| who(doc, &f.mark))))
        .collect()
}

fn ranges(p: &Paragraph) -> Vec<(RangeKind, i64)> {
    p.inlines
        .iter()
        .filter_map(|i| match i {
            Inline::Range(m) => Some((m.kind, m.id)),
            _ => None,
        })
        .collect()
}

/// The review invariants hold in the document an editor opens.
fn integrity(name: &str) {
    let e = crate::editing::tests::open(&format!("review/{name}.docx"));
    let v = crate::editing::integrity::check(e.document(), e.index(), &e.selection());
    assert!(v.is_empty(), "{name}: {v:?}");
}

const A: &str = "Author A";
const B: &str = "Author B";
const C: &str = "Author C";

#[test]
fn runs_carry_insertions_deletions_and_format_changes() {
    let doc = read("FX-REVIEW-RUNS");
    let ps = body(&doc);
    assert_eq!(ps.len(), 6);
    let (ins, del) = (Some((MarkKind::Insert, A.to_string())), Some((MarkKind::Delete, B.to_string())));
    for p in &ps {
        let t = tracked(&doc, p);
        let shape: Vec<_> = t.iter().map(|(_, i, d, f)| (i.clone(), d.clone(), f.clone())).collect();
        assert_eq!(
            shape,
            vec![
                (None, None, None),
                (ins.clone(), None, None),
                (None, None, None),
                (None, del.clone(), None),
                (ins.clone(), del.clone(), None),
                (None, None, Some(C.to_string())),
                (None, None, None),
            ],
            "{t:?}"
        );
        // Deleted text is read from `w:delText`.
        assert!(!t[3].0.is_empty() && !t[4].0.is_empty());
    }
    let p = ps[0];
    let runs: Vec<&Run> = p.inlines.iter().filter_map(Inline::run).collect();
    // One wrapper, one shared mark; every mark dated.
    let fmt = runs[5].rev.fmt.as_ref().expect("a format change");
    assert!(runs[5].bold && !fmt.previous.bold);
    assert!(fmt.previous_raw.0.as_deref().is_some_and(|x| x.starts_with("<w:rPr")));
    assert!(!runs[5].direct.0.as_deref().unwrap_or("").contains("rPrChange"), "direct formatting keeps no revision");
    let marks = [&runs[1].rev.ins, &runs[3].rev.del, &runs[4].rev.ins, &runs[4].rev.del];
    for m in marks.iter().copied().flatten() {
        assert!(m.date.as_ref().and_then(|d| d.utc).is_some(), "{m:?}");
    }
    let mut ids: Vec<i64> = marks.iter().copied().flatten().map(|m| m.id.0).chain([fmt.mark.id.0]).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 5, "distinct revision ids");
    assert_eq!(doc.people.persons.iter().map(|p| &*p.name).collect::<Vec<_>>(), [A, B, C]);
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-RUNS");
}

#[test]
fn paragraph_marks_and_property_changes() {
    let doc = read("FX-REVIEW-MARKS");
    let ps = body(&doc);
    // Deleted and inserted marks don't join paragraphs.
    assert_eq!(ps.len(), 9);
    assert_eq!(mark(&doc, ps[1].mark.rev.del.as_ref()), Some((MarkKind::Delete, B.into())));
    assert_eq!(mark(&doc, ps[4].mark.rev.ins.as_ref()), Some((MarkKind::Insert, A.into())));
    assert!(ps.iter().enumerate().all(|(i, p)| [1, 4].contains(&i) || p.mark.rev.is_empty()));
    assert_eq!(ps[1].style.jc, props::Jc::Center);
    let right = ps[6].style.change.as_ref().expect("a paragraph property change");
    assert_eq!((ps[6].style.jc, right.previous.jc), (props::Jc::Right, props::Jc::Left));
    assert_eq!((ps[6].style.ind_left, right.previous.ind_left), (72.0, 0.0));
    assert_eq!(who(&doc, &right.mark), C);
    let after = ps[7].style.change.as_ref().expect("a paragraph property change");
    assert_eq!((ps[7].style.space_after, after.previous.space_after), (24.0, 0.0));
    assert!(after.previous_raw.0.as_deref().is_some_and(|x| x.contains("w:after=\"0\"")));
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-MARKS");
}

#[test]
fn table_rows_and_grid_changes() {
    let doc = read("FX-REVIEW-TABLES");
    let t = tables(&doc)[0];
    assert_eq!(t.change.as_ref().map(|c| who(&doc, &c.mark)), Some(C.into()));
    assert!(t.change.as_ref().and_then(|c| c.previous_raw.0.as_deref()).is_some_and(|x| x.starts_with("<w:tblPr")));
    let grid = t.grid_change.as_ref().expect("a grid change");
    assert_eq!(grid.previous, t.grid.iter().rev().copied().collect::<Vec<_>>());
    assert_eq!(t.rows.len(), 5);
    let row = |i: usize| (mark(&doc, t.rows[i].rev.ins.as_ref()), mark(&doc, t.rows[i].rev.del.as_ref()));
    assert_eq!(row(0), (None, None));
    assert_eq!(row(1), (Some((MarkKind::Insert, A.into())), None));
    assert_eq!(row(3), (None, Some((MarkKind::Delete, B.into()))));
    let change = t.rows[2].rev.change.as_ref().expect("a row property change");
    assert!(change.previous_raw.0.as_deref().is_some_and(|x| x.contains("w:val=\"360\"")));
    assert_eq!(t.rows[2].height.as_ref().map(|h| h.0), Some(36.0));
    let cell = t.rows[4].cells[0].rev.change.as_ref().expect("a cell property change");
    assert!(cell.previous_raw.0.as_deref().is_some_and(|x| x.starts_with("<w:tcPr")));
    assert_eq!(t.rows[4].cells[0].shading, Some(0xD9E2F3));
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-TABLES");
}

#[test]
fn cell_marks_and_row_exceptions() {
    let doc = read("FX-REVIEW-CELLS");
    let t = tables(&doc)[0];
    let cell = |r: usize, c: usize| t.rows[r].cells[c].rev.structural.as_ref().map(|(m, k)| (m.kind, who(&doc, m), k.clone()));
    assert_eq!(cell(0, 1), Some((MarkKind::Insert, A.into(), CellMarkKind::Insert)));
    assert_eq!(cell(1, 1), Some((MarkKind::Delete, B.into(), CellMarkKind::Delete)));
    let merge = CellMarkKind::Merge { v_merge: Some(VMerge::Continue), v_merge_orig: Some(VMerge::Restart) };
    assert_eq!(cell(3, 0), Some((MarkKind::Format, A.into(), merge)));
    assert_eq!(cell(0, 0), None);
    assert_eq!(t.rows[3].rev.ex_change.as_ref().map(|c| who(&doc, &c.mark)), Some(C.into()));
    assert!(t.rows[..3].iter().all(|r| r.rev.ex_change.is_none()));
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-CELLS");
}

#[test]
fn section_changes_keep_the_previous_page() {
    let doc = read("FX-REVIEW-SECTIONS");
    let s = doc.sections.last().expect("a section");
    let c = s.change.as_ref().expect("a section change");
    assert_eq!((s.page_w, s.page_h, s.top), (792.0, 612.0, 54.0));
    assert_eq!((c.previous.page_w, c.previous.page_h, c.previous.top, c.previous.left), (612.0, 792.0, 72.0, 72.0));
    assert_eq!(who(&doc, &c.mark), C);
    assert!(c.previous_raw.0.as_deref().is_some_and(|x| x.starts_with("<w:sectPr")));
    // The twins have none.
    assert!(read("FX-REVIEW-SECTIONS-ACCEPTED").sections.iter().all(|s| s.change.is_none()));
    integrity("FX-REVIEW-SECTIONS");
}

#[test]
fn moves_pair_by_name() {
    let doc = read("FX-REVIEW-MOVES");
    let ps = body(&doc);
    use RangeKind::*;
    // Range markers between paragraphs start the next paragraph.
    assert_eq!(ranges(ps[1]), [(MoveFromStart, 901)]);
    assert_eq!(ranges(ps[2]), [(MoveFromEnd, 901)]);
    assert_eq!(ranges(ps[3]), [(MoveToStart, 902)]);
    assert_eq!(ranges(ps[4]), [(MoveToEnd, 902), (MoveFromStart, 903), (MoveFromEnd, 903), (MoveToStart, 904), (MoveToEnd, 904)]);
    let name = |p: &Paragraph, k: RangeKind| {
        p.inlines.iter().find_map(|i| match i {
            Inline::Range(m) if m.kind == k => m.name.as_deref().map(str::to_string),
            _ => None,
        })
    };
    assert_eq!(name(ps[1], MoveFromStart).as_deref(), Some("move123"));
    assert_eq!(name(ps[4], MoveToStart).as_deref(), Some("move456"));
    let moved = |p: &Paragraph| {
        p.inlines
            .iter()
            .filter_map(Inline::run)
            .flat_map(|r| [&r.rev.ins, &r.rev.del])
            .flatten()
            .map(|m| (m.kind, m.move_name.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(moved(ps[1]), [(MarkKind::MoveFrom, Some("move123".into()))]);
    assert_eq!(moved(ps[3]), [(MarkKind::MoveTo, Some("move123".into()))]);
    assert_eq!(moved(ps[4]), [(MarkKind::MoveFrom, Some("move456".into())), (MarkKind::MoveTo, Some("move456".into()))]);
    // The moved paragraph's mark moved with it, in the move whose range starts inside the paragraph.
    let mark = |m: &Option<std::rc::Rc<RevisionMark>>| m.as_ref().map(|m| (m.kind, m.move_name.clone()));
    assert_eq!(mark(&ps[1].mark.rev.del), Some((MarkKind::MoveFrom, Some("move123".into()))));
    assert_eq!(mark(&ps[3].mark.rev.ins), Some((MarkKind::MoveTo, Some("move123".into()))));
    // A numbering change alone.
    let n = ps[5].style.change.as_ref().expect("a numbering change");
    assert!(n.numbering_change.0.as_deref().is_some_and(|x| x.contains("w:original=\"%1.\"")));
    assert!(n.previous_raw.0.is_none());
    assert_eq!(who(&doc, &n.mark), C);
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-MOVES");
}

#[test]
fn comments_threads_bodies_and_people() {
    let doc = read("FX-REVIEW-COMMENTS");
    let cs = &doc.comments.by_id;
    assert_eq!(cs.keys().map(|c| c.0).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5]);
    let c0 = &cs[&CommentId(0)];
    assert_eq!((who_comment(&doc, c0), c0.initials.as_deref()), (A.to_string(), Some("AA")));
    assert_eq!((c0.para_id, c0.durable_id, c0.parent, c0.done), (0x10, Some(0x1A00_0001), None, false));
    assert!(c0.date.is_some() && c0.date_utc.as_ref().and_then(|d| d.utc).is_some());
    assert!(c0.extensible.0.as_deref().is_some_and(|x| x.contains("dateUtc")));
    // A rich body: two paragraphs, bold and italic runs.
    let text = paras(&c0.body);
    assert_eq!(text.len(), 2);
    let runs: Vec<(String, bool, bool)> =
        text[0].inlines.iter().filter_map(|i| if let Inline::Text(t, r) = i { Some((t.clone(), r.bold, r.italic)) } else { None }).collect();
    assert!(runs.contains(&("bold".into(), true, false)) && runs.contains(&("italic".into(), false, true)), "{runs:?}");
    assert_eq!(cs[&CommentId(1)].parent, Some(CommentId(0)));
    assert!(cs[&CommentId(2)].done);
    assert_eq!(doc.comments.threads()[0], (CommentId(0), vec![CommentId(1)]));
    // People: identities from the people part, initials from the comments.
    let person = |n: &str| doc.people.persons.iter().find(|p| &*p.name == n).expect("a person");
    assert_eq!(person(A).identity.as_ref().map(|i| (&*i.provider, &*i.user_id)), Some(("AD", "author.a@example.com")));
    assert_eq!(person(B).initials.as_deref(), Some("AB"));
    assert!(person(C).identity.is_none());
    // Nested and overlapping ranges, an empty range, a range at a point, and one in a cell.
    let ps = body(&doc);
    use RangeKind::*;
    assert_eq!(ranges(ps[0]), [(CommentStart, 0), (CommentStart, 3), (CommentEnd, 3), (CommentEnd, 0), (CommentStart, 1), (CommentEnd, 1)]);
    let refs: Vec<i64> = ps[0].inlines.iter().filter_map(|i| if let Inline::CommentRef { id, .. } = i { Some(id.0) } else { None }).collect();
    assert_eq!(refs, [3, 0, 1]);
    assert_eq!(ranges(ps[2]), [(CommentStart, 4), (CommentEnd, 4)]);
    let cell = &tables(&doc)[0].rows[0].cells[0];
    assert_eq!(ranges(paras(&cell.blocks)[0]), [(CommentStart, 5), (CommentEnd, 5)]);
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    // New ids go past those read.
    let mut ids = doc.review_ids.clone();
    assert!(ids.next_para_id() > 0x1A00_0006 && ids.next_revision_id() > 5);
    integrity("FX-REVIEW-COMMENTS");
}

fn who_comment(doc: &Document, c: &review::Comment) -> String {
    doc.people.get(c.author).map(|p| p.name.to_string()).unwrap_or_default()
}

#[test]
fn stories_carry_revisions_and_comments() {
    let doc = read("FX-REVIEW-STORIES");
    let kinds = |blocks: &[Block]| {
        paras(blocks)
            .iter()
            .flat_map(|p| p.inlines.iter().filter_map(Inline::run))
            .flat_map(|r| [&r.rev.ins, &r.rev.del])
            .flatten()
            .map(|m| m.kind)
            .collect::<Vec<_>>()
    };
    let story = |part: &str| doc.stories.iter().find(|(k, _)| k.contains(part)).map(|(_, b)| b.as_slice()).expect("a story");
    assert_eq!(kinds(story("header")), [MarkKind::Insert]);
    assert_eq!(kinds(story("footer")), [MarkKind::Delete]);
    let note = doc.footnotes.values().find(|b| !kinds(b).is_empty()).expect("a footnote with a revision");
    assert_eq!(kinds(note), [MarkKind::Insert]);
    assert!(paras(note).iter().any(|p| p.inlines.iter().any(|i| matches!(i, Inline::CommentRef { id: CommentId(7), .. }))));
    assert!(doc.endnotes.values().any(|b| kinds(b) == [MarkKind::Delete]));
    let text_box = body(&doc)
        .iter()
        .flat_map(|p| &p.inlines)
        .find_map(|i| if let Inline::Float(f, _) = i { f.text_box.as_ref() } else { None })
        .expect("a text box");
    assert_eq!(kinds(&text_box.blocks), [MarkKind::Insert]);
    assert_eq!(doc.comments.by_id.len(), 3);
    assert!(doc.diagnostics.warnings.is_empty(), "{:?}", doc.diagnostics);
    integrity("FX-REVIEW-STORIES");
}

#[test]
fn protection_and_tracking_settings() {
    let doc = read("FX-REVIEW-PROTECTED");
    assert!(doc.settings.track_revisions);
    let p = doc.settings.protection.as_ref().expect("protection");
    assert_eq!((p.edit, p.enforced), (ProtectionKind::TrackedChanges, true));
    assert!(p.forces_tracking() && p.raw.0.as_deref().is_some_and(|x| x.starts_with("<w:documentProtection")));
    let plain = read("FX-REVIEW-RUNS");
    assert!(!plain.settings.track_revisions && plain.settings.protection.is_none());
}

#[test]
fn malformed_review_data_is_reported_once_each() {
    let doc = read("FX-REVIEW-MALFORMED");
    let d = &doc.diagnostics;
    for code in [
        WarningCode::RevisionIdMissing,
        WarningCode::RangeUnpaired,
        WarningCode::AuthorMissing,
        WarningCode::CommentUnanchored,
        WarningCode::CommentReferenceOrphan,
    ] {
        assert_eq!(d.count(code), 1, "{code:?}: {d:?}");
    }
    assert_eq!(d.warnings.len(), 5, "{d:?}");
    let ps = body(&doc);
    // The insertion with no id got a new one past every id read.
    let ins = ps[0].inlines.iter().filter_map(Inline::run).find_map(|r| r.rev.ins.clone()).expect("the insertion");
    assert!(ins.id.0 > 77);
    // The deletion with no author is an empty name's; the end with no start and the orphan reference are left out.
    let del = ps[2].inlines.iter().filter_map(Inline::run).find_map(|r| r.rev.del.clone()).expect("the deletion");
    assert_eq!(who(&doc, &del), "");
    assert!(ranges(ps[1]).is_empty());
    assert!(!ps[4].inlines.iter().any(|i| matches!(i, Inline::CommentRef { .. })));
    integrity("FX-REVIEW-MALFORMED");
}

#[test]
fn every_review_fixture_reads() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review");
    for e in std::fs::read_dir(&dir).expect("the review fixtures").flatten() {
        let bytes = std::fs::read(e.path()).expect("readable");
        let doc = read_editable(&bytes).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
        let name = e.file_name().to_string_lossy().to_string();
        if !name.contains("MALFORMED") {
            assert!(doc.diagnostics.warnings.is_empty(), "{name}: {:?}", doc.diagnostics);
        }
        // The accepted and rejected twins have no revisions.
        if name.contains("-ACCEPTED") || name.contains("-REJECTED") {
            let none = body(&doc)
                .iter()
                .all(|p| p.mark.rev.is_empty() && p.style.change.is_none() && p.inlines.iter().filter_map(Inline::run).all(|r| r.rev.is_empty()));
            assert!(none, "{name}");
        }
    }
}
