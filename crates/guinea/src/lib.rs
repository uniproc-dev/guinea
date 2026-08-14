//! The windows-reactor face of guinea: routing, the run loop, and the
//! re-exports an application builds against.
//!
//! Everything that does not touch a toolkit lives in `guinea-app` and
//! `guinea-core`; this crate is where they meet windows-reactor.

mod dispatcher;
mod run;

pub mod headless;
pub mod router;
pub mod winui;

/// The backend this build renders with, as a module and as a type. `routes!`
/// targets these, so an application names the concrete backend nowhere - and
/// a second backend is a change to these two lines.
pub use winui as backend;
pub type Backend = winui::WinUi;

pub use run::run;

pub use guinea_app::{app, feature, lifecycle_tracker};
pub use guinea_app::timers as reactor;

pub use guinea_codegen as codegen;
pub use guinea_core as core;
pub use guinea_core::uri;
pub use guinea_meta as meta;
