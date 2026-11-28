//! Twip's document model: what a document holds, independent of any file format.
//!
//! An internal part of Twip with no stable API; `twip::api` is the contract. The crate depends on no XML, archive
//! or layout code, so nothing in it can read or write a package.

#![warn(missing_docs)]

pub mod anchor;
pub mod authored;
pub mod content;
pub mod foreign;
pub mod format;
pub mod graphic;
pub mod ident;
pub mod line;
pub mod origin;
pub mod paragraph;
pub mod policy;
pub mod position;
pub mod resource;
pub mod section;
