//! Baseline differential: the XML resolution path, given the property XML each paragraph, mark and run holds,
//! gives the format the document holds, after each representative edit, its undo and its redo; the edit's authored
//! result is what a hand-written patch of the property XML as read gives

use std::rc::Rc;

use super::super::Editor;
use super::super::commands::{Command, Outcome};
use super::{blank_body, fonts, texts_ids};
use crate::api::command::{Command as C, DeleteOptions, FontChoice, Indents, InsertFile, MoveSelection, MoveUnit, PasteOptions, VerticalDirection};
use crate::api::drawing::{ColourTransform, ColourValue, ThemeColour, TransformKind};
use crate::api::styles::{StyleChange, StyleModification};
use crate::api::value::{Alignment, AutoOr, Bytes, Direction, HighlightColor, Points, StyleId, UnderlineStyle};
use crate::docx::props::{ParaProps, RunProps, ThemeColours};
use crate::docx::resolve::Resolver;
use crate::docx::styles::Region;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, Document, Inline, Paragraph, Run, RunStyle};
use crate::docx::{char_props, page_borders, para_props, para_xml};
use crate::format::resolve::{self, RunEnv};
use crate::format::{CharProps, Color};
use crate::layout::{ParaId, Unit};

fn model(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

fn cx(d: &Document) -> Cx {
    Cx::of_xml(&d.source.as_ref().expect("read for editing").xml)
}

// Re-resolution from the property XML each node holds

/// A run's `w:rPr` as the document writes it: as read while its authored properties are those, else written from
/// them (a typed command's result, whose properties the edit's own check holds against the hand-written patch).
fn written(res: &Resolver, cx: &Cx, r: &RunStyle) -> Option<String> {
    r.direct.xml(&res.styles.colours, cx).map(std::borrow::Cow::into_owned)
}

/// Every body paragraph, mark and text run resolved again from its own property XML gives the format it holds.
/// Paragraphs joined through vanished marks are skipped, as the as-read harness skips them (`docx::resolve`).
fn check(d: &Document, what: &str) -> usize {
    let res = d.resolver.as_deref().expect("a resolver");
    let cx = cx(d);
    let mut checked = 0;
    for s in &d.sections {
        check_blocks(res, &cx, &s.blocks, None, what, &mut checked);
    }
    checked
}

fn check_blocks(res: &Resolver, cx: &Cx, blocks: &[Block], region: Option<&Region>, what: &str, checked: &mut usize) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                if p.src.as_ref().is_some_and(|s| s.ranges.len() > 1) {
                    continue;
                }
                let mut r = res.para_from_xml(p.style.direct.xml(&res.styles.colours, cx).as_deref(), region, cx);
                r.style.change = p.style.change.clone();
                r.style.in_frame = p.style.in_frame;
                r.style.frame = p.style.frame.clone();
                assert!(p.style.layout_eq(&r.style), "{what}: paragraph {:?}: {:?} against {:?}", p.id, p.style, r.style);
                let with_rev = |again: Run, read: &Run| RunStyle { rev: read.rev.clone(), ..(*again).clone() };
                let mark = with_rev(res.run_from_xml(&r.base, written(res, cx, &p.mark).as_deref(), cx), &p.mark);
                assert!(p.mark.layout_eq(&mark), "{what}: the mark of {:?}: {:?} against {mark:?}", p.id, p.mark);
                for i in &p.inlines {
                    if let Inline::Text(t, run) = i {
                        let again = res.rerun_xml(&r.base, run, written(res, cx, run).as_deref(), cx);
                        assert!(run.layout_eq(&again), "{what}: run {t:?} of {:?}: {run:?} against {again:?}", p.id);
                        *checked += 1;
                    }
                }
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, c) in row.cells.iter().enumerate() {
                        check_blocks(res, cx, &c.blocks, Some(&res.region_of(t, ri, ci)), what, checked);
                    }
                }
            }
        }
    }
}

/// Each body paragraph's authored XML: its own, its mark's and its text runs', in order.
type Authored = Vec<(u64, Option<Rc<str>>, Option<Rc<str>>, Vec<Option<Rc<str>>>)>;

fn authored(d: &Document) -> Authored {
    fn walk(res: &Resolver, cx: &Cx, blocks: &[Block], out: &mut Authored) {
        let rpr = |r: &RunStyle| written(res, cx, r).map(|x| Rc::from(x.as_str()));
        for b in blocks {
            match b {
                Block::Para(p) => out.push((
                    p.id,
                    p.style.direct.xml(&res.styles.colours, cx).map(|x| Rc::from(&*x)),
                    rpr(&p.mark),
                    p.inlines.iter().filter_map(|i| if let Inline::Text(_, r) = i { Some(rpr(r)) } else { None }).collect(),
                )),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(res, cx, &c.blocks, out)),
            }
        }
    }
    let (res, cx) = (d.resolver.as_deref().expect("a resolver"), cx(d));
    let mut out = Vec::new();
    d.sections.iter().for_each(|s| walk(res, &cx, &s.blocks, &mut out));
    out
}

// The representative edits

/// Where an edit applies: the paragraph whose text starts with the label.
#[derive(Clone, Copy)]
enum At {
    /// Its text.
    Text(&'static str),
    /// Its text and its mark.
    Whole(&'static str),
    /// A caret at its start.
    Start(&'static str),
    /// A caret after this many units.
    Inside(&'static str, u32),
}

/// What an edit should do to the property XML as read, of each selected run or of the paragraph.
type Patch = fn(&mut Props, &Cx);

struct Edit {
    name: &'static str,
    fixture: &'static str,
    at: At,
    commands: Vec<Command>,
    runs: Option<Patch>,
    para: Option<Patch>,
    /// Paragraphs other than the target keep their authored XML.
    local: bool,
}

fn edit(name: &'static str, fixture: &'static str, at: At, commands: Vec<Command>) -> Edit {
    Edit { name, fixture, at, commands, runs: None, para: None, local: true }
}

impl Edit {
    fn runs(self, p: Patch) -> Edit {
        Edit { runs: Some(p), ..self }
    }
    fn para(self, p: Patch) -> Edit {
        Edit { para: Some(p), ..self }
    }
    fn reaching(self) -> Edit {
        Edit { local: false, ..self }
    }
}

fn el(cx: &Cx, local: &str, attrs: &[(&str, &str)]) -> String {
    let a: String = attrs.iter().map(|(k, v)| format!(" {}{k}=\"{v}\"", cx.w)).collect();
    format!("<{}{local}{a}/>", cx.w)
}

fn on(p: &mut Props, cx: &Cx, local: &str) {
    p.set(local, el(cx, local, &[]));
}

fn off(p: &mut Props, cx: &Cx, local: &str) {
    p.set(local, el(cx, local, &[("val", "0")]));
}

fn style(id: &str, formatting: Command) -> Command {
    C::ModifyStyle(StyleModification { id: StyleId(id.into()), change: StyleChange { formatting: vec![formatting], ..Default::default() } })
}

fn theme(slot: ThemeColour, transforms: Vec<ColourTransform>) -> ColourValue {
    ColourValue { theme: Some(slot), transforms, ..Default::default() }
}

fn rgb(r: u8, g: u8, b: u8) -> ColourValue {
    ColourValue { rgb: Some(crate::api::value::Color(u32::from_be_bytes([0, r, g, b]))), ..Default::default() }
}

const OFF: &str = "explicit-off.docx";
const TOGGLES: &str = "style-toggles.docx";
const THEME: &str = "theme-references.docx";
const UNKNOWN: &str = "unknown-attributes.docx";

fn edits() -> Vec<Edit> {
    vec![
        // Toggles
        edit("bold on", OFF, At::Text("Underlined character style"), vec![C::SetBold(true)]).runs(|p, cx| {
            on(p, cx, "b");
            on(p, cx, "bCs");
        }),
        edit("bold off over a bold style", OFF, At::Text("Bold paragraph style, nothing"), vec![C::SetBold(false)]).runs(|p, cx| {
            off(p, cx, "b");
            off(p, cx, "bCs");
        }),
        edit("italic in a bold character style", TOGGLES, At::Text("Chain level one"), vec![C::SetItalic(true)]).runs(|p, cx| {
            on(p, cx, "i");
            on(p, cx, "iCs");
        }),
        edit("clear direct formatting", TOGGLES, At::Text("Direct bold over a bold paragraph"), vec![C::ClearFormatting]).runs(|p, _| p.kids.clear()),
        // Values
        edit("size", OFF, At::Text("Coloured character style"), vec![C::SetFontSize(Points(18.0))]).runs(|p, cx| {
            p.set("sz", el(cx, "sz", &[("val", "36")]));
            p.set("szCs", el(cx, "szCs", &[("val", "36")]));
        }),
        edit("character spacing", OFF, At::Text("Coloured character style"), vec![C::SetCharacterSpacing(Points(1.5))])
            .runs(|p, cx| p.set("spacing", el(cx, "spacing", &[("val", "30")]))),
        edit(
            "underline style and colour",
            OFF,
            At::Text("Coloured character style"),
            vec![C::SetUnderline(UnderlineStyle::Double), C::SetUnderlineColor(AutoOr::Value(rgb(0xC0, 0, 0)))],
        )
        .runs(|p, cx| p.set("u", el(cx, "u", &[("val", "double"), ("color", "C00000")]))),
        edit("highlight", OFF, At::Text("Highlight unset"), vec![C::SetHighlight(Some(HighlightColor::Yellow))])
            .runs(|p, cx| p.set("highlight", el(cx, "highlight", &[("val", "yellow")]))),
        edit("colour to automatic", OFF, At::Text("Coloured character style, nothing"), vec![C::SetFontColor(AutoOr::Auto)])
            .runs(|p, cx| p.set("color", el(cx, "color", &[("val", "auto")]))),
        // Fonts
        edit("a named family", OFF, At::Text("Coloured character style"), vec![C::SetFont(FontChoice::Name("Cambria".into()))]).runs(|p, cx| {
            p.set("rFonts", el(cx, "rFonts", &[("ascii", "Cambria"), ("hAnsi", "Cambria"), ("eastAsia", "Cambria"), ("cs", "Cambria")]))
        }),
        edit(
            "a theme family replaced by a named one",
            THEME,
            At::Text("Direct minor fonts"),
            vec![C::SetFont(FontChoice::Name("Times New Roman".into()))],
        )
        .runs(|p, cx| {
            let f = "Times New Roman";
            p.set("rFonts", el(cx, "rFonts", &[("ascii", f), ("hAnsi", f), ("eastAsia", f), ("cs", f)]));
        }),
        edit(
            "a named family over an empty East Asian theme slot",
            THEME,
            At::Text("Empty major East Asian slot over a named family"),
            vec![C::SetFont(FontChoice::Name("MS Mincho".into()))],
        )
        .runs(|p, cx| {
            let f = "MS Mincho";
            p.set("rFonts", el(cx, "rFonts", &[("ascii", f), ("hAnsi", f), ("eastAsia", f), ("cs", f)]));
        }),
        // Theme references
        edit(
            "a theme colour replaced by RGB",
            THEME,
            At::Text("Text in accent 1 darker"),
            vec![C::SetFontColor(AutoOr::Value(rgb(0x2F, 0x54, 0x96)))],
        )
        .runs(|p, cx| p.set("color", el(cx, "color", &[("val", "2F5496")]))),
        edit(
            "RGB replaced by a theme colour with a tint",
            OFF,
            At::Text("Coloured character style, nothing"),
            vec![C::SetFontColor(AutoOr::Value(theme(ThemeColour::Accent2, vec![ColourTransform { kind: TransformKind::Tint, value: 0.4 }])))],
        ),
        // Styles
        edit("apply a character style", OFF, At::Text("Underlined character style"), vec![C::ApplyCharacterStyle(Some(StyleId("BoldChar".into())))]),
        edit("remove a character style", OFF, At::Text("Bold character style, nothing"), vec![C::ApplyCharacterStyle(None)]),
        edit("apply a paragraph style", OFF, At::Whole("Underlined character style"), vec![C::SetParagraphStyle(StyleId("BoldPara".into()))])
            .para(|p, cx| p.set("pStyle", el(cx, "pStyle", &[("val", "BoldPara")]))),
        edit("a base style's bold", TOGGLES, At::Start("Bold paragraph style alone"), vec![style("BoldPara", C::SetBold(false))]),
        // Paragraphs and lists
        edit("alignment", OFF, At::Whole("Coloured character style"), vec![C::SetAlignment(Alignment::Center)])
            .para(|p, cx| p.set("jc", el(cx, "jc", &[("val", "center")]))),
        edit(
            "indent zero over a style",
            OFF,
            At::Whole("Indented paragraph style, nothing"),
            vec![C::SetIndents(Indents { left: Some(Points(0.0)), first_line: Some(Points(0.0)), ..Default::default() })],
        )
        .para(|p, cx| p.set("ind", el(cx, "ind", &[("left", "0"), ("firstLine", "0")]))),
        edit("remove a list inherited from a style", OFF, At::Whole("List paragraph style, nothing"), vec![C::SetList(None)]).reaching(),
        edit("clear a tab stop", OFF, At::Whole("Style tab, nothing"), vec![C::DeleteTabStop(Points(252.0))]),
        // Tables
        edit("cell shading", THEME, At::Start("Unbanded row"), vec![C::SetCellShading(Some(rgb(0xFF, 0xF2, 0xCC)))]),
        edit("a row moved between bands", THEME, At::Start("Band in accent 1"), vec![C::MoveRows(VerticalDirection::Down)]).reaching(),
        edit("table style", THEME, At::Start("Cell borders in accent 2"), vec![C::SetTableStyle(StyleId("ThemeGrid".into()))]).reaching(),
        // Structure
        edit("split", OFF, At::Inside("Coloured character style, nothing", 9), vec![C::InsertParagraph]).reaching(),
        edit("join", OFF, At::Start("Colour auto over"), vec![C::Delete(DeleteOptions { direction: Direction::Backward, unit: None })]).reaching(),
        edit(
            "paste within the part",
            TOGGLES,
            At::Text("Chain level three in a bold"),
            vec![
                Command::Copy,
                C::MoveSelection(MoveSelection { by: MoveUnit::Document, direction: Direction::Forward, extend: None }),
                C::Paste(PasteOptions::default()),
            ],
        )
        .reaching(),
        // Review
        edit(
            "a tracked format change, accepted",
            OFF,
            At::Text("Underlined character style"),
            vec![C::SetTrackChanges(true), C::SetBold(true), C::AcceptAllRevisions(Default::default())],
        )
        .reaching(),
        edit(
            "a tracked format change, rejected",
            OFF,
            At::Text("Underlined character style"),
            vec![C::SetTrackChanges(true), C::SetBold(true), C::RejectAllRevisions(Default::default())],
        )
        .reaching(),
        // Unmodelled markup beside an edited property
        edit("bold beside unread children and attributes", UNKNOWN, At::Text("Bold run whose unread"), vec![C::SetItalic(true)]).runs(|p, cx| {
            on(p, cx, "i");
            on(p, cx, "iCs");
        }),
        edit(
            "alignment beside unread paragraph children",
            UNKNOWN,
            At::Whole("Paragraph with adjustRightInd"),
            vec![C::SetAlignment(Alignment::Right)],
        )
        .para(|p, cx| p.set("jc", el(cx, "jc", &[("val", "right")]))),
    ]
}

fn para_by_label<'a>(d: &'a Document, ids: &[ParaId], label: &str) -> (usize, &'a Paragraph) {
    let text = |p: &Paragraph| p.inlines.iter().filter_map(|i| if let Inline::Text(t, _) = i { Some(&**t) } else { None }).collect::<String>();
    ids.iter()
        .enumerate()
        .find_map(|(n, id)| {
            let p = find(d, id.0)?;
            text(p).starts_with(label).then_some((n, p))
        })
        .unwrap_or_else(|| panic!("no paragraph starts {label:?}"))
}

fn find(d: &Document, id: u64) -> Option<&Paragraph> {
    fn walk(blocks: &[Block], id: u64) -> Option<&Paragraph> {
        blocks.iter().find_map(|b| match b {
            Block::Para(p) => (p.id == id).then_some(p),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| walk(&c.blocks, id)),
        })
    }
    d.sections.iter().find_map(|s| walk(&s.blocks, id))
}

fn select(e: &mut Editor, at: At) -> u64 {
    let ids = texts_ids(e);
    let label = match at {
        At::Text(l) | At::Whole(l) | At::Start(l) | At::Inside(l, _) => l,
    };
    let (n, p) = para_by_label(e.document(), &ids, label);
    let (id, target) = (ParaId(p.id), p.id);
    match at {
        At::Text(_) => e.select_range(id, Unit(0), Unit(u32::MAX), fonts()),
        At::Whole(_) => e.select_between((id, Unit(0)), (ids[n + 1], Unit(0)), fonts()),
        At::Start(_) => e.select_range(id, Unit(0), Unit(0), fonts()),
        At::Inside(_, u) => e.select_range(id, Unit(u), Unit(u), fonts()),
    };
    target
}

/// The run properties at each character of the paragraph's text, parsed from their XML (after `patch`).
fn run_props(res: &Resolver, cx: &Cx, p: &Paragraph, patch: Option<Patch>) -> Vec<RunProps> {
    let parse = |raw: Option<&str>| raw.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el), &res.styles.colours))).unwrap_or_default();
    p.inlines
        .iter()
        .filter_map(|i| if let Inline::Text(t, r) = i { Some((t, r)) } else { None })
        .flat_map(|(t, r)| {
            let raw = match patch {
                Some(f) => {
                    let mut props = Props::parse(r.direct.as_read().map(|x| &**x), "rPr", cx);
                    f(&mut props, cx);
                    props.xml()
                }
                None => written(res, cx, r),
            };
            std::iter::repeat_n(parse(raw.as_deref()), t.chars().count())
        })
        .collect()
}

fn para_props(res: &Resolver, cx: &Cx, p: &Paragraph, patch: Option<Patch>) -> ParaProps {
    let raw = match patch {
        Some(f) => {
            let mut props = Props::parse(p.style.direct.as_read().map(|x| &**x), "pPr", cx);
            f(&mut props, cx);
            props.xml()
        }
        None => p.style.direct.xml(&res.styles.colours, cx).map(String::from),
    };
    raw.and_then(|x| cx.parse(&x, |_, el| ParaProps::parse(Some(el), &res.styles.colours))).unwrap_or_default()
}

fn run(edit: &Edit) {
    let what = edit.name;
    let mut e = Editor::open(&model(edit.fixture), fonts()).expect("opens");
    e.complete(fonts());
    assert!(check(e.document(), &format!("{what}: as read")) > 0, "{what}: nothing checked");
    let read = authored(e.document());
    let target = select(&mut e, edit.at);
    let before = find(e.document(), target).expect("the target").clone();
    let mut steps = 0;
    for c in &edit.commands {
        match e.execute(c.clone(), fonts()) {
            Outcome::Changed(_) => steps += 1,
            Outcome::Moved => {}
            out => panic!("{what}: {c:?} gave {out:?}"),
        }
    }
    e.complete(fonts());
    check(e.document(), &format!("{what}: edited"));
    let edited = authored(e.document());
    let d = e.document();
    let (res, cx) = (d.resolver.as_deref().expect("a resolver"), cx(d));
    if edit.runs.is_some() || edit.para.is_some() {
        let after = find(d, target).expect("the target");
        if edit.runs.is_some() {
            assert_eq!(run_props(res, &cx, after, None), run_props(res, &cx, &before, edit.runs), "{what}: the runs' authored properties");
        }
        if edit.para.is_some() {
            assert_eq!(para_props(res, &cx, after, None), para_props(res, &cx, &before, edit.para), "{what}: the paragraph's authored properties");
        }
    }
    if edit.local {
        let others = |a: &Authored| a.iter().filter(|p| p.0 != target).cloned().collect::<Authored>();
        assert_eq!(others(&edited), others(&read), "{what}: other paragraphs keep their authored XML");
    }
    for n in 0..steps {
        assert!(matches!(e.execute(Command::Undo(Default::default()), fonts()), Outcome::Changed(_)), "{what}: undo {n}");
    }
    e.complete(fonts());
    check(e.document(), &format!("{what}: undone"));
    assert_eq!(authored(e.document()), read, "{what}: undo restores the authored XML as read");
    for n in 0..steps {
        assert!(matches!(e.execute(Command::Redo(Default::default()), fonts()), Outcome::Changed(_)), "{what}: redo {n}");
    }
    e.complete(fonts());
    check(e.document(), &format!("{what}: redone"));
    assert_eq!(authored(e.document()), edited, "{what}: redo restores the edited XML");
}

#[test]
fn each_representative_edit_resolves_as_its_xml_does() {
    for edit in edits() {
        run(&edit);
    }
}

#[test]
fn content_pasted_from_another_document_resolves_as_its_xml_does() {
    let mut src = Editor::open(&model("foreign-content-copy-source.docx"), fonts()).expect("opens");
    src.execute(Command::SelectAll, fonts());
    let frag = src.copy().expect("copies");
    let mut dst = Editor::open(&model("foreign-content-copy-destination.docx"), fonts()).expect("opens");
    let first = texts_ids(&dst)[0];
    dst.select_range(first, Unit(0), Unit(0), fonts());
    assert!(matches!(dst.paste_fragment(&frag, fonts()), Outcome::Changed(_)), "pasted");
    check(dst.document(), "pasted from another document");
}

/// Runs pasted from a document that binds the main namespace to another prefix keep their formatting and are written
/// in this document's prefixes.
#[test]
fn runs_pasted_from_another_prefix_are_written_in_this_ones() {
    let mut src = Editor::open(&model("foreign-content-copy-source.docx"), fonts()).expect("opens");
    src.execute(Command::SelectAll, fonts());
    let frag = src.copy().expect("copies");
    let mut dst = Editor::open(&model("foreign-content-copy-destination.docx"), fonts()).expect("opens");
    let first = texts_ids(&dst)[0];
    dst.select_range(first, Unit(0), Unit(0), fonts());
    assert!(matches!(dst.paste_fragment(&frag, fonts()), Outcome::Changed(_)), "pasted");
    let d = dst.document();
    let (res, cx) = (d.resolver.as_deref().expect("a resolver"), cx(d));
    let mut styled = 0;
    for p in d.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) {
        for r in std::iter::once(&p.mark).chain(p.inlines.iter().filter_map(Inline::run)) {
            let Some(x) = written(res, &cx, r) else { continue };
            let read = cx.parse(&x, |_, el| char_props::parse(el).0);
            assert_eq!(read.as_ref(), r.direct.props().map(|p| &**p), "{x} reads as written here");
            styled += usize::from(r.style_id.as_deref() == Some("SourceMark"));
        }
    }
    assert!(styled > 0, "the pasted run keeps its character style");
    let quote = |d: &Document| {
        let paras = d.sections.iter().flat_map(|s| &s.blocks).filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
        paras.filter(|p| p.style.style_id.as_deref() == Some("SourceQuote")).count()
    };
    assert_eq!(quote(d), 1, "the pasted paragraph keeps its style");
    let saved = crate::docx::write::save(d).expect("saves");
    let again = crate::docx::read_editable(&saved).expect("reads back");
    assert_eq!(quote(&again), 1, "the pasted paragraph keeps its style, saved");
    let main = crate::docx::package::read(&saved, &[], "word/document.xml").expect("reads").expect("the main part");
    assert!(main.contains("<m:oMath xmlns:m="), "the equation comes, its namespace declared");
}

#[test]
fn pasted_html_resolves_as_its_xml_does() {
    let mut e = Editor::open(&model(OFF), fonts()).expect("opens");
    let first = texts_ids(&e)[0];
    e.select_range(first, Unit(0), Unit(0), fonts());
    let html = r#"<p style="text-align:center"><b>Bold</b> and <span style="color:#c00000;font-size:16pt">red</span></p><ul><li>item</li></ul>"#;
    assert!(matches!(e.paste_html(html, fonts()), Outcome::Changed(_)), "pasted HTML");
    check(e.document(), "pasted HTML");
}

#[test]
fn an_inserted_file_resolves_as_its_xml_does() {
    let mut e = Editor::open(&model(OFF), fonts()).expect("opens");
    let first = texts_ids(&e)[0];
    e.select_range(first, Unit(0), Unit(0), fonts());
    assert!(matches!(e.execute(C::InsertFile(InsertFile { from: Bytes(model(TOGGLES)) }), fonts()), Outcome::Changed(_)), "inserted");
    check(e.document(), "an inserted file");
}

#[test]
fn an_imported_chunk_resolves_as_its_xml_does() {
    let body = r#"<w:p><w:r><w:t>before</w:t></w:r></w:p><w:altChunk r:id="rIdChunk"/><w:p><w:r><w:t>after</w:t></w:r></w:p>"#;
    let html = b"<html><body><p><b>chunk</b> <i>text</i></p></body></html>";
    let e = Editor::open(&with_html_chunk(body, html), fonts()).expect("opens");
    assert!(check(e.document(), "an imported chunk") >= 4, "the chunk's runs are checked");
}

/// A blank document with `body`, its `w:altChunk r:id="rIdChunk"` relating to `html`.
fn with_html_chunk(body: &str, html: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(blank_body(body))).expect("a zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).expect("reads");
        if name == "word/_rels/document.xml.rels" {
            let rel = r#"<Relationship Id="rIdChunk" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk" Target="chunk.html"/></Relationships>"#;
            bytes = String::from_utf8(bytes).expect("utf-8").replacen("</Relationships>", rel, 1).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("starts");
        out.write_all(&bytes).expect("writes");
    }
    out.start_file("word/chunk.html", zip::write::SimpleFileOptions::default()).expect("starts");
    out.write_all(html).expect("writes");
    out.finish().expect("finishes").into_inner()
}

/// Every fixture's documents, read for editing, with their resolver and the main part's namespaces.
fn fixtures() -> impl Iterator<Item = (std::path::PathBuf, Document)> {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    paths.into_iter().filter_map(|path| {
        let d = crate::docx::read_editable(&std::fs::read(&path).expect("reads")).ok()?;
        (d.resolver.is_some() && d.source.is_some()).then_some((path, d))
    })
}

/// `rpr`'s typed properties and revisions, written again by the typed serializer, parse to the same typed properties
/// and revisions, and to the same properties on the XML path, and the writing holds every element, text and
/// attribute value of `rpr` (whatever its prefixes and order); converting the XML path's properties gives the typed
/// ones but for what that path doesn't read.
fn typed_round_trip(rpr: roxmltree::Node, cx: &Cx, colours: &ThemeColours, what: &str) {
    let typed = char_props::parse(rpr);
    let xml = char_props::serialize(&typed.0, &typed.1, colours, cx);
    let again = cx.parse(&xml, |_, el| char_props::parse(el)).unwrap_or_else(|| panic!("{what}: written as {xml}, parses"));
    assert_eq!(again, typed, "{what}: typed, written as {xml}");
    let lost = cx.parse(&xml, |_, el| char_props::tests::lost(rpr, el)).unwrap_or_default();
    assert!(lost.is_empty(), "{what}: written as {xml}, loses {lost:?}");
    let read = RunProps::parse(Some(rpr), colours);
    assert_eq!(cx.parse(&xml, |_, el| RunProps::parse(Some(el), colours)), Some(read.clone()), "{what}: on the XML path, written as {xml}");
    let mut typed = typed.0;
    (typed.foreign, typed.lang.complex, typed.spec_vanish) = (None, None, None);
    let auto = |c: &mut Option<Color>| *c = c.filter(|c| *c != Color::Auto);
    if let Some(u) = &mut typed.underline {
        auto(&mut u.color);
    }
    if let Some(s) = &mut typed.shading {
        auto(&mut s.color);
        auto(&mut s.fill);
    }
    if let Some(b) = &mut typed.border {
        auto(&mut b.color);
    }
    assert_eq!(char_props::from_run_props(&read), typed, "{what}: converted from the XML path");
}

/// `ppr`'s typed properties, written again by the typed serializer with the children other nodes own, parse to the
/// same typed properties, and to the same properties on the XML path, and the writing holds every element, text and
/// attribute value of `ppr`; converting the XML path's properties gives the typed ones but for what that path doesn't
/// read.
fn typed_ppr_round_trip(ppr: roxmltree::Node, cx: &Cx, colours: &ThemeColours, what: &str) {
    let typed = para_props::parse(ppr);
    let text = ppr.document().input_text();
    let own = |name| crate::docx::xml::child(ppr, name).map(|c| &text[c.range()]);
    let owned = para_props::Owned { mark: own("rPr"), section: own("sectPr"), change: own("pPrChange") };
    let xml = para_props::serialize(&typed, owned, colours, cx);
    let again = cx.parse(&xml, |_, el| para_props::parse(el)).unwrap_or_else(|| panic!("{what}: written as {xml}, parses"));
    assert_eq!(again, typed, "{what}: typed, written as {xml}");
    let lost = cx.parse(&xml, |_, el| char_props::tests::lost(ppr, el)).unwrap_or_default();
    assert!(lost.is_empty(), "{what}: written as {xml}, loses {lost:?}");
    let read = ParaProps::parse(Some(ppr), colours);
    assert_eq!(cx.parse(&xml, |_, el| ParaProps::parse(Some(el), colours)), Some(read.clone()), "{what}: on the XML path, written as {xml}");
    let mut typed = typed;
    typed.foreign = None;
    let auto = |c: &mut Option<Color>| *c = c.filter(|c| *c != Color::Auto);
    if let Some(s) = &mut typed.shading {
        auto(&mut s.color);
        auto(&mut s.fill);
    }
    let b = &mut typed.borders;
    for side in [&mut b.top, &mut b.bottom, &mut b.left, &mut b.right, &mut b.between].into_iter().flatten() {
        auto(&mut side.color);
    }
    assert_eq!(para_props::from_para_props(&read), typed, "{what}: converted from the XML path");
}

/// Every `w:pgBorders` in every fixture's main part (a section's, and a tracked change's earlier one) is the one its
/// section writes while unchanged; its typed borders, written again by the typed serializer, parse to the same typed
/// borders and draw the same borders, and the writing holds every element, text and attribute value of it.
#[test]
fn page_border_xml_round_trips_through_the_typed_writer() {
    let mut n = 0;
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        let colours = &res.styles.colours;
        let src = &d.source.as_ref().expect("a source").xml;
        let doc = roxmltree::Document::parse(src).expect("the main part parses");
        let what = |raw: &str| format!("{}: {raw}", path.display());
        let (mut body, mut earlier) = (Vec::new(), Vec::new());
        for b in doc.descendants().filter(|n| crate::docx::xml::is_w(*n, "pgBorders")) {
            let raw = &src[b.range()];
            let typed = page_borders::parse(b);
            let xml = page_borders::serialize(&typed, colours, &cx);
            let again = cx.parse(&xml, |_, el| page_borders::parse(el)).unwrap_or_else(|| panic!("{}: written as {xml}, parses", what(raw)));
            assert_eq!(again, typed, "{}: typed, written as {xml}", what(raw));
            let lost = cx.parse(&xml, |_, el| char_props::tests::lost(b, el)).unwrap_or_default();
            assert!(lost.is_empty(), "{}: written as {xml}, loses {lost:?}", what(raw));
            let drawn = cx.parse(&xml, |_, el| page_borders::effective(el, colours, true, true));
            assert_eq!(drawn, Some(page_borders::effective(b, colours, true, true)), "{}: drawn, written as {xml}", what(raw));
            let tracked = b.ancestors().any(|a| crate::docx::xml::is_w(a, "sectPrChange"));
            if tracked { &mut earlier } else { &mut body }.push(raw);
            n += 1;
        }
        let held = |p: &crate::docx::SectionPage| p.page_borders_direct.xml(colours, &cx).map(|x| x.into_owned());
        let sections: Vec<String> = d.sections.iter().filter_map(|s| held(&s.page())).collect();
        assert_eq!(sections, body, "{}: each section's as read", path.display());
        let changes: Vec<String> = d.sections.iter().filter_map(|s| s.change.as_ref().and_then(|c| held(&c.previous))).collect();
        assert_eq!(changes, earlier, "{}: each tracked change's earlier one as read", path.display());
    }
    assert!(n >= 4, "only {n} page border elements checked");
}

/// Every paragraph property element in every fixture (the main part's, its tracked changes' earlier ones, a style's,
/// a table style region's, a list level's and the document defaults') round trips through the typed parse and
/// serializer.
#[test]
fn paragraph_property_xml_round_trips_through_the_typed_writer() {
    let mut n = 0;
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        let colours = &res.styles.colours;
        let what = |raw: &str| format!("{}: {raw}", path.display());
        let src = &d.source.as_ref().expect("a source").xml;
        let doc = roxmltree::Document::parse(src).expect("the main part parses");
        for p in doc.descendants().filter(|n| crate::docx::xml::is_w(*n, "pPr")) {
            typed_ppr_round_trip(p, &cx, colours, &what(&src[p.range()]));
            n += 1;
        }
        let scx = res.styles.cx();
        let mut styles: Vec<&str> = res.styles.by_id.values().map(|s| s.raw.as_str()).collect();
        styles.extend(res.styles.defaults_raw.as_deref());
        for raw in styles {
            let k = scx.parse(raw, |_, el| {
                let pprs = el.descendants().filter(|n| crate::docx::xml::is_w(*n, "pPr"));
                pprs.map(|p| typed_ppr_round_trip(p, &scx, colours, &what(raw))).count()
            });
            n += k.unwrap_or_else(|| panic!("{}: parses", what(raw)));
        }
    }
    assert!(n > 7_000, "only {n} paragraph property elements checked");
}

/// What the table property round trip checked: elements of each kind, and cells whose text flow the XML path keeps as
/// written where the typed path takes Word's value for it.
#[derive(Default)]
struct TableCounts {
    tables: usize,
    exceptions: usize,
    rows: usize,
    cells: usize,
    strict_flows: usize,
}

/// `el`, a table's, a row's exceptions', a row's or a cell's property element: its holder writes it as read; its typed
/// properties, written again by the typed serializer with the children other nodes own, parse to the same typed
/// properties and to the same properties on the XML path, and the writing holds every element, text and attribute
/// value of `el`; written over itself with nothing set it is as read; and the XML path's properties built from the
/// typed ones are the XML path's parse of `el`, and survive a conversion to typed properties and back.
fn typed_table_round_trip(el: roxmltree::Node, cx: &Cx, colours: &ThemeColours, what: &str, counts: &mut TableCounts) {
    use crate::docx::props::{TblProps, TcProps, TrProps};
    use crate::docx::table_props::{self as tp, CellDirect, ExceptionsDirect, RowDirect, TableDirect};
    let text = &el.document().input_text()[el.range()];
    let owned = |names: &[&'static str]| -> Vec<(&'static str, &str)> {
        names.iter().filter_map(|n| crate::docx::xml::child(el, n).map(|c| (*n, &el.document().input_text()[c.range()]))).collect()
    };
    macro_rules! round_trip {
        ($holder:ty, $owned:expr, $frozen:expr) => {{
            let typed = <$holder>::parse(el);
            let held = <$holder>::read(typed.clone(), text.into());
            assert_eq!(held.xml(colours, cx).as_deref(), Some(text), "{what}: as read");
            let xml = <$holder>::serialize(&typed, &owned($owned), colours, cx);
            let again = cx.parse(&xml, |_, e| <$holder>::parse(e)).unwrap_or_else(|| panic!("{what}: written as {xml}, parses"));
            assert_eq!(again, typed, "{what}: typed, written as {xml}");
            let lost = cx.parse(&xml, |_, e| char_props::tests::lost(el, e)).unwrap_or_default();
            assert!(lost.is_empty(), "{what}: written as {xml}, loses {lost:?}");
            let frozen = $frozen(el);
            assert_eq!(cx.parse(&xml, |_, e| $frozen(e)).as_ref(), Some(&frozen), "{what}: on the XML path, written as {xml}");
            assert_eq!(<$holder>::written_over(&typed, colours, cx, Some((&typed, text)), &[]).as_deref(), Some(text), "{what}: written over itself");
            (typed, frozen)
        }};
    }
    match el.tag_name().name() {
        "tblPr" | "tblPrEx" => {
            let (typed, frozen) = if el.tag_name().name() == "tblPr" {
                counts.tables += 1;
                round_trip!(TableDirect, &["tblPrChange"], |e| TblProps::parse(Some(e), colours))
            } else {
                counts.exceptions += 1;
                round_trip!(ExceptionsDirect, &["tblPrExChange"], |e| TblProps::parse(Some(e), colours))
            };
            assert_eq!(tp::tbl_props(&typed, colours), frozen, "{what}: as the XML path holds it");
            assert_eq!(tp::tbl_props(&tp::from_tbl_props(&frozen), colours), frozen, "{what}: converted from the XML path and back");
            let first = |name| crate::docx::xml::child(el, name).and_then(crate::docx::xml::val);
            assert_eq!(typed.caption.as_deref(), first("tblCaption"), "{what}: caption");
            assert_eq!(typed.description.as_deref(), first("tblDescription"), "{what}: description");
        }
        "trPr" => {
            counts.rows += 1;
            let (typed, frozen) = round_trip!(RowDirect, &["ins", "del", "trPrChange"], |e| TrProps::parse(Some(e)));
            assert_eq!(tp::tr_props(&typed), frozen, "{what}: as the XML path holds it");
            assert_eq!(tp::tr_props(&tp::from_tr_props(&frozen)), frozen, "{what}: converted from the XML path and back");
            let first = |name| crate::docx::xml::child(el, name);
            assert_eq!(typed.division, first("divId").and_then(|n| crate::docx::xml::int(crate::docx::xml::val(n))), "{what}: division");
            assert_eq!(typed.hidden.unwrap_or(false), first("hidden").is_some_and(crate::docx::xml::on_off), "{what}: hidden");
        }
        "tcPr" => {
            counts.cells += 1;
            let (typed, mut frozen) =
                round_trip!(CellDirect, &["cellIns", "cellDel", "cellMerge", "tcPrChange"], |e| TcProps::parse(Some(e), colours));
            let first = crate::docx::xml::child(el, "hideMark");
            assert_eq!(typed.hide_mark.unwrap_or(false), first.is_some_and(crate::docx::xml::on_off), "{what}: hidden mark");
            // A strict or unknown text direction as Word writes its flow
            let flow = frozen.text_direction.as_deref().map(|v| tp::flow_literal(crate::docx::model::text_flow(v)));
            if flow != frozen.text_direction.as_deref() {
                counts.strict_flows += 1;
                frozen.text_direction = flow.map(str::to_string);
            }
            assert_eq!(tp::tc_props(&typed, colours), frozen, "{what}: as the XML path holds it");
            assert_eq!(tp::tc_props(&tp::from_tc_props(&frozen), colours), frozen, "{what}: converted from the XML path and back");
        }
        _ => {}
    }
}

/// Every table, row exceptions, row and cell property element in every fixture (the main part's, its tracked changes'
/// earlier ones, a table style's and its regions') round trips through the typed parse and serializer, and its typed
/// properties are what the XML path reads.
#[test]
fn table_property_xml_round_trips_through_the_typed_writer() {
    let mut counts = TableCounts::default();
    let table = |n: &roxmltree::Node| ["tblPr", "tblPrEx", "trPr", "tcPr"].iter().any(|name| crate::docx::xml::is_w(*n, name));
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        let colours = &res.styles.colours;
        let what = |raw: &str| format!("{}: {raw}", path.display());
        let src = &d.source.as_ref().expect("a source").xml;
        let doc = roxmltree::Document::parse(src).expect("the main part parses");
        for p in doc.descendants().filter(table) {
            typed_table_round_trip(p, &cx, colours, &what(&src[p.range()]), &mut counts);
        }
        let scx = res.styles.cx();
        let mut styles: Vec<&str> = res.styles.by_id.values().map(|s| s.raw.as_str()).collect();
        styles.extend(res.styles.defaults_raw.as_deref());
        for raw in styles {
            let parsed = scx.parse(raw, |_, el| {
                for p in el.descendants().filter(table) {
                    typed_table_round_trip(p, &scx, colours, &what(raw), &mut counts);
                }
            });
            assert!(parsed.is_some(), "{}: parses", what(raw));
        }
    }
    let TableCounts { tables, exceptions, rows, cells, strict_flows } = counts;
    eprintln!("checked {tables} tables, {exceptions} row exceptions, {rows} rows, {cells} cells; {strict_flows} strict text flows");
    assert!(
        tables > 0 && exceptions > 0 && rows > 0 && cells > 0,
        "only {tables} tables, {exceptions} exceptions, {rows} rows and {cells} cells checked"
    );
}

/// Every table, row exceptions, row and cell in every fixture: its typed properties over its style region's (converted
/// from the XML path's) fold to what the XML path folds.
#[test]
fn typed_table_resolution_gives_what_reading_resolved() {
    let mut n = [0usize; 4];
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        for s in &d.sections {
            resolve_tables(res, &cx, &s.blocks, &path.display().to_string(), &mut n);
        }
    }
    let [tables, exceptions, rows, cells] = n;
    eprintln!("resolved {tables} tables, {exceptions} row exceptions, {rows} rows, {cells} cells");
    assert!(
        tables > 100 && exceptions > 0 && rows > 100 && cells > 1000,
        "only {tables} tables, {exceptions} exceptions, {rows} rows and {cells} cells"
    );
}

fn resolve_tables(res: &Resolver, cx: &Cx, blocks: &[Block], what: &str, n: &mut [usize; 4]) {
    use crate::docx::props::{TblProps, TcProps, TrProps};
    use crate::docx::table_props::{self as tp, CellDirect, ExceptionsDirect, RowDirect, TableDirect};
    let colours = &res.styles.colours;
    for b in blocks {
        let Block::Table(t) = b else { continue };
        let xml = |raw: &crate::docx::Raw| raw.0.clone();
        let parse = |raw: &crate::docx::Raw, f: &mut dyn FnMut(roxmltree::Node)| {
            if let Some(x) = raw.0.as_deref() {
                cx.parse(x, |_, el| f(el)).unwrap_or_else(|| panic!("{what}: {x} parses"));
            }
        };
        let style = res.styles.table_region(t.style_id.as_deref(), &[]);
        let mut tbl = style.tbl.clone();
        let direct = TableDirect::from_xml(xml(&t.direct).as_deref(), cx);
        parse(&t.direct, &mut |el| tbl.merge(&TblProps::parse(Some(el), colours)));
        let mut typed = tp::from_tbl_props(&style.tbl);
        typed.merge(direct.get());
        assert_eq!(tp::tbl_props(&typed, colours), tbl, "{what}: table {:?}", t.direct.0);
        n[0] += 1;
        for (ri, row) in t.rows.iter().enumerate() {
            let mut tr = style.tr.clone();
            parse(&row.direct, &mut |el| tr.merge(&TrProps::parse(Some(el))));
            let mut typed_tr = tp::from_tr_props(&style.tr);
            typed_tr.merge(RowDirect::from_xml(xml(&row.direct).as_deref(), cx).get());
            assert_eq!(tp::tr_props(&typed_tr), tr, "{what}: row {:?}", row.direct.0);
            n[2] += 1;
            if row.exceptions.0.is_some() {
                let mut ex = tbl.clone();
                parse(&row.exceptions, &mut |el| ex.merge(&TblProps::parse(Some(el), colours)));
                let mut typed_ex = typed.clone();
                typed_ex.merge(ExceptionsDirect::from_xml(xml(&row.exceptions).as_deref(), cx).get());
                assert_eq!(tp::tbl_props(&typed_ex, colours), ex, "{what}: row exceptions {:?}", row.exceptions.0);
                n[1] += 1;
            }
            for (ci, c) in row.cells.iter().enumerate() {
                let region = res.region_of(t, ri, ci);
                let mut tc = region.tc.clone();
                parse(&c.direct, &mut |el| tc.merge(&TcProps::parse(Some(el), colours)));
                let mut typed_tc = tp::from_tc_props(&region.tc);
                typed_tc.merge(CellDirect::from_xml(xml(&c.direct).as_deref(), cx).get());
                assert_eq!(tp::tc_props(&typed_tc, colours), tc, "{what}: cell {:?}", c.direct.0);
                n[3] += 1;
                resolve_tables(res, cx, &c.blocks, what, n);
            }
        }
    }
}

/// Every paragraph's property XML in every fixture, written again by the property writer, parses to the same
/// properties, whatever prefix (or default namespace) the element names its namespace with; every run property
/// element (a run's, a mark's and a formatting change's earlier one, as held and as written in the main part, with
/// its revisions, and a style's, a table style region's and the document defaults') round trips through the typed
/// parse and serializer.
#[test]
fn property_xml_round_trips_through_the_writer() {
    let w = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let (had, has) = (format!(r#"<w:rPr {w}><w:b w:val="1"/></w:rPr>"#), format!(r#"<w:rPr {w}><w:b/></w:rPr>"#));
    let (had, has) = (roxmltree::Document::parse(&had).expect("parses"), roxmltree::Document::parse(&has).expect("parses"));
    assert_eq!(char_props::tests::lost(had.root_element(), has.root_element()).len(), 1, "a value left out is lost");
    let (mut ppr, mut rpr) = (0, 0);
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        let colours = &res.styles.colours;
        let what = |raw: &str| format!("{}: {raw}", path.display());
        for p in authored(&d) {
            if let Some(raw) = p.1.as_deref() {
                let again = Props::parse(Some(raw), "pPr", &cx).xml();
                let parse = |x: Option<&str>| x.and_then(|x| cx.parse(x, |_, el| ParaProps::parse(Some(el), colours)));
                assert_eq!(parse(Some(raw)).unwrap_or_default(), parse(again.as_deref()).unwrap_or_default(), "{}", what(raw));
                ppr += 1;
            }
        }
        each_rpr(&d, &mut |raw| {
            cx.parse(raw, |_, el| typed_round_trip(el, &cx, colours, &what(raw))).unwrap_or_else(|| panic!("{}: parses", what(raw)));
            rpr += 1;
        });
        let src = &d.source.as_ref().expect("a source").xml;
        let doc = roxmltree::Document::parse(src).expect("the main part parses");
        for n in doc.descendants().filter(|n| crate::docx::xml::is_w(*n, "rPr") && !in_change(*n)) {
            typed_round_trip(n, &cx, colours, &what(&src[n.range()]));
            rpr += 1;
        }
        let scx = res.styles.cx();
        let mut styles: Vec<&str> = res.styles.by_id.values().map(|s| s.raw.as_str()).collect();
        styles.extend(res.styles.defaults_raw.as_deref());
        for raw in styles {
            let n = scx.parse(raw, |_, el| {
                let rprs = el.descendants().filter(|n| crate::docx::xml::is_w(*n, "rPr") && !in_change(*n));
                rprs.map(|n| typed_round_trip(n, &scx, colours, &what(raw))).count()
            });
            rpr += n.unwrap_or_else(|| panic!("{}: parses", what(raw)));
        }
    }
    assert!(ppr > 7_000, "only {ppr} paragraph property elements checked");
    assert!(rpr > 4_500, "only {rpr} run property elements checked");
}

/// Whether a property element is a formatting change's earlier one.
fn in_change(n: roxmltree::Node) -> bool {
    n.parent_element().is_some_and(|p| p.tag_name().name().ends_with("PrChange"))
}

/// Every run property element a body paragraph's mark and text runs hold as written: their own and their formatting
/// changes' earlier ones.
fn each_rpr(d: &Document, f: &mut dyn FnMut(&str)) {
    fn walk(blocks: &[Block], f: &mut dyn FnMut(&str)) {
        for b in blocks {
            match b {
                Block::Para(p) => {
                    let runs = p.inlines.iter().filter_map(|i| if let Inline::Text(_, r) = i { Some(r) } else { None });
                    for r in std::iter::once(&p.mark).chain(runs) {
                        let previous = r.rev.fmt.as_ref().and_then(|c| c.previous.direct.as_read().map(|x| &**x));
                        r.direct.as_read().map(|x| &**x).into_iter().chain(previous).for_each(&mut *f);
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, f)),
            }
        }
    }
    d.sections.iter().for_each(|s| walk(&s.blocks, f));
}

/// Every body paragraph's mark and text runs, and their formatting changes' earlier formatting, resolved by the typed
/// resolver from the typed parse of their property XML over the paragraph's run base give the format reading gave,
/// theme references included.
#[test]
fn typed_resolution_gives_what_reading_resolved() {
    let (mut runs, mut marks, mut changes) = (0, 0, 0);
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        let env = RunEnv { colours: &res.styles.colours, fonts: &res.theme, categories: &res.categories, open_type: res.styles.open_type };
        let mut counts = (&mut runs, &mut marks, &mut changes);
        for s in &d.sections {
            resolve_blocks(res, &env, &cx, &s.blocks, None, &path.display().to_string(), &mut counts);
        }
    }
    assert!(runs > 20_000 && marks > 10_000 && changes >= 6, "only {runs} runs, {marks} marks and {changes} formatting changes checked");
}

fn resolve_blocks(
    res: &Resolver,
    env: &RunEnv,
    cx: &Cx,
    blocks: &[Block],
    region: Option<&Region>,
    what: &str,
    counts: &mut (&mut usize, &mut usize, &mut usize),
) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                if p.src.as_ref().is_some_and(|s| s.ranges.len() > 1) {
                    continue;
                }
                let base = char_props::from_run_props(&res.para_from_xml(p.style.direct.xml(&res.styles.colours, cx).as_deref(), region, cx).base);
                let typed = |raw: Option<&str>, toc: bool| {
                    let mut direct = raw.and_then(|x| cx.parse(x, |_, el| char_props::parse(el).0)).unwrap_or_default();
                    if toc && direct.style.as_deref() == Some("Hyperlink") {
                        direct.style = None;
                    }
                    let chain: Option<Vec<CharProps>> = direct
                        .style
                        .as_deref()
                        .map(|id| res.styles.chain(Some(id)).iter().map(|s| char_props::from_run_props(&s.base.rpr)).collect());
                    let style = chain.map(|c| CharProps::fold_chain(&c));
                    resolve::run_style(env, &resolve::run_props(&base, style.as_ref(), &direct))
                };
                let same = |read: &RunStyle, again: RunStyle, of: &str| {
                    let again = RunStyle { rev: read.rev.clone(), toc: read.toc, ..again };
                    assert!(read.layout_eq(&again), "{what}: {of} of {:?}: {read:?} against {again:?}", p.id);
                    let refs = |s: &RunStyle| (s.color_theme, s.underline_color_theme, s.font_theme);
                    assert_eq!(refs(read), refs(&again), "{what}: the theme references of {of} of {:?}", p.id);
                };
                let changed = |r: &RunStyle, counts: &mut (&mut usize, &mut usize, &mut usize)| {
                    if let Some(c) = &r.rev.fmt {
                        same(&c.previous, typed(c.previous.direct.as_read().map(|x| &**x), false), "a formatting change");
                        *counts.2 += 1;
                    }
                };
                same(&p.mark, typed(p.mark.direct.as_read().map(|x| &**x), false), "the mark");
                changed(&p.mark, counts);
                *counts.1 += 1;
                for i in &p.inlines {
                    if let Inline::Text(t, run) = i {
                        same(run, typed(run.direct.as_read().map(|x| &**x), run.toc), &format!("run {t:?}"));
                        changed(run, counts);
                        *counts.0 += 1;
                    }
                }
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, c) in row.cells.iter().enumerate() {
                        resolve_blocks(res, env, cx, &c.blocks, Some(&res.region_of(t, ri, ci)), what, counts);
                    }
                }
            }
        }
    }
}

/// Every body paragraph, and its tracked change's earlier formatting, resolved by the typed paragraph resolver from
/// the typed parse of its property XML in its table style region gives the format reading gave.
#[test]
fn typed_paragraph_resolution_gives_what_reading_resolved() {
    let (mut paras, mut changes) = (0, 0);
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let cx = cx(&d);
        for s in &d.sections {
            resolve_paras(res, &cx, &s.blocks, None, &path.display().to_string(), &mut (&mut paras, &mut changes));
        }
    }
    assert!(paras > 10_000 && changes >= 2, "only {paras} paragraphs and {changes} property changes checked");
}

fn resolve_paras(res: &Resolver, cx: &Cx, blocks: &[Block], region: Option<&Region>, what: &str, counts: &mut (&mut usize, &mut usize)) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                if p.src.as_ref().is_some_and(|s| s.ranges.len() > 1) {
                    continue;
                }
                let typed = |raw: Option<&str>| {
                    let direct = raw.and_then(|x| cx.parse(x, |_, el| para_props::parse(el))).unwrap_or_default();
                    let mark = raw.and_then(|x| {
                        cx.parse(x, |_, el| crate::docx::xml::child(el, "rPr").map(|r| char_props::parse(r).0)).flatten().and_then(|m| m.size)
                    });
                    para_xml::resolve(res, &direct, mark, region).style
                };
                let mut again = typed(p.style.direct.as_read().map(|x| &**x));
                (again.change, again.in_frame) = (p.style.change.clone(), p.style.in_frame);
                if p.style.in_frame {
                    again.frame = p.style.frame.clone();
                }
                assert!(p.style.layout_eq(&again), "{what}: paragraph {:?}: {:?} against {again:?}", p.id, p.style);
                *counts.0 += 1;
                if let Some(c) = p.style.change.as_ref().filter(|c| c.properties && c.previous.direct.as_read().is_some()) {
                    let mut was = typed(c.previous.direct.as_read().map(|x| &**x));
                    (was.in_frame, was.frame) = (c.previous.in_frame, c.previous.frame.clone());
                    assert!(c.previous.layout_eq(&was), "{what}: the earlier formatting of {:?}: {:?} against {was:?}", p.id, c.previous);
                    *counts.1 += 1;
                }
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, c) in row.cells.iter().enumerate() {
                        resolve_paras(res, cx, &c.blocks, Some(&res.region_of(t, ri, ci)), what, counts);
                    }
                }
            }
        }
    }
}

#[test]
fn bold_on_a_run_under_a_locally_bound_prefix_keeps_its_underline() {
    let label = "Main namespace bound to w on this paragraph";
    let mut e = Editor::open(&model("namespace-dependencies.docx"), fonts()).expect("opens");
    select(&mut e, At::Text(label));
    assert!(matches!(e.execute(C::SetBold(true), fonts()), Outcome::Changed(_)));
    let holds = |d: &Document| {
        let ids: Vec<ParaId> = authored(d).iter().map(|p| ParaId(p.0)).collect();
        let (_, p) = para_by_label(d, &ids, label);
        let run = p.inlines.iter().find_map(Inline::run).expect("a run");
        assert!(run.bold && run.underline.is_some(), "the run is bold and keeps its underline: {:?}", run.direct.as_read());
    };
    holds(e.document());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    holds(&crate::docx::read_editable(&saved).expect("the saved package reads"));
}
