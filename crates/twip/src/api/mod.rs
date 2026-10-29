//! The contract: Twip's public editor API. Every first-class SDK (`@twip/web`, `Twip.Core`) follows what this module
//! defines: the operations, state, events, errors and warnings, and what each one means. It is typed throughout, and
//! its values cross the SDK wire as JSON through [`wire`].
//!
//! An [`engine::Engine`] owns the font library and the sessions of one thread. A [`session::Session`] is one document
//! being edited: [`command::Command`]s change it, each as one transaction, and after each change its
//! [`state::State`] is rebuilt and the engine's listener hears an [`event::Event`].

#![deny(missing_docs)]

pub mod catalog;
pub mod command;
pub mod engine;
pub mod error;
pub mod event;
pub mod fonts;
pub mod input;
pub mod record;
pub mod review;
pub mod services;
pub mod session;
pub mod state;
pub mod value;
pub mod warning;
pub mod wire;
