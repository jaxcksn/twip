//! Reading documents into the model: the read for editing alone (no layout), on the largest committed fixtures.

use divan::Bencher;

#[divan::bench(args = ["big.docx", "docx-feature-showcase.docx", "editor/FX-LONG-100.docx"], sample_count = 20)]
fn read(bencher: Bencher, name: &str) {
    let bytes = super::fixture(name);
    bencher.bench_local(|| twip::internal::docx::read_editable(&bytes).expect("the fixture reads"));
}
