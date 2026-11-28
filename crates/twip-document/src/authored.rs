//! Authored properties

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::origin::Origin;

/// A typed value with one empty instance, shared: what a holder of nothing of its own holds.
pub trait Empty {
    /// The empty value, shared.
    fn empty() -> Rc<Self>;
}

/// What one node sets of its own: the typed value `V` (`Rc<T>`, or `Option<Rc<T>>` where a node may set nothing),
/// where it was read from, and what edits since reading set (`S`, in order), for the adapter that read it to write the
/// value over what it read. `K` tells apart holders of the same value and set types that hold different things.
///
/// No part of a node's equality: its `==` is always true. [`Authored::authored_eq`] compares the typed values.
pub struct Authored<V, S, K = ()> {
    value: V,
    origin: Origin,
    set: Vec<S>,
    of: PhantomData<fn() -> K>,
}

impl<V, S, K> Authored<V, S, K> {
    /// `value`, read from where `origin` says.
    pub fn new(value: V, origin: Origin) -> Self {
        Authored { value, origin, set: Vec::new(), of: PhantomData }
    }

    /// `value`, made rather than read.
    pub fn typed(value: V) -> Self {
        Authored::new(value, Origin::NONE)
    }

    /// The typed value.
    pub fn get(&self) -> &V {
        &self.value
    }

    /// Where it was read from.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// What edits since reading set, in order.
    pub fn set(&self) -> &[S] {
        &self.set
    }

    /// The value `make` gives, set by an edit that set `set`, in place of this one: where it was read from kept,
    /// `set` after what was set before. `make` gets the value now and everything set, `set` last.
    pub fn edited_with(&self, set: &[S], make: impl FnOnce(&V, &[S]) -> V) -> Self
    where
        S: Clone,
    {
        let mut all = self.set.clone();
        all.extend_from_slice(set);
        Authored { value: make(&self.value, &all), origin: self.origin.clone(), set: all, of: PhantomData }
    }

    /// The same value and set records, read from where `origin` says.
    pub fn with_origin(&self, origin: Origin) -> Self
    where
        V: Clone,
        S: Clone,
    {
        Authored { value: self.value.clone(), origin, set: self.set.clone(), of: PhantomData }
    }
}

impl<T: PartialEq, S, K> Authored<Rc<T>, S, K> {
    /// Authored equality: the typed values.
    pub fn authored_eq(&self, o: &Self) -> bool {
        Rc::ptr_eq(&self.value, &o.value) || self.value == o.value
    }
}

impl<T: PartialEq, S, K> Authored<Option<Rc<T>>, S, K> {
    /// Authored equality: the typed values.
    pub fn authored_eq(&self, o: &Self) -> bool {
        match (&self.value, &o.value) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a == b,
            (None, None) => true,
            _ => false,
        }
    }
}

impl<T: Empty, S, K> Default for Authored<Rc<T>, S, K> {
    /// Nothing of its own: the shared empty value.
    fn default() -> Self {
        Authored::typed(T::empty())
    }
}

impl<T, S, K> Default for Authored<Option<Rc<T>>, S, K> {
    /// Nothing of its own.
    fn default() -> Self {
        Authored::typed(None)
    }
}

impl<V: Clone, S: Clone, K> Clone for Authored<V, S, K> {
    fn clone(&self) -> Self {
        Authored { value: self.value.clone(), origin: self.origin.clone(), set: self.set.clone(), of: PhantomData }
    }
}

impl<V, S, K> PartialEq for Authored<V, S, K> {
    /// Always equal: what a node sets of its own is compared with [`Authored::authored_eq`].
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<V: fmt::Debug, S: fmt::Debug, K> fmt::Debug for Authored<V, S, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authored").field("value", &self.value).field("origin", &self.origin).field("set", &self.set).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::Authored;
    use crate::origin::Origin;

    #[test]
    fn an_edit_keeps_the_origin_and_appends_what_it_set() {
        let read = Rc::new(1);
        let a: Authored<Rc<i32>, char> = Authored::new(read.clone(), Origin::new("as read"));
        let b = a.edited_with(&['x'], |v, all| {
            assert_eq!((**v, all), (1, &['x'][..]));
            Rc::new(2)
        });
        let c = b.edited_with(&['y'], |_, all| {
            assert_eq!(all, ['x', 'y']);
            Rc::new(1)
        });
        assert!(b.origin().ptr_eq(a.origin()) && c.origin().get::<&str>() == Some(&"as read"));
        assert!(a.authored_eq(&c) && !a.authored_eq(&b) && a == b, "equal by value; `==` always");
        assert!(Rc::ptr_eq(a.get(), &read), "the value read, shared");
        assert!(c.with_origin(Origin::NONE).origin().is_none() && c.with_origin(Origin::NONE).set() == ['x', 'y']);
        let none: Authored<Option<Rc<i32>>, char> = Authored::default();
        assert!(none.authored_eq(&Authored::typed(None)) && !none.authored_eq(&Authored::typed(Some(Rc::new(1)))));
    }
}
