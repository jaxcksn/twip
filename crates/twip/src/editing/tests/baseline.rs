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
use crate::docx::props::{ParaProps, RunProps};
use crate::docx::resolve::Resolver;
use crate::docx::styles::Region;
use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, Document, Inline, Paragraph, Run, RunStyle};
use crate::layout::{ParaId, Unit};

fn model(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/model").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

fn cx(d: &Document) -> Cx {
    Cx::of_xml(&d.source.as_ref().expect("read for editing").xml)
}

// Re-resolution from the property XML each node holds

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
                let mut r = res.para_from_xml(p.style.direct.0.as_deref(), region, cx);
                r.style.change = p.style.change.clone();
                r.style.in_frame = p.style.in_frame;
                r.style.frame = p.style.frame.clone();
                assert_eq!(*p.style, r.style, "{what}: paragraph {:?}", p.id);
                let with_rev = |again: Run, read: &Run| RunStyle { rev: read.rev.clone(), ..(*again).clone() };
                let mark = with_rev(res.run_from_xml(&r.base, p.mark.direct.0.as_deref(), cx), &p.mark);
                assert_eq!(*p.mark, mark, "{what}: the mark of {:?}", p.id);
                for i in &p.inlines {
                    if let Inline::Text(t, run) = i {
                        assert_eq!(**run, *res.rerun(&r.base, run, cx), "{what}: run {t:?} of {:?}", p.id);
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
    fn walk(blocks: &[Block], out: &mut Authored) {
        for b in blocks {
            match b {
                Block::Para(p) => out.push((
                    p.id,
                    p.style.direct.0.clone(),
                    p.mark.direct.0.clone(),
                    p.inlines.iter().filter_map(|i| if let Inline::Text(_, r) = i { Some(r.direct.0.clone()) } else { None }).collect(),
                )),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    d.sections.iter().for_each(|s| walk(&s.blocks, &mut out));
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
                    let mut props = Props::parse(r.direct.0.as_deref(), "rPr", cx);
                    f(&mut props, cx);
                    props.xml()
                }
                None => r.direct.0.as_deref().map(str::to_string),
            };
            std::iter::repeat_n(parse(raw.as_deref()), t.chars().count())
        })
        .collect()
}

fn para_props(res: &Resolver, cx: &Cx, p: &Paragraph, patch: Option<Patch>) -> ParaProps {
    let raw = match patch {
        Some(f) => {
            let mut props = Props::parse(p.style.direct.0.as_deref(), "pPr", cx);
            f(&mut props, cx);
            props.xml()
        }
        None => p.style.direct.0.as_deref().map(str::to_string),
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

/// Every paragraph's, mark's and text run's property XML in every fixture, written again by the property writer,
/// parses to the same properties, whatever prefix (or default namespace) the element names its namespace with.
#[test]
fn property_xml_round_trips_through_the_writer() {
    let mut paths = Vec::new();
    crate::layout::tests::output::fixtures(&crate::layout::tests::output::root().join("fixtures"), &mut paths);
    let mut checked = 0;
    for path in &paths {
        let Ok(d) = crate::docx::read_editable(&std::fs::read(path).expect("reads")) else { continue };
        let (Some(res), Some(_)) = (d.resolver.as_deref(), d.source.as_ref()) else { continue };
        let cx = cx(&d);
        let colours = &res.styles.colours;
        let mut each = |raw: Option<&str>, local: &str| {
            let Some(raw) = raw else { return };
            let again = Props::parse(Some(raw), local, &cx).xml();
            let what = format!("{}: {raw}", path.display());
            if local == "rPr" {
                let parse = |x: Option<&str>| x.and_then(|x| cx.parse(x, |_, el| RunProps::parse(Some(el), colours)));
                assert_eq!(parse(Some(raw)).unwrap_or_default(), parse(again.as_deref()).unwrap_or_default(), "{what}");
            } else {
                let parse = |x: Option<&str>| x.and_then(|x| cx.parse(x, |_, el| ParaProps::parse(Some(el), colours)));
                assert_eq!(parse(Some(raw)).unwrap_or_default(), parse(again.as_deref()).unwrap_or_default(), "{what}");
            }
            checked += 1;
        };
        for p in authored(&d) {
            each(p.1.as_deref(), "pPr");
            each(p.2.as_deref(), "rPr");
            p.3.iter().for_each(|r| each(r.as_deref(), "rPr"));
        }
    }
    assert!(checked > 5_000, "only {checked} property elements checked");
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
        assert!(run.bold && run.underline.is_some(), "the run is bold and keeps its underline: {:?}", run.direct.0);
    };
    holds(e.document());
    let saved = crate::docx::write::save(e.document()).expect("saves");
    holds(&crate::docx::read_editable(&saved).expect("the saved package reads"));
}
