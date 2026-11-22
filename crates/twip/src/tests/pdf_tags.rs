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
