//! Headless conversion: the same batch converted one after another, and spread over threads.

use divan::Bencher;
use twip::pdf::{FontSet, PdfOptions, convert};

const BATCH: [&str; 4] = ["text-basics.docx", "lists.docx", "tables.docx", "pagination.docx"];

/// The test font kit as a conversion's font set.
fn fonts() -> FontSet {
    let (faces, aliases) = crate::kit();
    faces.into_iter().fold(FontSet::builder(), |b, face| b.font(face)).aliases(aliases).build().expect("the kit builds").0
}

/// The batch, eight times over: enough work for 16 threads.
fn batch() -> Vec<Vec<u8>> {
    (0..8).flat_map(|_| BATCH.map(super::fixture)).collect()
}

/// The batch converted on one thread: the baseline `parallel` is measured against.
#[divan::bench(sample_count = 5)]
fn sequential(bencher: Bencher) {
    let (fonts, docs) = (fonts(), batch());
    bencher.bench_local(|| {
        for d in &docs {
            convert(d, &fonts, &PdfOptions::default()).expect("converts");
        }
    });
}

/// The batch spread over the machine's threads (at most 16), one font set shared. Budget: at least 0.7 x the thread
/// count faster than `sequential`.
#[divan::bench(ignore = cfg!(codspeed_build), sample_count = 5)]
fn parallel(bencher: Bencher) {
    let (fonts, docs) = (fonts(), batch());
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
    bencher.bench_local(|| {
        std::thread::scope(|s| {
            for t in 0..threads {
                let (fonts, docs) = (&fonts, &docs);
                s.spawn(move || {
                    for d in docs.iter().skip(t).step_by(threads) {
                        convert(d, fonts, &PdfOptions::default()).expect("converts");
                    }
                });
            }
        });
    });
}
