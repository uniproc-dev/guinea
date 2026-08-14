//! guinea, assembled: the agnostic halves plus the backend this build renders
//! with.
//!
//! An application depends on this crate and nothing else. The backend appears
//! in exactly two items here - [`backend`] and [`Backend`] - and `routes!`
//! targets them, so swapping toolkits is a change to this file rather than to
//! the application.

pub use guinea_router::{headless, router};
pub use guinea_winui as winui;

/// The backend this build renders with, as a module and as a type.
pub use guinea_winui as backend;
pub type Backend = guinea_winui::WinUi;

pub use guinea_winui::run;

pub use guinea_app::{app, feature, lifecycle_tracker};
pub use guinea_app::timers as reactor;

pub use guinea_codegen as codegen;
pub use guinea_core as core;
pub use guinea_core::uri;
pub use guinea_meta as meta;
