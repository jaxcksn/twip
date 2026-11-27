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
use crate::document::paragraph::ListId;
use crate::docx::list_xml::NumberingXml;
use crate::docx::props::{ParaProps, RunProps, ThemeColours};
use crate::docx::style_xml::StylesPart;
use crate::docx::styles::{Region, Styles};
use crate::docx::write::props::{Cx, Props};
use crate::docx::xml::{attr, is_w};
use crate::docx::{Block, Document, Inline, Paragraph, Run, RunStyle};
use crate::docx::{char_props, page_borders, para_props, para_xml, run_xml, style_xml_tests};
use crate::format::cascade::{Cascade, Rules, TableRegion};
use crate::format::resolve::{self, RunEnv};
use crate::format::{CharProps, Color, RegionKind, StyleKind, StyleProp, StyleSet};
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
fn written(res: &crate::docx::resolvers::Resolvers, cx: &Cx, r: &RunStyle) -> Option<String> {
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
    carried_matches(d, what);
    checked
}

/// The document's carried typed definitions cascade as its carried resolver does: every definition, and every
/// paragraph, run, label and cell of it.
fn carried_matches(d: &Document, what: &str) -> CascadeCounts {
    let res = d.resolver.as_deref().expect("a resolver");
    let typed = res.typed();
    let mut n = CascadeCounts::default();
    let what = format!("{what}: carried");
    definitions_match(res, &typed.cascade, &typed.numbering, &what, &mut n);
    let (cx, mut counters) = (cx(d), Default::default());
    for s in &d.sections {
        document_matches(res, &typed.cascade, &cx, &s.blocks, None, &mut counters, &what, &mut n);
    }
    n
}

fn check_blocks(res: &crate::docx::resolvers::Resolvers, cx: &Cx, blocks: &[Block], region: Option<&Region>, what: &str, checked: &mut usize) {
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
    fn walk(res: &crate::docx::resolvers::Resolvers, cx: &Cx, blocks: &[Block], out: &mut Authored) {
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
fn run_props(res: &crate::docx::resolvers::Resolvers, cx: &Cx, p: &Paragraph, patch: Option<Patch>) -> Vec<RunProps> {
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

fn para_props(res: &crate::docx::resolvers::Resolvers, cx: &Cx, p: &Paragraph, patch: Option<Patch>) -> ParaProps {
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

fn resolve_tables(res: &crate::docx::resolvers::Resolvers, cx: &Cx, blocks: &[Block], what: &str, n: &mut [usize; 4]) {
    use crate::docx::props::{TblProps, TcProps, TrProps};
    use crate::docx::table_props::{self as tp, CellDirect, ExceptionsDirect, RowDirect, TableDirect};
    let colours = &res.styles.colours;
    for b in blocks {
        let Block::Table(t) = b else { continue };
        let parse = |raw: Option<&Rc<str>>, f: &mut dyn FnMut(roxmltree::Node)| {
            if let Some(x) = raw {
                cx.parse(x, |_, el| f(el)).unwrap_or_else(|| panic!("{what}: {x} parses"));
            }
        };
        let style = res.styles.table_region(t.style_id.as_deref(), &[]);
        let mut tbl = style.tbl.clone();
        let read = t.direct.as_read();
        assert!(t.direct.authored_eq(&TableDirect::from_xml(read.map(|x| &**x), cx)), "{what}: table {read:?} as held");
        parse(read, &mut |el| tbl.merge(&TblProps::parse(Some(el), colours)));
        let mut typed = tp::from_tbl_props(&style.tbl);
        typed.merge(t.direct.get());
        assert_eq!(tp::tbl_props(&typed, colours), tbl, "{what}: table {read:?}");
        n[0] += 1;
        for (ri, row) in t.rows.iter().enumerate() {
            let mut tr = style.tr.clone();
            let read = row.direct.as_read();
            assert!(row.direct.authored_eq(&RowDirect::from_xml(read.map(|x| &**x), cx)), "{what}: row {read:?} as held");
            parse(read, &mut |el| tr.merge(&TrProps::parse(Some(el))));
            let mut typed_tr = tp::from_tr_props(&style.tr);
            typed_tr.merge(row.direct.get());
            assert_eq!(tp::tr_props(&typed_tr), tr, "{what}: row {read:?}");
            n[2] += 1;
            if let Some(read) = row.exceptions.as_read() {
                assert!(row.exceptions.authored_eq(&ExceptionsDirect::from_xml(Some(read), cx)), "{what}: row exceptions {read:?} as held");
                let mut ex = tbl.clone();
                parse(Some(read), &mut |el| ex.merge(&TblProps::parse(Some(el), colours)));
                let mut typed_ex = typed.clone();
                typed_ex.merge(row.exceptions.get());
                assert_eq!(tp::tbl_props(&typed_ex, colours), ex, "{what}: row exceptions {read:?}");
                n[1] += 1;
            }
            for (ci, c) in row.cells.iter().enumerate() {
                let region = res.region_of(t, ri, ci);
                let mut tc = region.tc.clone();
                let read = c.direct.as_read();
                assert!(c.direct.authored_eq(&CellDirect::from_xml(read.map(|x| &**x), cx)), "{what}: cell {read:?} as held");
                parse(read, &mut |el| tc.merge(&TcProps::parse(Some(el), colours)));
                let mut typed_tc = tp::from_tc_props(&region.tc);
                typed_tc.merge(c.direct.get());
                assert_eq!(tp::tc_props(&typed_tc, colours), tc, "{what}: cell {read:?}");
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
    res: &crate::docx::resolvers::Resolvers,
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

fn resolve_paras(
    res: &crate::docx::resolvers::Resolvers,
    cx: &Cx,
    blocks: &[Block],
    region: Option<&TableRegion>,
    what: &str,
    counts: &mut (&mut usize, &mut usize),
) {
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
                        resolve_paras(res, cx, &c.blocks, Some(&res.cascade().region_of(t, ri, ci)), what, counts);
                    }
                }
            }
        }
    }
}

// Style definitions

/// A relationship target of `from` of the kind its type ends with.
fn related(pkg: &[u8], from: &str, kind: &str) -> Option<String> {
    let rels = crate::docx::package::read(pkg, &[], &crate::docx::package::rels_name(from)).ok()??;
    let dom = roxmltree::Document::parse(&rels).ok()?;
    let target = dom
        .root_element()
        .children()
        .filter(|r| r.attribute("Type").is_some_and(|t| t.ends_with(&format!("/{kind}"))) && r.attribute("TargetMode") != Some("External"))
        .find_map(|r| r.attribute("Target"))?;
    Some(crate::docx::package::resolve(from, target))
}

/// Every styles part of every fixture (the document's and the glossary's), with its document's theme colours.
fn styles_parts() -> Vec<(String, String, ThemeColours)> {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    let mut out = Vec::new();
    for path in paths {
        let Ok(pkg) = std::fs::read(&path) else { continue };
        let Some(main) = related(&pkg, "", "officeDocument") else { continue };
        let read = |name: Option<String>| name.and_then(|n| crate::docx::package::read(&pkg, &[], &n).ok().flatten());
        let theme = crate::drawing::theme::Theme::parse(read(related(&pkg, &main, "theme")).as_deref());
        let settings = read(related(&pkg, &main, "settings"));
        let tree = settings.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
        let colours = ThemeColours::new(&theme, &crate::drawing::theme::ColourMap::parse(tree.as_ref().map(|d| d.root_element())));
        let mut sources = vec![main.clone()];
        sources.extend(related(&pkg, &main, "glossaryDocument"));
        for from in sources {
            if let Some(xml) = read(related(&pkg, &from, "styles")) {
                out.push((format!("{}: {from}", path.display()), xml, colours));
            }
        }
    }
    out
}

/// Every `w:style` (with an id), `w:docDefaults` and `w:latentStyles` of every fixture's styles parts round trips
/// through the typed parse, serializer and holders.
#[test]
fn style_definitions_round_trip_through_the_typed_writer() {
    let mut n = style_xml_tests::Counts::default();
    for (what, xml, colours) in styles_parts() {
        let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{what}: {e}"));
        let cx = StylesPart::parse(Some(&xml)).cx();
        for el in doc.root_element().children().filter(|c| c.is_element()) {
            let style = is_w(el, "style") && attr(el, "styleId").is_some();
            if style || is_w(el, "docDefaults") || is_w(el, "latentStyles") {
                style_xml_tests::round_trip(el, &cx, &colours, &format!("{what}: {}", &xml[el.range()]), &mut n);
            }
        }
    }
    eprintln!("round tripped {n:?}");
    assert!(n.styles > 2000 && n.regions > 700 && n.defaults > 200 && n.latent > 0, "only {n:?}");
}

/// The typed reading of every fixture's styles parts converts to the XML path's reading of the same part: every style
/// (its fields, base and regions as read, a table style's run properties from Word's built-in table defaults where it
/// takes them, Word's built-in default table style's fixed properties), the default styles, the paragraph styles'
/// order, the document defaults, the latent styles and the frozen table styles.
#[test]
fn typed_style_sheets_convert_to_what_reading_resolved() {
    let (mut n, mut parts) = (style_xml_tests::SheetCounts::default(), 0);
    for (what, xml, colours) in styles_parts() {
        let frozen = Styles::parse(Some(&xml), colours);
        let k = style_xml_tests::sheet_matches(&frozen, &StylesPart::parse(Some(&xml)), &colours, &what);
        parts += 1;
        n.styles += k.styles;
        n.table_styles += k.table_styles;
        n.regions += k.regions;
        n.frozen += k.frozen;
        n.run_defaults += k.run_defaults;
        n.exceptions += k.exceptions;
    }
    eprintln!("converted {parts} parts: {n:?}");
    assert!(parts > 200 && n.styles > 2000 && n.table_styles > 300 && n.regions > 700, "only {parts} parts: {n:?}");
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

// Typed cascade

/// Fixture `d`'s typed definitions, read from its package's parts apart from the reader: the styles and numbering
/// parts and the glossary's, merged as the reader merges them; picture bullets' images, the theme, the settings'
/// rules, the font categories and the divisions as the reader's resolver holds them.
fn typed_cascade(d: &Document) -> (Cascade, StylesPart, NumberingXml) {
    let res = d.resolver.as_deref().expect("a resolver");
    let src = d.source.as_ref().expect("a source");
    let pkg = &src.package;
    let read = |name: Option<String>| name.and_then(|n| crate::docx::package::read(pkg, &[], &n).ok().flatten());
    let styles = StylesPart::parse(read(related(pkg, &src.main, "styles")).as_deref());
    let numbering = NumberingXml::parse(read(related(pkg, &src.main, "numbering")).as_deref());
    let (mut sheet, mut lists) = (styles.sheet.clone(), numbering.lists());
    if let Some(g) = related(pkg, &src.main, "glossaryDocument") {
        let gstyles = StylesPart::parse(read(related(pkg, &g, "styles")).as_deref());
        let glists = NumberingXml::parse(read(related(pkg, &g, "numbering")).as_deref()).lists();
        crate::docx::glossary::merge_definitions(&mut sheet, &mut lists, &gstyles.sheet, &glists);
    }
    for (id, p) in lists.pictures.iter_mut() {
        Rc::make_mut(p).image = res.numbering.pic_bullets.get(id).and_then(|b| b.image);
    }
    let rules = Rules { word2002_rules: res.styles.word2002_rules, cell_size_rules: res.styles.cell_size_rules, open_type: res.styles.open_type };
    let mut c = Cascade::new(Rc::new(sheet), Rc::new(lists), res.theme.clone(), res.styles.colours, rules);
    (c.categories, c.divisions) = (res.categories.clone(), res.divisions.clone());
    (c, styles, numbering)
}

/// What the typed cascade was compared on.
#[derive(Default, Debug)]
struct CascadeCounts {
    styles: usize,
    table_regions: usize,
    lists: usize,
    levels: usize,
    links: usize,
    counted: usize,
    paragraphs: usize,
    runs: usize,
    labels: usize,
    cells: usize,
}

/// The definitions: every style's chain and its folds, its dependants, every table style's base and single regions
/// and what it defines, every list's definition and levels, the style links.
fn definitions_match(res: &crate::docx::resolvers::Resolvers, c: &Cascade, numbering: &NumberingXml, what: &str, n: &mut CascadeCounts) {
    let colours = &res.styles.colours;
    let mut ids: Vec<&String> = res.styles.by_id.keys().collect();
    ids.sort();
    let mut typed_ids: Vec<String> = c.styles.styles.keys().map(|k| k.to_string()).collect();
    typed_ids.sort();
    assert_eq!(ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), typed_ids, "{what}: style ids");
    for key in &ids {
        let (key, id) = (key.as_str(), Some(key.as_str()));
        let frozen_chain: Vec<String> = res.styles.chain(id).iter().map(|s| s.id.clone()).collect();
        let typed_chain: Vec<String> = c.chain(id).iter().map(|s| s.style.id.to_string()).collect();
        assert_eq!(typed_chain, frozen_chain, "{what}: {id:?}: chain");
        assert_eq!(para_props::to_para_props(&c.chain_para(id), colours), res.styles.chain_ppr(id), "{what}: {id:?}: paragraph chain");
        assert_eq!(char_props::to_run_props(&c.chain_char(id), colours), res.styles.chain_rpr(id), "{what}: {id:?}: run chain");
        let deps: Vec<String> = c.dependants(key).iter().map(|s| s.to_string()).collect();
        assert_eq!(deps, res.styles.dependants(key), "{what}: {id:?}: dependants");
        assert_eq!(c.paragraph_style(id), res.styles.paragraph_style(id), "{what}: {id:?}: as a paragraph style");
        assert_eq!(c.table_style(id), res.styles.table_style(id), "{what}: {id:?}: as a table style");
        let (e, f) = (&c.styles.styles[key], &res.styles.by_id[key]);
        let origin = matches!(e.origin, crate::format::Origin::Glossary) == (f.origin == crate::docx::styles::StyleOrigin::Glossary);
        assert!(origin && e.order == f.order && e.frozen == f.frozen, "{what}: {id:?}: origin, order and frozen");
        n.styles += 1;
        if c.styles.styles[key].style.kind != StyleKind::Table {
            continue;
        }
        style_xml_tests::regions_match(&res.styles.table_region(id, &[]), &c.table_region(id, &[]), colours, &format!("{what}: {id:?}: base"));
        for kind in RegionKind::ALL {
            let name = crate::docx::style_xml::region_literal(kind);
            style_xml_tests::regions_match(
                &res.styles.table_region(id, &[name]),
                &c.table_region(id, &[kind]),
                colours,
                &format!("{what}: {id:?}: {name}"),
            );
            assert_eq!(c.table_defines(id, kind), res.styles.table_defines(id, name), "{what}: {id:?}: defines {name}");
            n.table_regions += 1;
        }
    }
    let order: Vec<String> = c.styles.paragraph_order.iter().map(|s| s.to_string()).collect();
    assert_eq!(order, res.styles.paragraph_order, "{what}: paragraph styles' order");
    let text = |v: &Option<Rc<str>>| v.as_deref().map(str::to_string);
    assert_eq!(text(&c.styles.default_paragraph), res.styles.default_paragraph, "{what}: default paragraph style");
    assert_eq!(text(&c.styles.default_table), res.styles.default_table, "{what}: default table style");
    let ncx = numbering.cx();
    let mut lists: Vec<i64> = res.numbering.nums.keys().copied().collect();
    lists.sort();
    let mut typed_lists: Vec<i64> = c.lists.lists.keys().map(|k| k.0).collect();
    typed_lists.sort();
    assert_eq!(typed_lists, lists, "{what}: lists");
    let level = |l: Option<&Rc<crate::format::ListLevel>>| l.map(|l| crate::docx::list_xml::to_level(l, colours, &ncx));
    let same = |a: Option<crate::docx::numbering::Level>, b: Option<&crate::docx::numbering::Level>, of: &str| match (a, b) {
        (Some(a), Some(b)) => {
            assert!(crate::docx::list_xml::tests::level_diff(&a, b).is_empty(), "{what}: {of}: {:?}", crate::docx::list_xml::tests::level_diff(&a, b))
        }
        (a, b) => assert_eq!(a.is_some(), b.is_some(), "{what}: {of}"),
    };
    for id in lists {
        let list = ListId(id);
        assert_eq!(c.abstract_of(list).map(|a| a.0), res.numbering.abstract_of(id, &res.styles), "{what}: list {id}: definition");
        let (typed, frozen) = (c.levels(list), res.numbering.levels(id, &res.styles));
        assert_eq!(typed.is_some(), frozen.is_some(), "{what}: list {id}: levels");
        for k in 0..9i64 {
            same(level(c.level(list, k)), res.numbering.level(id, k, &res.styles), &format!("list {id} level {k}"));
            if let (Some(t), Some(f)) = (&typed, &frozen) {
                same(level(t[k as usize]), f[k as usize], &format!("list {id} levels {k}"));
                if let (Some(t), Some(f)) = (t[k as usize], f[k as usize]) {
                    assert_eq!(c.bullet_image(t), res.numbering.bullet_image(f), "{what}: list {id} level {k}: bullet image");
                }
            }
            n.levels += 1;
        }
        n.lists += 1;
    }
    let mut links: Vec<((i64, i64), String)> = c.style_links.iter().map(|((a, l), s)| ((a.0, *l), s.to_string())).collect();
    links.sort();
    let mut frozen: Vec<((i64, i64), String)> = res.style_links.iter().map(|(k, s)| (*k, s.clone())).collect();
    frozen.sort();
    assert_eq!(links, frozen, "{what}: style links");
    n.links += links.len();
}

/// The document's paragraphs, marks, text runs, labels and cells resolved by the typed cascade from their typed
/// properties give what reading resolved; its lists' labels counted in order give the XML path's.
#[allow(clippy::too_many_arguments)]
fn document_matches(
    res: &crate::docx::resolvers::Resolvers,
    c: &Cascade,
    cx: &Cx,
    blocks: &[Block],
    region: Option<&TableRegion>,
    counters: &mut (crate::docx::numbering::Counters, crate::format::counters::Counters),
    what: &str,
    n: &mut CascadeCounts,
) {
    for b in blocks {
        match b {
            Block::Para(p) => {
                if let Some(r) = p.style.num {
                    let (from_style, style_id) = {
                        let direct = p.style.direct.get();
                        let typed = c.paragraph(direct, None, region);
                        (typed.from_style, typed.style_id.as_deref().map(str::to_string))
                    };
                    let shows = res.label_shows(r.list.0, r.level, from_style, style_id.as_deref());
                    assert_eq!(c.label_shows(r.list, r.level, from_style, style_id.as_deref()), shows, "{what}: {:?}: label shows", p.id);
                    if shows {
                        let frozen = counters.0.next(&res.numbering, &res.styles, r.list.0, r.level);
                        assert_eq!(counters.1.next(c, r.list, r.level), frozen, "{what}: {:?}: label", p.id);
                        let vals = counters.0.values(&res.numbering, &res.styles, r.list.0);
                        assert_eq!(counters.1.values(c, r.list), vals, "{what}: {:?}: counters", p.id);
                        assert_eq!(counters.1.value(c, r.list, r.level), counters.0.value(&res.numbering, &res.styles, r.list.0, r.level));
                        n.counted += 1;
                    }
                }
                if p.src.as_ref().is_some_and(|s| s.ranges.len() > 1) {
                    continue;
                }
                let typed = para_xml::of_typed(c, &p.style.direct, region, cx);
                let again = para_xml::again_typed(c, &p.style, typed.style.clone(), region, cx);
                assert!(p.style.layout_eq(&again), "{what}: paragraph {:?}: {:?} against {again:?}", p.id, p.style);
                if let (Some(was), Some(now)) = (&p.style.change, &again.change) {
                    assert!(was.previous.layout_eq(&now.previous), "{what}: the earlier formatting of {:?}", p.id);
                }
                n.paragraphs += 1;
                let same = |read: &RunStyle, again: &RunStyle, of: &str| {
                    assert!(read.layout_eq(again), "{what}: {of} of {:?}: {read:?} against {again:?}", p.id);
                    let refs = |s: &RunStyle| (s.color_theme, s.underline_color_theme, s.font_theme);
                    assert_eq!(refs(read), refs(again), "{what}: the theme references of {of} of {:?}", p.id);
                    if let (Some(a), Some(b)) = (&read.rev.fmt, &again.rev.fmt) {
                        assert!(a.previous.layout_eq(&b.previous), "{what}: the earlier formatting of {of} of {:?}", p.id);
                    }
                };
                let mark = RunStyle { rev: p.mark.rev.clone(), ..(*run_xml::rerun_typed(c, &typed.base, &p.mark)).clone() };
                same(&p.mark, &mark, "the mark");
                for i in &p.inlines {
                    if let Inline::Text(t, r) = i {
                        same(r, &run_xml::rerun_typed(c, &typed.base, r), &format!("run {t:?}"));
                        n.runs += 1;
                    }
                }
                let mark_own = p.mark.direct.props().map(|m| (**m).clone());
                if let (Some(label), Some(level)) = (&p.list, &typed.level) {
                    let under = c.run_props(&typed.base, &mark_own.clone().unwrap_or_default());
                    same(&label.run, &c.label(&under, Some(&level.char)), "the label");
                    assert_eq!(label.picture, c.bullet_image(level), "{what}: the label picture of {:?}", p.id);
                    n.labels += 1;
                }
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, cell) in row.cells.iter().enumerate() {
                        let typed = c.region_of(t, ri, ci);
                        style_xml_tests::regions_match(&res.region_of(t, ri, ci), &typed, &res.styles.colours, &format!("{what}: cell {ri} {ci}"));
                        n.cells += 1;
                        document_matches(res, c, cx, &cell.blocks, Some(&typed), counters, what, n);
                    }
                }
            }
        }
    }
}

/// Every fixture's typed definitions, read apart from the reader, cascade as the reader's resolver does: every style
/// chain, table style region, list level and style link, and every paragraph, mark, text run, label and cell of the
/// document; its lists counted in order label alike.
#[test]
fn typed_cascade_gives_what_the_resolver_gives() {
    let mut n = CascadeCounts::default();
    for (path, d) in fixtures() {
        let res = d.resolver.as_deref().expect("a resolver");
        let (c, _, numbering) = typed_cascade(&d);
        let what = path.display().to_string();
        definitions_match(res, &c, &numbering, &what, &mut n);
        let cx = cx(&d);
        let mut counters = Default::default();
        for s in &d.sections {
            document_matches(res, &c, &cx, &s.blocks, None, &mut counters, &what, &mut n);
        }
    }
    eprintln!("cascaded {n:?}");
    assert!(n.styles > 2000 && n.paragraphs > 10_000 && n.runs > 20_000 && n.cells > 1000 && n.counted > 100, "only {n:?}");
}

/// After each of a few style edits (a table style based on another, a style deleted), both readings of every
/// fixture's styles part work out again alike what depends on the styles together.
#[test]
fn style_edits_work_the_read_order_rule_out_again_alike() {
    let (mut sets, mut deletes, mut loops) = (0, 0, 0);
    for (what, xml, colours) in styles_parts() {
        let mut frozen = Styles::parse(Some(&xml), colours);
        let mut typed = StylesPart::parse(Some(&xml));
        let cx = typed.cx();
        let order: Vec<Rc<str>> = typed.sheet.in_order().iter().map(|e| e.style.id.clone()).collect();
        let tables: Vec<Rc<str>> = order.iter().filter(|id| typed.sheet.styles[*id].style.kind == StyleKind::Table).cloned().collect();
        // Each table style based on the last one, unless that comes from it; the last on none
        for (k, id) in tables.iter().enumerate() {
            let last = tables.last().filter(|l| *l != id);
            let c = Cascade::new(Rc::new(typed.sheet.clone()), Rc::default(), Default::default(), colours, Rules::default());
            if last.is_some_and(|l| c.dependants(id).contains(l)) || k + 1 < tables.len() && last.is_none() {
                continue;
            }
            let held = &typed.styles[id];
            let mut s = (**held.get()).clone();
            s.based_on = last.cloned();
            let edited = held.edited(s, &[StyleSet::Style(StyleProp::BasedOn)]);
            let raw = edited.xml(&colours, &cx).into_owned();
            frozen.set_xml(raw).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            typed.set(edited, &colours).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            style_xml_tests::sheet_matches(&frozen, &typed, &colours, &format!("{what}: {id} based on {last:?}"));
            sets += 1;
        }
        // Each style based on one of its dependants: refused alike, nothing changed
        let c = Cascade::new(Rc::new(typed.sheet.clone()), Rc::default(), Default::default(), colours, Rules::default());
        for id in &order {
            let Some(dependant) = c.dependants(id).into_iter().find(|d| d != id) else { continue };
            let held = &typed.styles[id];
            let mut s = (**held.get()).clone();
            s.based_on = Some(dependant.clone());
            let edited = held.edited(s, &[StyleSet::Style(StyleProp::BasedOn)]);
            let raw = edited.xml(&colours, &cx).into_owned();
            let before = style_xml_tests::written(&typed, &colours);
            let frozen_set = frozen.set_xml(raw);
            let typed_set = typed.set(edited, &colours);
            assert_eq!(typed_set, frozen_set.map(|_| ()), "{what}: {id} based on {dependant}");
            assert!(typed_set.is_err(), "{what}: {id} based on {dependant} loops");
            assert_eq!(style_xml_tests::written(&typed, &colours), before, "{what}: a refused set changes nothing");
            loops += 1;
        }
        // The style the most others are based on, and the first table style
        let based = |id: &str| typed.sheet.styles.values().filter(|e| e.style.based_on.as_deref() == Some(id)).count();
        let busiest = order.iter().max_by_key(|id| based(id)).cloned();
        for id in busiest.into_iter().chain(tables.first().cloned()) {
            if !typed.sheet.styles.contains_key(&id) {
                continue;
            }
            frozen.delete(&id).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            assert!(typed.delete(&id, &colours));
            style_xml_tests::sheet_matches(&frozen, &typed, &colours, &format!("{what}: {id} deleted"));
            deletes += 1;
        }
    }
    eprintln!("{sets} table styles based anew, {loops} loops refused, {deletes} styles deleted");
    assert!(sets > 300 && deletes > 300 && loops > 100, "only {sets} styles based anew and {deletes} deleted");
}

/// After each style, defaults, list and font edit, its undo and its redo, the document's carried typed definitions
/// cascade as its carried resolver does.
#[test]
fn carried_cascade_matches_after_edits() {
    use crate::api::command::{DocumentDefaults, LevelChange, ListKind, ReplaceFont};
    use crate::api::styles::{NewStyle, StyleChange};
    let change = |id: &str, change: StyleChange| C::ModifyStyle(StyleModification { id: StyleId(id.into()), change });
    let edits: Vec<(&str, &str, At, Vec<Command>)> = vec![
        ("a style's formatting", TOGGLES, At::Start("Bold paragraph style alone"), vec![style("BoldPara", C::SetBold(false))]),
        (
            "a style's base",
            TOGGLES,
            At::Start("Bold paragraph style alone"),
            vec![change("BoldPara", StyleChange { based_on: Some(None), ..Default::default() })],
        ),
        (
            "a new style",
            OFF,
            At::Whole("Underlined character style"),
            vec![C::CreateStyle(NewStyle { from_selection: true, apply: true, ..Default::default() })],
        ),
        (
            "a new style deleted",
            OFF,
            At::Whole("Underlined character style"),
            vec![C::CreateStyle(NewStyle { from_selection: true, apply: true, ..Default::default() }), C::DeleteStyle(StyleId("Style1".into()))],
        ),
        (
            "the defaults",
            OFF,
            At::Whole("Underlined character style"),
            vec![C::SetDocumentDefaults(DocumentDefaults { size: Some(Points(13.0)), ..Default::default() })],
        ),
        ("a bulleted list", OFF, At::Whole("Underlined character style"), vec![C::SetList(Some(ListKind::Bullet))]),
        (
            "a numbered list, a level deeper, restarted",
            OFF,
            At::Whole("Underlined character style"),
            vec![C::SetList(Some(ListKind::Numbered)), C::SetListLevel(LevelChange::Increase), C::RestartNumbering],
        ),
        ("a table style", THEME, At::Start("Cell borders in accent 2"), vec![C::SetTableStyle(StyleId("ThemeGrid".into()))]),
        (
            "a font replaced",
            OFF,
            At::Whole("Underlined character style"),
            vec![C::ReplaceFont(ReplaceFont { from: "Calibri".into(), to: "Arial".into(), scope: Default::default(), bold: None, italic: None })],
        ),
    ];
    for (what, fixture, at, commands) in edits {
        let mut e = Editor::open(&model(fixture), fonts()).expect("opens");
        e.complete(fonts());
        carried_matches(e.document(), &format!("{what}: as read"));
        select(&mut e, at);
        let (mut steps, commands_len) = (0, commands.len());
        for c in commands {
            if let Outcome::Changed(_) = e.execute(c.clone(), fonts()) {
                steps += 1;
            }
            carried_matches(e.document(), &format!("{what}: {c:?}"));
        }
        assert!(steps == commands_len, "{what}: only {steps} of {commands_len} changed");
        for _ in 0..steps {
            e.execute(Command::Undo(Default::default()), fonts());
            carried_matches(e.document(), &format!("{what}: undone"));
        }
        for _ in 0..steps {
            e.execute(Command::Redo(Default::default()), fonts());
            carried_matches(e.document(), &format!("{what}: redone"));
        }
    }
}
