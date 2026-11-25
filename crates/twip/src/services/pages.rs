//! Page versions

use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::layout::fragment::PageFragment;

/// A version for each laid-out page, for caching what is drawn of it (a thumbnail): a page keeps its version while
/// layout keeps its fragment, and a page laid out again takes a new one. Versions are never 0 and never reused.
///
/// Each page is known by its fragment's address, held by a [`Weak`] so the allocation can't be freed and the address
/// given to another page while the version is held.
#[derive(Debug, Default)]
pub struct PageVersions {
    by_page: HashMap<*const PageFragment, (Weak<PageFragment>, u64)>,
    spare: HashMap<*const PageFragment, (Weak<PageFragment>, u64)>,
    versions: Vec<u64>,
    last: u64,
}

impl PageVersions {
    /// The versions of `pages` (a layout's pages in order), one per page: each page laid out since the last call has
    /// a new one. One pass over the pages; no allocation once warm.
    pub fn update(&mut self, pages: &crate::layout::pvec::Pvec<Rc<PageFragment>>) -> &[u64] {
        self.spare.clear();
        self.versions.clear();
        for page in pages {
            let key = Rc::as_ptr(page);
            // A fragment the layout shows twice is one page drawn twice: one version
            if let Some(e) = self.spare.get(&key) {
                self.versions.push(e.1);
                continue;
            }
            let entry = match self.by_page.remove(&key) {
                Some(e) => e,
                None => {
                    self.last += 1;
                    (Rc::downgrade(page), self.last)
                }
            };
            self.versions.push(entry.1);
            self.spare.insert(key, entry);
        }
        std::mem::swap(&mut self.by_page, &mut self.spare);
        &self.versions
    }
}

#[cfg(test)]
mod tests {
    use super::PageVersions;
    use crate::api::command::Command as C;
    use crate::editing::Editor;
    use crate::editing::tests::fonts;

    #[test]
    fn a_page_keeps_its_version_until_it_is_laid_out_again() {
        let brk = r#"<w:r><w:br w:type="page"/></w:r>"#;
        let body = format!("<w:p><w:r><w:t>a</w:t></w:r>{brk}</w:p><w:p><w:r><w:t>b</w:t></w:r>{brk}</w:p><w:p><w:r><w:t>c</w:t></w:r></w:p>");
        let mut e = Editor::open(&crate::editing::tests::blank_body(&body), fonts()).expect("opens");
        e.complete(fonts());
        let mut v = PageVersions::default();
        let first = v.update(e.snapshot().pages()).to_vec();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|&n| n != 0) && first[0] != first[1] && first[1] != first[2]);
        assert_eq!(v.update(e.snapshot().pages()), first, "the same layout");
        e.execute(C::InsertText("x".into()), fonts());
        e.complete(fonts());
        assert!(crate::editing::tests::texts(&e)[0].starts_with("xa"), "typed on the first page");
        let after = v.update(e.snapshot().pages()).to_vec();
        assert_ne!(after[0], first[0], "the page typed on");
        assert_eq!(after[1..], first[1..], "the pages layout kept");
        e.execute(C::Undo(Default::default()), fonts());
        e.complete(fonts());
        let undone = v.update(e.snapshot().pages()).to_vec();
        assert!(undone[0] != first[0] && undone[0] != after[0], "a page laid out again never takes an old version");
    }

    #[test]
    fn a_fragment_shown_twice_keeps_one_version() {
        let mut e = Editor::open(&crate::editing::tests::blank_with("a"), fonts()).expect("opens");
        e.complete(fonts());
        let page = e.snapshot().pages()[0].clone();
        let twice: crate::layout::pvec::Pvec<_> = [page.clone(), page].into_iter().collect();
        let mut v = PageVersions::default();
        let first = v.update(&twice).to_vec();
        assert_eq!(first[0], first[1], "one fragment, one version");
        assert_eq!(v.update(&twice), first, "and the same next time");
    }
}
