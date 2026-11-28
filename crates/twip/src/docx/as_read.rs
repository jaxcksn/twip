//! The as-read record in the origin of authored properties

use std::rc::Rc;

use crate::document::authored::Authored;
use crate::document::origin::Origin;

/// What authored properties read from a part keep in their origin: the typed value read, and the element it was read
/// from (or last written as).
pub struct AsRead<T> {
    /// The typed value read.
    pub read: Rc<T>,
    /// The element it was read from.
    pub xml: Rc<str>,
}

impl<T: 'static> AsRead<T> {
    /// An origin holding `read`, read from `xml`.
    pub fn origin(read: Rc<T>, xml: Rc<str>) -> Origin {
        Origin::new(AsRead { read, xml })
    }

    /// The record `o` holds, if it holds one.
    pub fn of(o: &Origin) -> Option<&AsRead<T>> {
        o.get()
    }
}

/// `read`, read from `xml`, with no edits since.
pub fn read<T: 'static, S, K>(read: Rc<T>, xml: Rc<str>) -> Authored<Rc<T>, S, K> {
    Authored::new(read.clone(), AsRead::origin(read, xml))
}

/// The element `d` was read from, while its value is the one read from it (the same, or `alike`).
pub fn as_read<T: 'static, S, K>(d: &Authored<Rc<T>, S, K>, alike: impl FnOnce(&T, &T) -> bool) -> Option<&Rc<str>> {
    let r = AsRead::<T>::of(d.origin())?;
    (Rc::ptr_eq(&r.read, d.get()) || alike(&r.read, d.get())).then_some(&r.xml)
}

/// The element `d` was read from (or last written as), whatever its value is now.
pub fn origin_xml<T: 'static, S, K>(d: &Authored<Rc<T>, S, K>) -> Option<&Rc<str>> {
    AsRead::<T>::of(d.origin()).map(|r| &r.xml)
}

/// `d` with `xml` in place of the element it was read from.
pub fn with_origin_xml<T: 'static, S: Clone, K>(d: &Authored<Rc<T>, S, K>, xml: Rc<str>) -> Authored<Rc<T>, S, K> {
    match AsRead::<T>::of(d.origin()) {
        Some(r) => d.with_origin(AsRead::origin(r.read.clone(), xml)),
        None => d.clone(),
    }
}

/// The value read and the element it was read from, as the writer takes them.
pub fn record<T: 'static, S, K>(d: &Authored<Rc<T>, S, K>) -> Option<(&T, &str)> {
    AsRead::<T>::of(d.origin()).map(|r| (&*r.read, &*r.xml))
}
