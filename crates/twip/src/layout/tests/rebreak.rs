//! Breaking a shaped paragraph at another width doesn't shape it again.

use crate::docx::Block;
use crate::layout::cache::LayoutCache;
use crate::layout::env::LayoutEnv;
use crate::layout::job::{Memos, Meter};
use crate::layout::line::{break_lines, break_lines_uncached, neighbour_borders};
use crate::layout::space::{ParaSpace, ShapeSpace};
use crate::layout::text::shape;

use super::{fonts, plain_doc, words};

#[test]
fn rebreak_does_not_reshape() {
    let Some(doc) = plain_doc(&[words(300)]) else { return };
    let Block::Para(p) = &doc.sections[0].blocks[0] else { unreachable!() };
    let fonts = fonts();
    let env = LayoutEnv::new(&doc);
    let mut cache = LayoutCache::new();
    let mut meter = Meter::unlimited();
    let mut memos = Memos { cache: &mut cache, job: None, meter: &mut meter };
    crate::prof::take();
    let shaped = shape(p, &ShapeSpace::default(), &fonts, &mut memos).expect("unlimited");
    for width in [468.0, 300.0] {
        let space = ParaSpace { width, start: 0, borders: neighbour_borders(&p.style, None, None), x0: 0.0, env: env.text.clone() };
        let lines = break_lines(&shaped, p, &space, &fonts, &mut memos).expect("unlimited");
        let fresh = break_lines_uncached(&shaped, p, &space, &fonts, &mut memos).expect("unlimited");
        assert_eq!(*lines, fresh, "lines at width {width}");
        assert!(lines.lines.len() > 1, "300 words break into several lines");
    }
    let shaped_calls = crate::prof::take().iter().filter(|t| t.0 == "itemize").map(|t| t.2).sum::<u64>();
    assert_eq!(shaped_calls, 1, "the paragraph is shaped once for both widths");
}
