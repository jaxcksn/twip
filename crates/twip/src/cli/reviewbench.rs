//! `twip reviewbench <doc.docx>`: how long a document with many revisions takes to open, to switch between the
//! markup views (All → No Markup → Original → All) and to accept or reject everything, each with its whole layout
//! (background layout drained). Budget: 2 s a step.

use std::time::Instant;

use twip::internal::docx::review::MarkupView;
use twip::internal::editing::Editor;
use twip::internal::editing::commands::Command;
use twip::internal::editing::revisions::index::revisions;
use twip::internal::font::Fonts;

const BUDGET_MS: f64 = 2000.0;

/// Runs `f` and the layout it leaves, prints the time, and notes `label` in `over` when it is over budget.
fn timed(label: &str, fonts: &Fonts, over: &mut Vec<String>, f: impl FnOnce() -> Result<Editor, String>) -> Result<Editor, String> {
    let t = Instant::now();
    let mut e = f()?;
    while e.idle(fonts) {}
    let ms = t.elapsed().as_secs_f64() * 1e3;
    println!("{label:<16} {ms:8.1} ms, {} pages", e.snapshot().pages().len());
    if ms > BUDGET_MS {
        over.push(label.to_string());
    }
    Ok(e)
}

pub fn run(path: &str) -> Result<(), String> {
    let fonts = Fonts::system();
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let open = || Editor::open(&bytes, &fonts).map_err(|e| format!("{path}: {e}"));
    let mut over = Vec::new();
    let mut e = timed("open", &fonts, &mut over, open)?;
    println!("{} revisions, {} comments", revisions(e.document(), e.index()).len(), e.document().comments.by_id.len());
    for (label, view) in [("view no markup", MarkupView::NoMarkup), ("view original", MarkupView::Original), ("view all", MarkupView::All)] {
        e = timed(label, &fonts, &mut over, || {
            e.set_markup_view(view);
            Ok(e)
        })?;
    }
    for (label, accept) in [("accept all", true), ("reject all", false)] {
        let mut fresh = open()?;
        while fresh.idle(&fonts) {}
        let done = timed(label, &fonts, &mut over, || {
            let all = twip::api::command::AuthorFilter::default();
            fresh.execute(if accept { Command::AcceptAllRevisions(all) } else { Command::RejectAllRevisions(all) }, &fonts);
            Ok(fresh)
        })?;
        println!("  {} revisions left", revisions(done.document(), done.index()).len());
    }
    if over.is_empty() { Ok(()) } else { Err(format!("over {BUDGET_MS} ms: {}", over.join(", "))) }
}
