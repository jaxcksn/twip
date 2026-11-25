//! Allocations on layout's per-page and per-edge paths

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use crate::docx::review::MarkupView;
use crate::docx::{Block, Document};

/// The system allocator, counting this thread's allocations and the bytes it holds.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static LIVE: Cell<i64> = const { Cell::new(0) };
}

fn held(n: i64) {
    let _ = LIVE.try_with(|l| l.set(l.get() + n));
}

// SAFETY: every call is forwarded unchanged to `System`; the counter is a const-initialised thread-local, which
// neither allocates nor reenters the allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        held(layout.size() as i64);
        // SAFETY: the caller's contract is `System`'s.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        held(-(layout.size() as i64));
        // SAFETY: the caller's contract is `System`'s.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        held(new_size as i64 - layout.size() as i64);
        // SAFETY: the caller's contract is `System`'s.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// How many allocations `f` made on this thread.
fn allocations(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

/// How many allocations `f` made on this thread, and the bytes it left held there.
fn allocations_and_held<T>(f: impl FnOnce() -> T) -> (T, u64, i64) {
    let (n, live) = (ALLOCATIONS.with(Cell::get), LIVE.with(Cell::get));
    let out = f();
    (out, ALLOCATIONS.with(Cell::get) - n, LIVE.with(Cell::get) - live)
}

fn showcase() -> Document {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/docx-feature-showcase.docx")).expect("the showcase");
    crate::docx::read_editable(&bytes).expect("reads")
}

#[test]
fn resolved_table_borders_copy_without_allocating() {
    fn borders(blocks: &[Block], out: &mut Vec<crate::docx::props::Border>) {
        for b in blocks {
            let Block::Table(t) = b else { continue };
            out.extend(t.borders.iter().flatten().cloned());
            for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                out.extend([&c.top, &c.left, &c.bottom, &c.right].into_iter().flatten().cloned());
                borders(&c.blocks, out);
            }
        }
    }
    assert_eq!(allocations(|| drop(black_box(Box::new(0u8)))), 1, "the count counts");
    let doc = showcase();
    let mut all = Vec::new();
    doc.sections.iter().for_each(|s| borders(&s.blocks, &mut all));
    assert!(all.iter().any(|b| b.visible()), "the showcase's tables have borders");
    assert_eq!(allocations(|| all.iter().for_each(|b| drop(black_box(b.clone())))), 0, "a cell edge's border is copied per edge");
}

#[test]
fn a_sections_page_setup_borrows_it() {
    let doc = showcase();
    let n = allocations(|| {
        for s in &doc.sections {
            for view in [MarkupView::All, MarkupView::Original] {
                black_box(s.section_in(view));
            }
        }
    });
    assert_eq!(n, 0, "layout reads a section's page setup several times a page");
}

#[test]
fn a_new_pages_borders_share_its_sections() {
    let doc = showcase();
    let borders: Vec<_> = doc.sections.iter().filter_map(|s| s.page_borders.as_ref()).collect();
    assert!(!borders.is_empty(), "the showcase has page borders");
    assert_eq!(allocations(|| borders.iter().for_each(|b| drop(black_box((*b).clone())))), 0, "every new page records its borders");
}

/// Reading each large fixture for editing (allocations, and the heap the model holds), and one keystroke in an open
/// editor (allocations). Deterministic: `TWIP_ALLOC_OUT=file` writes them, to compare two builds.
#[test]
fn allocations_per_load_and_per_keystroke() {
    use std::fmt::Write;
    let fonts = crate::font::Fonts::system_recommended();
    let mut report = String::new();
    for name in ["big.docx", "docx-feature-showcase.docx", "editor/FX-LONG-100.docx"] {
        let bytes = std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).expect("the fixture");
        let (doc, n, live) = allocations_and_held(|| crate::docx::read_editable(&bytes).expect("reads"));
        writeln!(report, "{name} load allocations={n} model_bytes={live}").unwrap();
        drop(doc);
        let mut e = crate::editing::Editor::open(&bytes, &fonts).expect("opens");
        e.complete(&fonts);
        let ((), n, _) = allocations_and_held(|| {
            e.execute(crate::editing::commands::Command::InsertText("x".into()), &fonts);
            e.complete(&fonts);
        });
        writeln!(report, "{name} keystroke allocations={n}").unwrap();
    }
    if let Some(out) = std::env::var_os("TWIP_ALLOC_OUT") {
        std::fs::write(out, &report).expect("writes the report");
    }
    assert!(report.lines().count() == 6, "{report}");
}
