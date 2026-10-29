//! Twip: DOCX layout, editing and rendering. [`api`] is the public contract every SDK follows: the engine and its
//! sessions, the commands, state, events, services, errors and warnings.

pub mod api;
pub(crate) mod docx;
pub(crate) mod editing;
#[cfg(not(target_os = "emscripten"))]
pub(crate) mod features;
pub(crate) mod font;
#[cfg(not(target_os = "emscripten"))]
pub(crate) mod golden;
pub(crate) mod layout;
pub(crate) mod pdf;
pub(crate) mod prof;
pub(crate) mod render;
pub(crate) mod services;
#[cfg(test)]
mod tests;

/// The engine's modules, for the crate's own tools (`twip-tools`: the golden, harness and benchmark commands). Not
/// part of the API: no documentation and no compatibility promise.
#[doc(hidden)]
pub mod internal {
    pub mod docx {
        pub use crate::docx::*;
    }
    pub mod editing {
        pub use crate::editing::*;
    }
    #[cfg(not(target_os = "emscripten"))]
    pub mod features {
        pub use crate::features::*;
    }
    pub mod font {
        pub use crate::font::*;
    }
    #[cfg(not(target_os = "emscripten"))]
    pub mod golden {
        pub use crate::golden::*;
    }
    pub mod layout {
        pub use crate::layout::*;
    }
    pub mod pdf {
        pub use crate::pdf::*;
    }
    pub mod prof {
        pub use crate::prof::*;
    }
    pub mod render {
        pub use crate::render::*;
    }
    pub mod services {
        pub use crate::services::*;
    }
}
