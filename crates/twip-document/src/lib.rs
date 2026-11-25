//! Twip's document model: what a document holds, independent of any file format.
//!
//! An internal part of Twip with no stable API; `twip::api` is the contract. The crate depends on no XML, archive
//! or layout code, so nothing in it can read or write a package.

#![warn(missing_docs)]

pub mod content;
pub mod format;
pub mod ident;
pub mod policy;
pub mod position;
