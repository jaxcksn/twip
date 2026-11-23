//! Tagged PDF of a document the editor shows

/// A sub-template's body shown at its reference (an editor's layout of it) is tagged after the reference's
/// paragraph, a division of its own at each reference.
#[test]
fn a_shown_sub_template_body_is_tagged_at_its_reference() {
    use crate::editing::tests::{fonts, open};
    let mut e = open("FX-SUBTEMPLATES.docx");
    e.execute(crate::editing::sub_templates::rules("SUBTEMPLATE", "Sub-templates"), fonts());
    e.complete(fonts());
    assert!(e.shows_expansions(), "the bodies show");
    let (pdf, _) = crate::pdf::export::export(e.document(), &e.snapshot(), "", &crate::api::session::PdfOptions::default(), None).expect("exports");
    let dump = crate::pdf::output::dump(pdf);
    let structure = &dump[dump.find("\nstructure\n").expect("a structure")..];
    // Violation's body shown at two references (whose own text isn't drawn): each a division where its reference's
    // paragraph is, its list with its numbers.
    assert_eq!(structure.matches("\n    Div\n      H1\n      P\n      L ").count(), 2, "{structure}");
    assert!(structure.starts_with("\nstructure\n  Document\n    H1\n    Div\n"), "the first follows the title: {structure}");
    assert_eq!(structure.matches("Lbl").count(), 4, "{structure}");
    assert!(dump.lines().filter(|l| l.trim() == "text Violation notice").count() == 2, "both bodies are drawn");
}

#[test]
fn the_printed_document_leaves_out_shown_hidden_text() {
    let body = r#"<w:p><w:r><w:t xml:space="preserve">Shown </w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t>secret</w:t></w:r><w:r><w:t xml:space="preserve"> end</w:t></w:r></w:p>"#;
    let mut e = crate::editing::Editor::open(&crate::editing::tests::blank_body(body), crate::editing::tests::fonts()).expect("opens");
    e.complete(crate::editing::tests::fonts());
    e.set_markup_view(crate::docx::review::MarkupView::NoMarkup);
    assert!(e.lays_out_as_printed(false));
    e.set_mark_options(crate::editing::view::MarkOptions { hidden: true, optional_hyphens: true, ..crate::editing::view::MarkOptions::default() });
    assert!(!e.lays_out_as_printed(false) && !e.lays_out_as_printed(true));
    let doc = e.printed_document(false);
    assert!(!doc.review.show_hidden && !doc.review.show_optional_hyphens && doc.review.view == crate::docx::review::MarkupView::NoMarkup);
    let snapshot = crate::layout::layout(&doc, crate::editing::tests::fonts());
    let pdf =
        crate::pdf::export::export_with(&doc, &snapshot, "", &Default::default(), None, Some(crate::editing::tests::fonts())).expect("exports").0;
    let text = crate::pdf::output::dump(pdf);
    assert!(text.contains("Shown") && !text.contains("secret"), "{text}");
    e.complete(crate::editing::tests::fonts());
    let shown = crate::pdf::export::export_with(e.document(), &e.snapshot(), "", &Default::default(), None, Some(crate::editing::tests::fonts()))
        .expect("exports")
        .0;
    assert!(crate::pdf::output::dump(shown).contains("secret"), "the editor's own layout has it");
    assert!(e.printed_document(true).review.show_hidden);
}
