//! A warm cache stays correct when a setting or the fonts change.

use std::path::Path;

use crate::docx::Document;
use crate::layout::cache::LayoutCache;
use crate::layout::ids::Revision;
use crate::layout::{DocumentLayout, dump, layout, layout_with};

use super::{doc, fixture, fonts};

fn toggle(d: &mut Document, name: &str) {
    let c = &mut d.settings.compat;
    if !c.remove(name) {
        c.insert(name.to_string());
    }
}

/// A named change to a document.
type Sweep = (&'static str, fn(&mut Document));

/// One change per `TextEnv` and `LayoutEnv` field.
fn sweeps() -> Vec<Sweep> {
    vec![
        ("compat mode", |d| d.settings.compat_mode = if d.settings.compat_mode >= 15 { 14 } else { 15 }),
        ("default tab", |d| d.settings.default_tab += 18.0),
        ("doNotExpandShiftReturn", |d| toggle(d, "doNotExpandShiftReturn")),
        ("splitPgBreakAndParaMark", |d| toggle(d, "splitPgBreakAndParaMark")),
        ("allowSpaceOfSameStyleInTable", |d| toggle(d, "allowSpaceOfSameStyleInTable")),
        ("doNotUseHTMLParagraphAutoSpacing", |d| toggle(d, "doNotUseHTMLParagraphAutoSpacing")),
        ("even and odd headers", |d| d.settings.even_and_odd = !d.settings.even_and_odd),
        ("mirror margins", |d| {
            d.settings.mirror_margins = !d.settings.mirror_margins;
            d.sections.iter_mut().for_each(|s| s.gutter = 36.0);
        }),
        ("gutter at top", |d| {
            d.settings.gutter_at_top = !d.settings.gutter_at_top;
            d.sections.iter_mut().for_each(|s| s.gutter = 36.0);
        }),
        ("line number run", |d| {
            let run =
                d.sections.iter().flat_map(|s| &s.blocks).find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.mark.clone()) } else { None });
            d.line_number_run = run.map(|r| std::rc::Rc::new(crate::docx::model::RunStyle { size: r.size + 3.0, ..(*r).clone() }));
            d.sections.iter_mut().for_each(|s| {
                s.line_numbers = Some(crate::docx::model::LineNumbering { count_by: 1, start: 1, distance: 18.0, restart: "newPage".into() })
            });
        }),
    ]
}

fn documents() -> Vec<Document> {
    [
        doc(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tables.docx"))),
        fixture(r"HF9\HF9_g_evenodd.docx"),
        fixture(r"K1\K1b_keepnext.docx"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[test]
fn settings_sweeps_with_a_warm_cache() {
    let fonts = fonts();
    for d in documents() {
        for (name, change) in sweeps() {
            let mut cache = LayoutCache::new();
            layout_with(&d, &fonts, &mut cache);
            let mut changed = d.clone();
            change(&mut changed);
            let want = dump::page_text(&layout(&changed, &fonts), &changed);
            assert_eq!(dump::page_text(&layout_with(&changed, &fonts, &mut cache), &changed), want, "{name}: warm cache");
            // And through DocumentLayout, whose baseline was laid out with the old setting.
            let mut dl = DocumentLayout::new();
            dl.edit(Revision(1), None);
            dl.complete(&d, &fonts);
            dl.edit(Revision(2), None);
            dl.complete(&changed, &fonts);
            assert_eq!(dump::page_text(&dl.snapshot(), &changed), want, "{name}: DocumentLayout");
        }
    }
}

#[test]
fn added_font_with_a_warm_cache() {
    let Some(d) = documents().into_iter().next() else { return };
    let mut fonts = fonts();
    let mut cache = LayoutCache::new();
    layout_with(&d, &fonts, &mut cache);
    let before = fonts.revision();
    // Any font file this machine has that the catalog lacks.
    let candidates = [r"C:\Windows\Fonts\consola.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"];
    let Some(extra) = candidates.into_iter().find(|p| Path::new(p).exists()) else { return };
    fonts.add_file(extra);
    assert!(fonts.revision() > before, "adding a face bumps the revision");
    assert_eq!(dump::page_text(&layout_with(&d, &fonts, &mut cache), &d), dump::page_text(&layout(&d, &fonts), &d));
}
