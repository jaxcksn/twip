//! Where a node came from in the file it was read from

use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// Where a node came from in the file it was read from: what the adapter that read it keeps to write it back (byte
/// ranges, a part name, the value as read, the element as written). Owned by that adapter; the model never looks
/// inside, and it is no part of a node's equality.
#[derive(Clone, Default)]
pub struct Origin(Option<Rc<dyn Any>>);

impl Origin {
    /// No origin: a node made, or copied, rather than read.
    pub const NONE: Origin = Origin(None);

    /// An origin holding `v`.
    pub fn new<T: Any>(v: T) -> Origin {
        Origin(Some(Rc::new(v)))
    }

    /// An origin holding `v`, already shared.
    pub fn shared<T: Any>(v: Rc<T>) -> Origin {
        Origin(Some(v))
    }

    /// What it holds, when that is a `T`.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.as_deref().and_then(|v| v.downcast_ref())
    }

    /// Whether it holds anything.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Whether it holds nothing.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Whether `self` and `o` hold the same allocation (both nothing counts as the same).
    pub fn ptr_eq(&self, o: &Origin) -> bool {
        match (&self.0, &o.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl PartialEq for Origin {
    /// Always equal: where a node came from is no part of what it is.
    fn eq(&self, _: &Origin) -> bool {
        true
    }
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() { "Origin(..)" } else { "Origin(none)" })
    }
}

#[cfg(test)]
mod tests {
    use super::Origin;

    #[test]
    fn an_origin_gives_back_what_it_holds_and_never_differs() {
        let o = Origin::new(4..9usize);
        assert_eq!(o.get::<std::ops::Range<usize>>(), Some(&(4..9)));
        assert_eq!(o.get::<String>(), None, "only as the type it holds");
        assert!(o.is_some() && Origin::NONE.is_none());
        assert_eq!(o, Origin::NONE, "no part of equality");
        assert!(o.ptr_eq(&o.clone()) && !o.ptr_eq(&Origin::new(4..9usize)) && Origin::NONE.ptr_eq(&Origin::default()));
    }
}
