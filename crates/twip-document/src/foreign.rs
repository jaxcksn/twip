//! Content kept in the format it was read from

use std::rc::Rc;

/// The file format a [`Foreign`] payload is in, by the name the adapter that reads and writes it gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormatId(pub &'static str);

/// Content the model doesn't hold as its own (an equation, a subdocument, an element no adapter models), kept as the
/// adapter that read it read it, for that adapter to write back. No caret units; not selectable on its own and not
/// editable: it moves, copies and deletes with the text around it. Compared verbatim, payload and bindings in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Foreign {
    /// The format the payload is in: only an adapter for it writes it.
    pub format: FormatId,
    /// The content as read, in its format's own text.
    pub payload: Rc<str>,
    /// The namespace bindings the payload uses and doesn't declare itself, as prefix (empty: the default namespace)
    /// and URI: what the part it is written in must bind.
    pub bindings: Rc<[(Rc<str>, Rc<str>)]>,
    /// Whether the payload names something in its package (a relationship, a package-scoped id): it means the same
    /// only in the package it was read from.
    pub package_bound: bool,
    /// Whether it is an equation (Go To lands on it).
    pub equation: bool,
}
