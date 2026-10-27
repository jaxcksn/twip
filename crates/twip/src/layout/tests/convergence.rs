//! Convergence of pass-dependent values and the stabilizing pass (FIELD-6, HDR-10).

use std::path::PathBuf;

use crate::layout::LayoutStatus;
use crate::layout::cache::LayoutCache;
use crate::layout::document::{FAIL_VERIFY, HF_ALTERNATE, layout_records};
use crate::layout::record::PageRecord;

use super::{doc, fixture, fixtures, fonts};

/// The fixtures of headers and footers with page fields (HDR-10).
fn hf9() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(fixtures().join("HF9"))
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "docx")).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Whether page `pg`'s composed header and footer heights are the ones its first body was laid out with.
fn heights_agree(pg: &PageRecord) -> bool {
    let used = pg.bodies.first().map_or([None, None], |b| b.space.hf);
    (0..2).all(|k| match (&pg.hf[k], used[k]) {
        (None, None) => true,
        (Some(h), Some(u)) => (h.height - u).abs() < 1e-6,
        _ => false,
    })
}

#[test]
fn converged_header_heights() {
    let paths = hf9();
    for path in &paths {
        let Some(d) = doc(path) else { continue };
        let (s, records) = layout_records(&d, &fonts(), &mut LayoutCache::new());
        assert_eq!(*s.status(), LayoutStatus::Converged, "{}", path.display());
        for (i, pg) in records.iter().enumerate() {
            assert!(heights_agree(pg), "{} page {}: composed header/footer heights differ from the body's", path.display(), i + 1);
        }
    }
}

/// Runs `f` with the alternating-height hook (and the failing-verification hook) set, resetting them afterwards.
fn with_hooks<T>(height: f64, fail: bool, f: impl FnOnce() -> T) -> T {
    HF_ALTERNATE.set(height);
    FAIL_VERIFY.set(fail);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    HF_ALTERNATE.set(0.0);
    FAIL_VERIFY.set(false);
    r.unwrap_or_else(|e| std::panic::resume_unwind(e))
}

#[test]
fn forced_non_convergence() {
    let Some(d) = fixture(r"HF9\HF9_f_pagenum.docx") else { return };
    let (s, records) = with_hooks(12.0, false, || layout_records(&d, &fonts(), &mut LayoutCache::new()));
    let LayoutStatus::Stabilized { frozen_hf, .. } = s.status() else { panic!("expected Stabilized, got {:?}", s.status()) };
    let with_header: Vec<usize> = records.iter().enumerate().filter(|(_, pg)| pg.hf[0].is_some()).map(|(i, _)| i).collect();
    assert!(!with_header.is_empty(), "the fixture has headers");
    assert_eq!(&frozen_hf[..], &with_header[..], "every page with a header was composed shorter than its reservation");
    let (s, _) = with_hooks(12.0, true, || layout_records(&d, &fonts(), &mut LayoutCache::new()));
    let LayoutStatus::Inconsistent { pages } = s.status() else { panic!("expected Inconsistent, got {:?}", s.status()) };
    assert_eq!(&pages[..], &with_header[..]);
}

/// Diagnostic (run with `--ignored --nocapture`): the status of every reference dump document.
#[test]
#[ignore]
fn status_census() {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (path, d) in super::gate_set(crate::features::ALL) {
        let s = crate::layout::layout(&d, &fonts());
        let key = match s.status() {
            LayoutStatus::Converged => "Converged".to_string(),
            other => {
                eprintln!("{}: {other:?}", path.display());
                format!("{other:?}").split(' ').next().unwrap_or("").to_string()
            }
        };
        *counts.entry(key).or_default() += 1;
    }
    eprintln!("{counts:?}");
}

/// A body records only the values of the page fields it shows: none for plain text, the page number for a
/// PAGE field.
#[test]
fn bodies_record_the_fields_they_show() {
    use crate::docx::Block;
    let Some(mut d) = super::plain_doc(&[super::words(400), super::words(20)]) else { return };
    let (_, plain) = layout_records(&d, &fonts(), &mut LayoutCache::new());
    assert!(plain.iter().flat_map(|p| &p.bodies).all(|b| b.fields == Default::default()), "plain bodies show no page fields");
    let Block::Para(p) = &mut d.sections[0].blocks[1] else { unreachable!() };
    let run = p.mark.clone();
    p.inlines.extend(crate::docx::fields::field_inlines("PAGE", "1", &run));
    let (_, records) = layout_records(&d, &fonts(), &mut LayoutCache::new());
    let last = records.last().and_then(|p| p.bodies.last()).expect("a body");
    assert_eq!(last.fields.page.as_deref(), Some(records.len().to_string().as_str()), "the body with PAGE records its page number");
    assert!(last.fields.num_pages.is_none() && last.fields.page_refs.is_none());
}
