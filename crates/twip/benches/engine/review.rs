//! Review on `FX-REVIEW-LARGE.docx` (many revisions and comments): open, the markup views, accept and reject
//! everything, each with the whole layout it leaves (background layout drained). Budget: 2 s each.

use std::cell::RefCell;

use divan::Bencher;
use twip::internal::docx::review::MarkupView;
use twip::internal::editing::Editor;
use twip::internal::editing::commands::Command;
use twip::internal::font::Fonts;

const DOC: &str = "editor/review/FX-REVIEW-LARGE.docx";

fn settled(mut e: Editor, fonts: &Fonts) -> Editor {
    while e.idle(fonts) {}
    e
}

#[divan::bench(sample_count = 10)]
fn open(bencher: Bencher) {
    let (fonts, bytes) = (Fonts::system(), super::fixture(DOC));
    bencher.bench_local(|| settled(Editor::open(&bytes, &fonts).expect("the fixture opens"), &fonts));
}

/// A switch from All markup to `view`.
#[divan::bench(args = [MarkupView::NoMarkup, MarkupView::Original], sample_count = 10, sample_size = 1)]
fn view(bencher: Bencher, view: MarkupView) {
    let fonts = Fonts::system();
    let e = RefCell::new(settled(Editor::open(&super::fixture(DOC), &fonts).expect("the fixture opens"), &fonts));
    bencher
        .with_inputs(|| {
            let mut e = e.borrow_mut();
            e.set_markup_view(MarkupView::All);
            while e.idle(&fonts) {}
        })
        .bench_local_values(|()| {
            let mut e = e.borrow_mut();
            e.set_markup_view(view);
            while e.idle(&fonts) {}
        });
}

/// Accept all (`true`) or reject all, on a freshly opened document each sample.
#[divan::bench(args = [true, false], sample_count = 10, sample_size = 1)]
fn resolve_all(bencher: Bencher, accept: bool) {
    let (fonts, bytes) = (Fonts::system(), super::fixture(DOC));
    bencher.with_inputs(|| settled(Editor::open(&bytes, &fonts).expect("the fixture opens"), &fonts)).bench_local_values(|mut e| {
        let all = twip::api::command::AuthorFilter::default();
        e.execute(if accept { Command::AcceptAllRevisions(all) } else { Command::RejectAllRevisions(all) }, &fonts);
        settled(e, &fonts)
    });
}
