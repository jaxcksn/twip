//! Crate-wide tests: subsystem boundaries.

mod boundaries;
mod contract;
mod ltb;
mod merge_us1;
mod merge_us5;
mod review_comments;
mod review_equivalence;
mod review_resolve;
mod review_roundtrip;
mod review_services;
mod review_views;

/// An engine with one session on `bytes`, the system's fonts in its library.
fn session(bytes: &[u8]) -> (crate::api::engine::Engine, crate::api::engine::SessionId) {
    let mut engine = crate::api::engine::Engine::new(crate::api::fonts::FontLibrary::system());
    let id = engine.create_session(crate::api::engine::SessionOptions::default());
    engine.session(id).expect("a session").open(bytes).expect("opens");
    (engine, id)
}
