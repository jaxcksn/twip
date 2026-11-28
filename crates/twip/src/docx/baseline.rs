//! A document's typed definitions beside the XML path's resolver, read apart from the same parts

use std::rc::Rc;

use super::Session;
use super::definitions::{Definitions, TypedDefs};
use super::model::Document;
use super::resolve::Resolver;
use crate::format::cascade::Cascade;

/// A document's typed definitions, and the XML path's resolver read from the parts the document is saved as.
pub struct Baseline {
    /// The XML path's resolver.
    pub frozen: Resolver,
    /// The document's typed definitions.
    pub typed: Rc<Definitions>,
}

impl std::ops::Deref for Baseline {
    type Target = Resolver;
    fn deref(&self) -> &Resolver {
        &self.frozen
    }
}

impl Baseline {
    /// `d`'s typed definitions, and the XML path's resolver of `d` saved with `docx`.
    pub fn of(d: &Document, docx: &Session) -> Baseline {
        let bytes = super::write::save(d, docx).unwrap_or_else(|e| panic!("the document saves: {e}"));
        Baseline::read(d, &bytes)
    }

    /// `d`'s typed definitions, and the XML path's resolver of the package `d` was read from (`docx`'s source).
    pub fn as_read(d: &Document, docx: &Session) -> Baseline {
        Baseline::read(d, &docx.source.as_ref().expect("the document was read for editing").package)
    }

    /// `d`'s typed definitions, and the XML path's resolver of the package `bytes`.
    pub fn read(d: &Document, bytes: &[u8]) -> Baseline {
        let mut frozen = super::reader::frozen_resolver(bytes).expect("the package reads");
        let typed = d.definitions.clone().expect("the document has definitions");
        // Picture bullets' images, as reading gave them
        for (id, p) in &typed.cascade().lists.pictures {
            if let Some(b) = frozen.numbering.pic_bullets.get_mut(id) {
                b.image = p.image;
            }
        }
        Baseline { frozen, typed }
    }

    /// The typed definitions.
    pub fn typed(&self) -> Rc<TypedDefs> {
        self.typed.typed()
    }

    /// The typed cascade.
    pub fn cascade(&self) -> Rc<Cascade> {
        self.typed.cascade()
    }
}
