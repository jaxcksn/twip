//! Formatting and resolving it (`twip_document::format`): [`resolve`] folds levels into the effective
//! [`crate::docx::RunStyle`] and [`crate::docx::ParaStyle`], [`cascade`] follows style chains, table style regions
//! and list levels, [`counters`] numbers lists.

pub use crate::document::format::*;
