//! A dependency left out of a cache key is caught by verify mode: with the fault flag on, the lines key omits
//! `ParaSpace.width`, so the second width hits the first width's lines and the recomputation differs.

use crate::docx::Block;
use crate::layout::cache::LayoutCache;
use crate::layout::env::LayoutEnv;
use crate::layout::job::{Memos, Meter};
use crate::layout::line::{OMIT_WIDTH, break_lines, neighbour_borders};
use crate::layout::space::{ParaSpace, ShapeSpace};
use crate::layout::text::shape;

use super::{fonts, plain_doc, words};

/// `catch_unwind` rather than `#[should_panic]`: the test skips itself without fixture fonts, as the others do.
#[test]
fn omitted_key_is_caught() {
    let Some(doc) = plain_doc(&[words(300)]) else { return };
    let Block::Para(p) = &doc.sections[0].blocks[0] else { unreachable!() };
    let fonts = fonts();
    let env = LayoutEnv::new(&doc);
    let mut cache = LayoutCache::new();
    let mut meter = Meter::unlimited();
    let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
    let shaped = shape(p, &ShapeSpace::default(), &fonts, &mut memos).expect("unlimited");
    OMIT_WIDTH.set(true);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for width in [468.0, 300.0] {
            let space = ParaSpace { width, start: 0, borders: neighbour_borders(&p.style, None, None), x0: 0.0, env: env.text.clone() };
            break_lines(&shaped, p, &space, &fonts, &mut memos).expect("unlimited");
        }
    }));
    OMIT_WIDTH.set(false);
    let message = caught.expect_err("verify mode catches the omitted width");
    let message = message.downcast_ref::<String>().map(String::as_str).unwrap_or_default();
    assert!(message.contains("boundary `lines`"), "{message}");
}
