//! Tracked revisions in the editor: recording edits as revisions while tracking is on, accepting and rejecting
//! them, and finding them in document order.

pub mod index;
pub(super) mod record;
pub mod resolve;

pub(super) use record::Recording;
