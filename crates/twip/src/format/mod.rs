//! Authored formatting (`twip_document::format`), and resolving it: [`resolve`] folds levels into the effective
//! [`crate::docx::RunStyle`] and [`crate::docx::ParaStyle`], [`cascade`] follows style chains, table style regions
//! and list levels, [`counters`] numbers lists.

pub mod cascade;
pub mod counters;
pub mod resolve;

pub use crate::document::format::*;
