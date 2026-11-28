//! Authored formatting: a document built from typed properties with no XML or package, the package generated from it,
//! layout reuse after an edit that changes no effective value, and explicit formatting equal to the inherited value
//! through later edits

use std::io::{Read, Write};
use std::rc::Rc;

use super::fonts;
use crate::api::command::{Command as C, Indents};
use crate::api::styles::{StyleChange, StyleModification};
use crate::api::value::{Alignment, Points};
use crate::docx::char_props::RunDirect;
use crate::docx::char_props::RunXml;
use crate::docx::model::ThemeColourRef;
use crate::docx::numbering::Numbering;
use crate::docx::props::{Jc, ParaProps, RunProps, toggle};
use crate::docx::resolve::Resolver;
use crate::docx::styles::{Styles, ThemeFonts};
use crate::docx::write::GenerateError;
use crate::docx::{Block, BreakKind, Columns, Document, Inline, Paragraph, Run, Section, SectionBreak, next_para_id};
use crate::drawing::colour::SchemeSlot;
use crate::editing::Editor;
use crate::editing::commands::{Command, Outcome};
use crate::format::{CharProps, Color, FontRef, ScriptClass, ScriptFonts, ThemeFace, Toggle};
use crate::layout::{ParaId, Unit};

fn section(blocks: Vec<Block>) -> Section {
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
        chapter: None,
        line_numbers: None,
        v_align: crate::docx::VerticalAlignment::Top,
        page_borders: None,
        page_borders_direct: Default::default(),
        line_grid: None,
        notes: Default::default(),
        no_endnote: false,
        text_flow: 0,
        rtl_gutter: false,
        footnote_columns: 0,
        form_protected: true,
        blocks,
        change: None,
        origin: crate::docx::Origin::NONE,
    }
}

/// A resolver with no styles, numbering or theme part: the application defaults.
fn bare_resolver() -> crate::docx::resolvers::Resolvers {
    let env = crate::docx::resolvers::Surroundings { theme: ThemeFonts::parse(None), ..Default::default() };
    crate::docx::resolvers::Resolvers::new(crate::docx::resolvers::TypedDefs::read(None, None, None, env).0)
}

/// A paragraph from authored properties alone; the XML path's resolver of the styles `defs` writes gives the
/// effective formatting.
fn paragraph(defs: &crate::docx::resolvers::Resolvers, ppr: &ParaProps, runs: &[(&str, RunProps)], id: u64) -> Paragraph {
    let t = defs.typed();
    let (cx, colours) = (t.styles.cx(), defs.colours());
    let root =
        if t.styles.root.is_empty() { format!("<w:styles xmlns:w=\"{}\">", crate::docx::write::props::W_NS) } else { t.styles.root.to_string() };
    let mut xml = root;
    xml.extend(t.styles.defaults.iter().map(|d| d.xml(&colours, &cx).into_owned()));
    xml.extend(t.styles.styles.values().map(|d| d.xml(&colours, &cx).into_owned()));
    xml.push_str(&format!("</{}styles>", cx.w));
    let res = Resolver::new(Styles::parse(Some(&xml), colours), Numbering::default(), t.cascade.theme.clone());
    let p = res.paragraph(ppr, None);
    let inlines = runs.iter().map(|(t, rpr)| Inline::Text((*t).into(), res.run_style(&res.run_props(&p.base, rpr)))).collect();
    Paragraph {
        mark: res.run_style(&p.base),
        style: Rc::new(p.style),
        list: None,
        inlines,
        origin: crate::docx::Origin::NONE,
        id,
        controls: Vec::new(),
        div: None,
    }
}

fn sourceless(paras: Vec<Paragraph>, res: &crate::docx::resolvers::Resolvers) -> Document {
    Document { sections: vec![section(paras.into_iter().map(Block::Para).collect())], resolver: Some(Rc::new(res.clone())), ..Default::default() }
}

fn runs(d: &Document, para: usize) -> Vec<(String, Run)> {
    let Block::Para(p) = &d.sections[0].blocks[para] else { panic!("block {para} is not a paragraph") };
    p.inlines.iter().filter_map(|i| if let Inline::Text(t, r) = i { Some((t.to_string(), r.clone())) } else { None }).collect()
}

fn glyph_lines(text: &str) -> Vec<&str> {
    text.lines().filter(|l| l.starts_with("G ") || l.starts_with("page ")).collect()
}

/// Shaping and line breaking done so far: the misses of the editor's shaping and line caches. (A test build
/// recomputes every cache hit to verify it, so timing counters would count hits too.)
fn layout_work(e: &Editor) -> (u64, u64) {
    let cache = e.layout.cache();
    (cache.shaped.counts().1, cache.lines.counts().1)
}

fn since(e: &Editor, before: (u64, u64)) -> (u64, u64) {
    let now = layout_work(e);
    (now.0 - before.0, now.1 - before.1)
}

#[test]
fn a_document_built_from_typed_properties_lays_out_and_edits_without_xml() {
    let fonts = fonts();
    let res = bare_resolver();
    let centred = ParaProps { jc: Some(Jc::Center), ..Default::default() };
    let bold = RunProps { toggles_set: toggle::B, toggles: toggle::B, size: Some(14.0), ..Default::default() };
    let id = next_para_id();
    let doc = sourceless(vec![paragraph(&res, &centred, &[("Built without ", RunProps::default()), ("XML", bold)], id)], &res);
    assert!(doc.source.is_none(), "no package");
    let (_, run) = &runs(&doc, 0)[1];
    assert!(run.bold && run.size == 14.0 && run.direct.props().is_none(), "typed props resolve with no XML: {run:?}");

    let first = crate::layout::dump::page_text(&crate::layout::layout(&doc, fonts), &doc);
    assert_eq!(glyph_lines(&first).iter().filter(|l| l.starts_with("page ")).count(), 1, "one page:\n{first}");

    // A typed edit: the second run italic at 20pt, explicitly not bold.
    let italic = RunProps { toggles_set: toggle::B | toggle::I, toggles: toggle::I, size: Some(20.0), ..Default::default() };
    let mut edited = doc.clone();
    edited.sections[0].blocks[0] = Block::Para(paragraph(&res, &centred, &[("Built without ", RunProps::default()), ("XML", italic)], id));
    let (_, run) = &runs(&edited, 0)[1];
    assert!(!run.bold && run.italic && run.size == 20.0, "{run:?}");
    let second = crate::layout::dump::page_text(&crate::layout::layout(&edited, fonts), &edited);
    assert_ne!(glyph_lines(&first), glyph_lines(&second), "the edit moves the glyphs");

    // The editor on the sourceless document: typing, a formatting command, incremental against fresh layout, undo.
    let mut e = Editor::with_document(edited, fonts);
    assert!(matches!(e.execute(Command::InsertText("Typed. ".into()), fonts), Outcome::Changed(_)));
    e.execute(Command::SelectAll, fonts);
    assert!(matches!(e.execute(C::SetBold(true), fonts), Outcome::Changed(_)));
    e.complete(fonts);
    assert!(runs(e.document(), 0).iter().all(|(_, r)| r.bold), "bold everywhere");
    let incremental = crate::layout::dump::page_text(&e.snapshot(), e.document());
    let fresh = crate::layout::dump::page_text(&crate::layout::layout(e.document(), fonts), e.document());
    assert_eq!(incremental, fresh, "incremental layout equals fresh layout");
    assert!(matches!(e.execute(Command::Undo(Default::default()), fonts), Outcome::Changed(_)));
    assert!(!runs(e.document(), 0).iter().all(|(_, r)| r.bold), "undone");
}

#[test]
fn an_edit_that_changes_no_effective_value_reuses_shaping_and_lines() {
    let fonts = fonts();
    let res = bare_resolver();
    // Bold sets the complex-script twin too, so the text has both already.
    let both = toggle::B | toggle::B_CS;
    let bold = RunProps { toggles_set: both, toggles: both, ..Default::default() };
    let words = "The quick brown fox jumps over the lazy dog. ".repeat(40);
    // The marks are bold too, so Bold over everything changes no effective value.
    let paras = (0..6)
        .map(|_| {
            let mut p = paragraph(&res, &ParaProps::default(), &[(words.as_str(), bold.clone())], next_para_id());
            let Inline::Text(_, r) = &p.inlines[0] else { unreachable!() };
            p.mark = r.clone();
            p
        })
        .collect();
    let mut e = Editor::with_document(sourceless(paras, &res), fonts);
    e.complete(fonts);
    e.execute(Command::SelectAll, fonts);
    e.complete(fonts);
    let (work, _) = (layout_work(&e), crate::prof::take_reuse());

    assert!(matches!(e.execute(C::SetBold(true), fonts), Outcome::Changed(_)), "an authored change records a step");
    e.complete(fonts);
    assert_eq!(since(&e, work), (0, 0), "no shaping and no line breaking");
    let [paras, _, _] = crate::prof::take_reuse();
    assert_eq!(paras[2], 0, "no paragraph laid out anew: {paras:?}");

    assert!(matches!(e.execute(C::SetItalic(true), fonts), Outcome::Changed(_)));
    e.complete(fonts);
    let (shaped, broken) = since(&e, work);
    assert!(shaped > 0 && broken > 0, "an effective change shapes and breaks again: {shaped}, {broken}");
}

// Explicit formatting equal to the inherited value

/// What carries the explicit formatting: a run, a paragraph mark or the paragraph.
#[derive(Clone, Copy, Debug)]
enum Target {
    Run,
    Mark,
    Paragraph,
}

/// A paragraph in a bold, centred, 14pt style ("Strong"), then one in a plain style. The style sets the
/// complex-script twins too, as the Bold and size commands do.
fn styled_docx() -> Vec<u8> {
    with_strong_style(
        r#"<w:p><w:pPr><w:pStyle w:val="Strong"/></w:pPr><w:r><w:t>Total</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Plain"/></w:pPr><w:r><w:t>Other</w:t></w:r></w:p>"#,
    )
}

/// `body` in a package that has the "Strong" and "Plain" paragraph styles.
fn with_strong_style(body: &str) -> Vec<u8> {
    let styles = r#"<w:style w:type="paragraph" w:styleId="Strong"><w:name w:val="Strong Paragraph"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Plain"><w:name w:val="Plain Paragraph"/><w:basedOn w:val="Normal"/></w:style>"#;
    let src = crate::docx::blank::blank_docx();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(src)).expect("the blank package");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("an entry");
        let name = f.name().expect("a name").to_string();
        let mut data = String::new();
        f.read_to_string(&mut data).expect("text");
        match name.as_str() {
            "word/document.xml" => data = data.replacen("<w:p/>", body, 1),
            "word/styles.xml" => data = data.replacen("</w:styles>", &format!("{styles}</w:styles>"), 1),
            _ => {}
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).expect("writes");
        out.write_all(data.as_bytes()).expect("writes");
    }
    out.finish().expect("writes").into_inner()
}

fn para(d: &Document, n: usize) -> &Paragraph {
    d.sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None }).nth(n).expect("a paragraph")
}

fn first_run(p: &Paragraph) -> &Run {
    p.inlines.iter().find_map(|i| i.run()).expect("a run")
}

/// A run's `w:rPr` as saving writes it, in a part that binds the main namespace to `w`.
fn written(r: &crate::docx::RunStyle) -> String {
    let cx = crate::docx::write::props::Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
    r.direct.xml(&Default::default(), &cx).map(std::borrow::Cow::into_owned).unwrap_or_default()
}

/// A paragraph's `w:pPr` as saving writes it, in a part that binds the main namespace to `w`.
fn direct(d: &crate::docx::para_props::ParaDirect) -> String {
    let cx = crate::docx::write::props::Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
    d.xml(&Default::default(), &cx).map(std::borrow::Cow::into_owned).unwrap_or_default()
}

/// The target's authored XML.
fn authored(p: &Paragraph, t: Target) -> String {
    match t {
        Target::Run => written(first_run(p)),
        Target::Mark => written(&p.mark),
        Target::Paragraph => direct(&p.style.direct),
    }
}

/// Whether the target has the style's effective value (bold, or centred).
fn inherited_value(p: &Paragraph, t: Target) -> bool {
    match t {
        Target::Run => first_run(p).bold,
        Target::Mark => p.mark.bold,
        Target::Paragraph => p.style.jc == Jc::Center,
    }
}

/// Sets the style's value explicitly on the target: Bold for a run or mark, centring for a paragraph.
fn set_explicitly(t: Target) -> C {
    match t {
        Target::Run | Target::Mark => C::SetBold(true),
        Target::Paragraph => C::SetAlignment(Alignment::Center),
    }
}

fn explicit_marker(t: Target) -> &'static str {
    match t {
        Target::Run | Target::Mark => "<w:b/>",
        Target::Paragraph => r#"<w:jc w:val="center"/>"#,
    }
}

/// Takes the value out of the "Strong" style.
fn change_base(t: Target, formatting: C) -> C {
    let _ = t;
    C::ModifyStyle(StyleModification {
        id: crate::api::value::StyleId("Strong".into()),
        change: StyleChange { formatting: vec![formatting], ..Default::default() },
    })
}

fn base_without_value(t: Target) -> C {
    change_base(t, if matches!(t, Target::Paragraph) { C::SetAlignment(Alignment::Left) } else { C::SetBold(false) })
}

/// The first paragraph's text and mark.
fn select_first(e: &mut Editor) {
    let (a, b) = (ParaId(para(e.document(), 0).id), ParaId(para(e.document(), 1).id));
    e.select_between((a, Unit(0)), (b, Unit(0)), fonts());
}

fn reread(e: &Editor) -> Document {
    let bytes = crate::docx::write::save(e.document()).expect("saves");
    crate::docx::read_editable(&bytes).expect("reads back")
}

fn state(e: &Editor, t: Target) -> (String, bool) {
    let p = para(e.document(), 0);
    (authored(p, t), inherited_value(p, t))
}

/// Text inherits the style's value; the value is set explicitly; the style loses it. The explicit value holds, a
/// control paragraph without it follows the style, and undo returns each state.
fn inherited_then_explicit_then_base_change(t: Target) {
    let fonts = fonts();
    let mut e = Editor::open(&styled_docx(), fonts).expect("opens");
    e.complete(fonts);
    select_first(&mut e);
    let original = para(e.document(), 0).clone();
    assert!(inherited_value(&original, t), "{t:?} inherits the style's value");
    assert!(!authored(&original, t).contains(explicit_marker(t)), "nothing direct");
    let before = state(&e, t);
    let work = layout_work(&e);

    let out = e.execute(set_explicitly(t), fonts);
    e.complete(fonts);
    assert!(matches!(out, Outcome::Changed(_)), "a step is recorded for {t:?}: {out:?}");
    assert!(authored(para(e.document(), 0), t).contains(explicit_marker(t)), "the explicit value is in the authored state of the {t:?}");
    assert!(inherited_value(para(e.document(), 0), t), "the effective value is unchanged");
    assert_eq!(since(&e, work), (0, 0), "no shaping and no line breaking");
    assert!(authored(para(&reread(&e), 0), t).contains(explicit_marker(t)), "the save holds the explicit value");
    let explicit = state(&e, t);

    assert!(matches!(e.execute(base_without_value(t), fonts), Outcome::Changed(_)), "the style changes");
    e.complete(fonts);
    assert!(inherited_value(para(e.document(), 0), t), "the explicit value holds when the style loses it");
    assert!(authored(para(&reread(&e), 0), t).contains(explicit_marker(t)), "the save still holds the explicit value");
    let mut control = Editor::open(&styled_docx(), fonts).expect("opens");
    control.execute(base_without_value(t), fonts);
    assert!(!inherited_value(para(control.document(), 0), t), "a control paragraph follows the style");

    e.execute(Command::Undo(Default::default()), fonts);
    assert_eq!(state(&e, t), explicit, "undo returns the explicit state");
    e.execute(Command::Undo(Default::default()), fonts);
    assert_eq!(state(&e, t), before, "undo returns the inherited state");
    let back = para(e.document(), 0);
    let same = match t {
        Target::Run => Rc::ptr_eq(first_run(back), first_run(&original)),
        Target::Mark => Rc::ptr_eq(&back.mark, &original.mark),
        Target::Paragraph => Rc::ptr_eq(&back.style, &original.style),
    };
    assert!(same, "the original format object returns");
}

#[test]
fn explicit_bold_on_a_run_holds_when_its_style_loses_bold() {
    inherited_then_explicit_then_base_change(Target::Run);
}

#[test]
fn explicit_bold_on_a_paragraph_mark_holds_when_its_style_loses_bold() {
    inherited_then_explicit_then_base_change(Target::Mark);
}

#[test]
fn explicit_centring_holds_when_the_paragraph_style_loses_it() {
    inherited_then_explicit_then_base_change(Target::Paragraph);
}

// Stale formats in the layout cache

/// Sets the style's inherited 14pt size (or centring) explicitly: an authored change that layout does not see.
fn set_inherited(t: Target) -> C {
    match t {
        Target::Run | Target::Mark => C::SetFontSize(Points(14.0)),
        Target::Paragraph => C::SetAlignment(Alignment::Center),
    }
}

fn inherited_marker(t: Target) -> &'static str {
    match t {
        Target::Run | Target::Mark => r#"<w:sz w:val="28"/>"#,
        Target::Paragraph => r#"<w:jc w:val="center"/>"#,
    }
}

fn second_command(t: Target) -> C {
    match t {
        Target::Run | Target::Mark => C::SetItalic(true),
        Target::Paragraph => C::SetIndents(Indents { left: Some(Points(36.0)), ..Default::default() }),
    }
}

fn second_marker(t: Target) -> &'static str {
    match t {
        Target::Run | Target::Mark => "<w:i/>",
        Target::Paragraph => r#"w:left="720""#,
    }
}

fn holds(xml: &str, markers: &[&str]) -> bool {
    markers.iter().all(|m| xml.contains(m))
}

/// After an authored-only edit, layout keeps cached shaping that holds the format from before it. A second command,
/// a paste, a save, a change of the base and each undo must all work from the document's current authored state.
fn stale_formats_in_the_cache(t: Target) {
    let fonts = fonts();
    let mut e = Editor::open(&styled_docx(), fonts).expect("opens");
    e.complete(fonts);
    select_first(&mut e);
    e.complete(fonts);
    let saved_before = crate::docx::write::save(e.document()).expect("saves");
    let work = layout_work(&e);

    let out = e.execute(set_inherited(t), fonts);
    e.complete(fonts);
    assert!(matches!(out, Outcome::Changed(_)), "a step is recorded for {t:?}: {out:?}");
    assert!(authored(para(e.document(), 0), t).contains(inherited_marker(t)), "the explicit value is in the authored state of the {t:?}");
    assert_eq!(since(&e, work), (0, 0), "no shaping and no line breaking");

    select_first(&mut e);
    assert!(matches!(e.execute(second_command(t), fonts), Outcome::Changed(_)));
    e.complete(fonts);
    let both = [inherited_marker(t), second_marker(t)];
    assert!(holds(&authored(para(e.document(), 0), t), &both), "both properties: {:?}", authored(para(e.document(), 0), t));

    // Copy the first paragraph's text (and mark) to the start of the plain one.
    select_first(&mut e);
    if matches!(t, Target::Run) {
        let a = ParaId(para(e.document(), 0).id);
        e.select_range(a, Unit(0), Unit(5), fonts);
    }
    e.execute(Command::Copy, fonts);
    let b = ParaId(para(e.document(), 1).id);
    e.select_range(b, Unit(0), Unit(0), fonts);
    assert!(matches!(e.execute(Command::Paste(Default::default()), fonts), Outcome::Changed(_)), "pasted");
    e.complete(fonts);
    assert!(holds(&authored(para(e.document(), 1), t), &both), "the pasted copy keeps both: {:?}", authored(para(e.document(), 1), t));

    let back = reread(&e);
    assert!(holds(&authored(para(&back, 0), t), &both), "the saved file holds both");

    let base = match t {
        Target::Run | Target::Mark => change_base(t, C::SetFontSize(Points(20.0))),
        Target::Paragraph => change_base(t, C::SetAlignment(Alignment::Left)),
    };
    assert!(matches!(e.execute(base, fonts), Outcome::Changed(_)));
    e.complete(fonts);
    let p = para(e.document(), 0);
    let kept = match t {
        Target::Run => first_run(p).size == 14.0,
        Target::Mark => p.mark.size == 14.0,
        Target::Paragraph => p.style.jc == Jc::Center,
    };
    assert!(kept, "the explicit value holds when the base changes");

    // Undo the base change, the paste, the second command and the explicit value, saving after each.
    for (n, expect) in [(1, &both[..]), (2, &both[..]), (3, &both[..1])] {
        e.execute(Command::Undo(Default::default()), fonts);
        assert!(holds(&authored(para(e.document(), 0), t), expect), "undo {n}: {:?}", authored(para(e.document(), 0), t));
        assert!(holds(&authored(para(&reread(&e), 0), t), expect), "undo {n}, saved");
    }
    assert!(!authored(para(e.document(), 0), t).contains(second_marker(t)), "the second command is undone");
    e.execute(Command::Undo(Default::default()), fonts);
    assert!(!authored(para(e.document(), 0), t).contains(inherited_marker(t)), "the explicit value is undone");
    assert!(crate::docx::write::save(e.document()).expect("saves") == saved_before, "the saved bytes equal those before the edits");
}

#[test]
fn a_run_edit_reads_the_documents_format_not_the_cached_one() {
    stale_formats_in_the_cache(Target::Run);
}

#[test]
fn a_mark_edit_reads_the_documents_format_not_the_cached_one() {
    stale_formats_in_the_cache(Target::Mark);
}

#[test]
fn a_paragraph_edit_reads_the_documents_format_not_the_cached_one() {
    stale_formats_in_the_cache(Target::Paragraph);
}

// Joining text into one run

/// "te" bold directly and "h" bold through its paragraph style: typing a space corrects "teh" to "the", and each
/// character keeps the authored formatting of the one it replaces, though all three look the same.
#[test]
fn an_autocorrection_keeps_each_characters_authored_formatting() {
    use crate::editing::autocorrect::{AutoCorrect, Trigger};
    let body = r#"<w:p><w:pPr><w:pStyle w:val="Strong"/></w:pPr><w:r><w:rPr><w:b/><w:bCs/></w:rPr><w:t>te</w:t></w:r><w:r><w:t>h</w:t></w:r></w:p>"#;
    let mut e = Editor::open(&with_strong_style(body), fonts()).expect("opens");
    let p = ParaId(para(e.document(), 0).id);
    e.select_range(p, Unit(3), Unit(3), fonts());
    e.execute(Command::InsertText(" ".into()), fonts());
    assert!(
        matches!(e.auto_correct(&AutoCorrect { sentence_capitals: false, ..AutoCorrect::default() }, Trigger::Typed, fonts()), Outcome::Changed(_)),
        "corrected"
    );
    let runs: Vec<(String, String)> = para(e.document(), 0)
        .inlines
        .iter()
        .filter_map(|i| if let Inline::Text(t, r) = i { Some((t.to_string(), written(r))) } else { None })
        .collect();
    assert_eq!(runs.iter().map(|r| r.0.as_str()).collect::<String>(), "the ");
    let last_e = runs.iter().find(|(t, _)| t.contains('e')).expect("the e");
    assert!(!last_e.1.contains("<w:b/>"), "the e takes the plain h's formatting, not the direct bold: {runs:?}");
}

// A package generated from the model alone

/// A paragraph's content, as the editor holds it.
enum Piece {
    Text(&'static str),
    Tab,
    Break(BreakKind),
}

/// A paragraph in paragraph style `style` whose runs carry typed authored properties alone, resolved over its base.
fn typed(res: &crate::docx::resolvers::Resolvers, style: Option<&str>, pieces: Vec<(Piece, CharProps)>) -> Paragraph {
    typed_para(res, crate::format::ParaProps { style: style.map(Into::into), ..Default::default() }, pieces)
}

/// A paragraph with typed authored properties `props` whose runs carry typed authored properties alone.
fn typed_para(res: &crate::docx::resolvers::Resolvers, props: crate::format::ParaProps, pieces: Vec<(Piece, CharProps)>) -> Paragraph {
    let p = crate::docx::para_xml::with(res, crate::docx::para_props::ParaDirect::typed(Rc::new(props)), None, None);
    let run = |props: CharProps| {
        let own = (props != CharProps::default()).then(|| Rc::new(props));
        crate::docx::run_xml::run_of(res, &p.base, RunDirect::typed(own))
    };
    let inlines = pieces
        .into_iter()
        .map(|(piece, props)| match piece {
            Piece::Text(t) => Inline::Text(t.into(), run(props)),
            Piece::Tab => Inline::Tab(run(props)),
            Piece::Break(k) => Inline::Break(k, run(props)),
        })
        .collect();
    Paragraph {
        mark: crate::docx::run_xml::plain(res, &p.base),
        style: Rc::new(p.style),
        list: None,
        inlines,
        origin: crate::docx::Origin::NONE,
        id: next_para_id(),
        controls: Vec::new(),
        div: None,
    }
}

fn on(t: Toggle) -> CharProps {
    let mut p = CharProps::default();
    p.toggles.put(t, true);
    p
}

/// A list definition: decimal numbers at its first level.
const NUMBERING: &str = r#"<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#;

/// Styled text built from typed properties, in a new document's styles (with "Strong"), a list definition and
/// settings: bold, italic, a size, an RGB colour, a theme colour and a theme font, tabs, a line break and a page break,
/// direct paragraph formatting of every kind the paragraph commands set, and two list items.
fn built() -> Document {
    use crate::document::format::{Leader, LineRule, ShadingPattern, TabKind};
    use crate::document::line::BorderStyle;
    use crate::document::paragraph::ListId;
    use crate::format::{ListChoice, ParaBorders, ParaFlag, ParaProps as Typed, Shading, TabStop, TextBorder};
    let new = crate::docx::read(&with_strong_style("<w:p/>")).expect("reads");
    let mut res = (**new.resolver.as_ref().expect("styles")).clone();
    res.set_numbering_part(NUMBERING);
    let res = Rc::new(res);
    let mut flags = crate::format::ParaFlags::default();
    flags.put(ParaFlag::KEEP_NEXT, true);
    flags.put(ParaFlag::WIDOW_CONTROL, false);
    let line = TextBorder { style: BorderStyle::Single, width: 0.5, space: 1.0, shadow: false, color: Some(Color::Auto) };
    let formatted = Typed {
        align: Some(crate::document::format::Jc::Right),
        indent_left: Some(36.0),
        indent_right: Some(18.0),
        indent_first: Some(-18.0),
        space_before: Some(6.0),
        space_after: Some(12.0),
        line: Some(360.0),
        line_rule: Some(LineRule::Auto),
        flags,
        tabs: vec![TabStop { pos: 144.0, kind: TabKind::Center, leader: Leader::Dot }],
        borders: ParaBorders { bottom: Some(line), ..ParaBorders::default() },
        shading: Some(Shading { pattern: ShadingPattern::Clear, color: Some(Color::Auto), fill: Some(Color::Rgb(0xEE_EEEE)) }),
        outline_level: Some(1),
        ..Typed::default()
    };
    let item = |text| {
        let props = Typed { list: Some(ListChoice::In(ListId(1))), list_level: Some(0), ..Typed::default() };
        typed_para(&res, props, vec![(Piece::Text(text), CharProps::default())])
    };
    let accent = Color::Theme(ThemeColourRef { slot: SchemeSlot::Accent(1), tint: None, shade: None });
    let heading = FontRef::Theme { face: ThemeFace::Major, script: ScriptClass::Latin };
    let paras = vec![
        typed(&res, Some("Strong"), vec![(Piece::Text("A styled heading"), CharProps::default())]),
        typed(
            &res,
            None,
            vec![
                (Piece::Text("Bold"), on(Toggle::BOLD)),
                (Piece::Tab, CharProps::default()),
                (Piece::Text("italic at 16pt"), CharProps { size: Some(16.0), ..on(Toggle::ITALIC) }),
                (Piece::Break(BreakKind::Line), CharProps::default()),
                (Piece::Text("in red, "), CharProps { color: Some(Color::Rgb(0xC0_0000)), ..Default::default() }),
                (Piece::Text("in the accent colour"), CharProps { color: Some(accent), ..Default::default() }),
                (Piece::Tab, CharProps::default()),
                (
                    Piece::Text("in the heading font"),
                    CharProps { fonts: ScriptFonts { latin: Some(heading), ..Default::default() }, ..Default::default() },
                ),
                (Piece::Break(BreakKind::Page), CharProps::default()),
                (Piece::Text("After the page break"), CharProps::default()),
            ],
        ),
        typed_para(&res, formatted, vec![(Piece::Text("Formatted"), CharProps::default()), (Piece::Tab, CharProps::default())]),
        item("First item"),
        item("Second item"),
    ];
    Document {
        sections: vec![section(paras.into_iter().map(Block::Para).collect())],
        resolver: Some(res),
        settings: new.settings,
        ..Default::default()
    }
}

/// An editor on `doc` with its list labels derived, as reading derives them.
fn labelled(mut doc: Document, fonts: &crate::font::Fonts) -> Editor {
    let labels = Editor::with_document(doc.clone(), fonts).labels_changed(|_| true);
    for b in &mut doc.sections[0].blocks {
        if let Block::Para(p) = b
            && let Some((_, label)) = labels.iter().find(|(id, _)| id.0 == p.id)
        {
            p.list = label.clone();
        }
    }
    Editor::with_document(doc, fonts)
}

/// The document dump, then every laid-out page.
fn dump(e: &mut Editor) -> String {
    let v = crate::editing::inspect::inspect(e, "dump", true);
    let pages = crate::layout::dump::page_text(&crate::layout::layout(e.document(), fonts()), e.document());
    format!("{}\n{pages}", serde_json::to_string_pretty(&v["dump"]).expect("a dump"))
}

/// Generates `e`'s document, reads it back and compares the dumps, and each run's authored properties (but what the
/// reader keeps of the written form, such as a theme colour's RGB value) and resolved formatting, which show what the
/// dumps don't (theme colours and fonts). The runs read back.
fn reads_back_as_built(e: &mut Editor) -> Vec<Run> {
    let bytes = crate::docx::write::generate(e.document()).expect("generates");
    let mut back = Editor::open(&bytes, fonts()).expect("reads back");
    assert_eq!(dump(e), dump(&mut back));
    let runs = |d: &Document| -> Vec<Run> {
        let paras = d.sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
        paras.flat_map(|p| p.inlines.iter().filter_map(Inline::run).chain([&p.mark]).cloned()).collect()
    };
    let authored = |r: &Run| r.direct.props().map(|p| CharProps { foreign: None, ..(**p).clone() });
    let (built, read) = (runs(e.document()), runs(back.document()));
    assert_eq!(built.len(), read.len(), "as many runs");
    for (a, b) in built.iter().zip(&read) {
        assert!(authored(a) == authored(b) && a.layout_eq(b), "{a:?}\nreads back as\n{b:?}");
    }
    let paras = |d: &Document| -> Vec<Paragraph> {
        d.sections[0].blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p.clone()) } else { None }).collect()
    };
    let (built, back) = (paras(e.document()), paras(back.document()));
    assert_eq!(built.len(), back.len(), "as many paragraphs");
    for (a, b) in built.iter().zip(&back) {
        let mut own = (**a.style.direct.get()).clone();
        if own.style.is_none() {
            own.style = a.style.style_id.as_deref().filter(|id| *id != "Normal").map(Into::into);
        }
        let read = crate::format::ParaProps { foreign: None, ..(**b.style.direct.get()).clone() };
        assert!(own == read && a.style.layout_eq(&b.style), "{:?}\nreads back as\n{:?}", a.style, b.style);
        assert_eq!(a.list.as_ref().map(|l| &l.text), b.list.as_ref().map(|l| &l.text), "the list label");
    }
    read
}

/// The built document, generated: the schema check validates it.
pub(crate) fn generated_package() -> Vec<u8> {
    crate::docx::write::generate(&built()).expect("generates")
}

#[test]
fn a_document_built_from_typed_properties_generates_a_package_that_reads_back_the_same() {
    let fonts = fonts();
    let doc = built();
    assert!(doc.source.is_none(), "no package");
    let mut e = labelled(doc, fonts);
    e.complete(fonts);
    let read = reads_back_as_built(&mut e);
    assert!(read.iter().any(|r| r.color_theme.is_some()) && read.iter().any(|r| r.font_theme.is_some()), "a theme colour and a theme font");

    let p = ParaId(para(e.document(), 1).id);
    e.select_range(p, Unit(2), Unit(8), fonts);
    assert!(matches!(e.execute(C::SetUnderline(crate::api::value::UnderlineStyle::Single), fonts), Outcome::Changed(_)));
    e.complete(fonts);
    assert!(para(e.document(), 1).inlines.iter().filter_map(Inline::run).any(|r| r.underline.is_some()), "underlined");
    reads_back_as_built(&mut e);
}

/// A paragraph command on a paragraph built from typed properties, through the editor: what it doesn't set stays,
/// with no XML behind any of it, and the generated package holds both.
#[test]
fn typed_paragraph_formatting_with_no_xml_survives_a_paragraph_command() {
    let fonts = fonts();
    let res = built().resolver.expect("styles");
    let indented = crate::format::ParaProps { indent_left: Some(36.0), ..Default::default() };
    let p = typed_para(&res, indented, vec![(Piece::Text("Indented"), CharProps::default())]);
    let doc = Document { sections: vec![section(vec![Block::Para(p)])], ..built() };
    let mut e = labelled(doc, fonts);
    e.complete(fonts);
    let id = ParaId(para(e.document(), 0).id);
    e.select_range(id, Unit(0), Unit(3), fonts);
    assert!(matches!(e.execute(C::SetAlignment(Alignment::Center), fonts), Outcome::Changed(_)), "centred");
    let p = para(e.document(), 0);
    assert_eq!((p.style.ind_left, p.style.jc), (36.0, Jc::Center), "both show");
    let own = p.style.direct.get();
    assert_eq!((own.indent_left, own.align), (Some(36.0), Some(Jc::Center)), "both are authored");
    reads_back_as_built(&mut e);
}

/// The first table in `d`'s body.
fn first_table(d: &Document) -> &crate::docx::Table {
    d.sections[0].blocks.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).expect("a table")
}

/// A table's fields as laid out: every one but the identity of what it was read from and of its paragraphs.
fn bare(t: &crate::docx::Table) -> crate::docx::Table {
    use crate::docx::{Cell, Row, Table};
    let rows = t.rows.iter().map(|r| Row {
        origin: crate::docx::Origin::NONE,
        cells: r.cells.iter().map(|c| Cell { blocks: Vec::new(), ..c.clone() }).collect(),
        ..r.clone()
    });
    Table { origin: crate::docx::Origin::NONE, id: 0, rows: rows.collect(), ..t.clone() }
}

/// A table built through the table commands, with no XML behind it (a header row, row heights, cell widths and a
/// merged cell, cell borders and shading, the table's width, indent and cell margins, a row's exceptions), on a page
/// set up through the page commands (landscape, margins, columns, page borders, line numbers): the generated package
/// reads back with the same tables, page and layout.
#[test]
fn a_built_table_and_page_setup_generate_a_package_that_reads_back_the_same() {
    use serde_json::json;
    let fonts = fonts();
    let mut doc = built();
    // The line numbers' run, as reading derives it
    doc.line_number_run = doc.resolver.as_deref().map(crate::docx::line_number_run);
    let mut e = labelled(doc, fonts);
    e.complete(fonts);
    let run = |e: &mut Editor, v: serde_json::Value| {
        let c: Command = serde_json::from_value(v.clone()).expect("a command");
        assert!(matches!(e.execute(c, fonts), Outcome::Changed(_)), "{v}");
    };
    let last = para(e.document(), 4);
    let (id, end) = (ParaId(last.id), Unit(last.inlines.iter().map(|i| i.units()).sum::<usize>() as u32));
    e.select_range(id, end, end, fonts);
    run(&mut e, json!({ "type": "insertTable", "rows": 3, "columns": 3 }));
    let cell_para = |e: &Editor, r: usize, c: usize| {
        let Block::Para(p) = &first_table(e.document()).rows[r].cells[c].blocks[0] else { panic!("a paragraph") };
        ParaId(p.id)
    };
    let at = |e: &mut Editor, r: usize, c: usize| {
        let p = cell_para(e, r, c);
        e.select_range(p, Unit(0), Unit(0), fonts);
    };
    at(&mut e, 0, 0);
    run(&mut e, json!({ "type": "insertText", "value": "Head" }));
    run(&mut e, json!({ "type": "setHeaderRow", "value": true }));
    run(&mut e, json!({ "type": "setRowHeight", "value": { "height": 30.0, "rule": "exact" } }));
    run(&mut e, json!({ "type": "setCellShading", "value": { "rgb": "#DDEBF7" } }));
    run(&mut e, json!({ "type": "setCellBorders", "edges": "outside", "border": { "color": "auto", "style": "double", "width": 1.5 } }));
    run(&mut e, json!({ "type": "setCellProperties", "width": { "kind": "points", "value": 120.0 } }));
    run(&mut e, json!({ "type": "setTableWidth", "kind": "points", "value": 400.0 }));
    run(&mut e, json!({ "type": "setTableIndent", "value": 18.0 }));
    run(&mut e, json!({ "type": "setTableCellMargins", "left": 6.0, "right": 6.0 }));
    at(&mut e, 2, 0);
    run(&mut e, json!({ "type": "setRowProperties", "exceptions": { "alignment": "center", "cellSpacing": 1.5 } }));
    let (a, b) = (cell_para(&e, 1, 0), cell_para(&e, 1, 1));
    e.select_between((a, Unit(0)), (b, Unit(0)), fonts);
    run(&mut e, json!({ "type": "mergeCells" }));
    run(&mut e, json!({ "type": "setOrientation", "value": "landscape" }));
    run(&mut e, json!({ "type": "setMargins", "top": 54.0, "bottom": 54.0, "left": 63.0, "right": 63.0 }));
    run(&mut e, json!({ "type": "setColumns", "count": 2, "spacing": 24.0 }));
    run(&mut e, json!({ "type": "setPageBorders", "top": { "color": { "rgb": "#7030A0" }, "space": 24.0, "style": "single", "width": 0.75 } }));
    run(&mut e, json!({ "type": "setLineNumbering", "value": { "countBy": 5, "start": 1 } }));
    e.complete(fonts);
    let t = first_table(e.document());
    assert!(t.rows[0].header && t.rows[0].height.is_some() && t.rows[1].cells.len() == 2 && t.rows[2].cell_spacing.is_some(), "{t:?}");
    let s = &e.document().sections[0];
    assert!(s.page_w > s.page_h && s.columns.num == 2 && s.page_borders.is_some() && s.line_numbers.is_some(), "{:?}", s.page());

    let bytes = crate::docx::write::generate(e.document()).expect("generates");
    let mut again = Editor::open(&bytes, fonts).expect("reads back");
    assert_eq!(dump(&mut e), dump(&mut again), "the same document, laid out alike");
    let back = again.document();
    assert_eq!(bare(first_table(back)), bare(first_table(e.document())), "the table reads back");
    let (was, now) = (&e.document().sections[0], &back.sections[0]);
    let typed = |s: &crate::docx::Section| {
        let mut p = s.page();
        p.page_borders_direct = crate::docx::page_borders::PageBordersDirect::typed(p.page_borders_direct.get().map(|b| (**b).clone()));
        p
    };
    assert_eq!(typed(now), typed(was), "the page reads back");
    assert_eq!((now.line_grid, now.rtl_gutter), (was.line_grid, was.rtl_gutter));
}

#[test]
fn generating_names_what_it_cannot_write() {
    let unsupported = |d: &Document| crate::docx::write::generate(d).expect_err("not generated");
    let table = r#"<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:ins w:id="1" w:author="A"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>"#;
    let mut doc = crate::docx::read(&with_strong_style(table)).expect("reads");
    doc.source = None;
    assert_eq!(unsupported(&doc), GenerateError::Unsupported("a tracked change"));

    let mut doc = built();
    doc.sections[0].headers[0] = Some(crate::docx::HeaderFooterId::next());
    assert_eq!(unsupported(&doc), GenerateError::Unsupported("a header or footer"));

    // A picture bullet's image isn't written
    let mut doc = built();
    let mut res = (**doc.resolver.as_ref().expect("styles")).clone();
    let xml = crate::docx::list_xml::new_picture_xml(0, "rId99", &res.typed().numbering.cx());
    res.add_picture(0, xml, "rId99".into(), crate::docx::ResourceId(0));
    doc.resolver = Some(Rc::new(res));
    assert_eq!(unsupported(&doc), GenerateError::Unsupported("a picture bullet"));

    // Formatting with nothing authored behind it: a centred paragraph, a bold run.
    let res = doc.resolver.clone().expect("styles");
    let centred = paragraph(&res, &ParaProps { jc: Some(Jc::Center), ..Default::default() }, &[("centred", RunProps::default())], next_para_id());
    let doc = Document { sections: vec![section(vec![Block::Para(centred)])], ..built() };
    assert_eq!(unsupported(&doc), GenerateError::Unsupported("paragraph formatting that is not its own"));
    let bold = RunProps { toggles_set: toggle::B, toggles: toggle::B, ..Default::default() };
    let bold = paragraph(&res, &ParaProps::default(), &[("bold", bold)], next_para_id());
    let doc = Document { sections: vec![section(vec![Block::Para(bold)])], ..built() };
    assert_eq!(unsupported(&doc), GenerateError::Unsupported("character formatting that is not its own"));
}

#[test]
fn a_document_with_no_styles_generates_the_application_defaults() {
    let fonts = fonts();
    let new = built();
    let mut res = bare_resolver();
    res.set_rules(new.resolver.as_ref().expect("styles").cascade().rules);
    let mut doc = sourceless(vec![typed(&res, None, vec![(Piece::Text("Bold at the defaults"), on(Toggle::BOLD))])], &res);
    doc.settings = new.settings;
    let mut e = Editor::with_document(doc, fonts);
    e.complete(fonts);
    reads_back_as_built(&mut e);
}
