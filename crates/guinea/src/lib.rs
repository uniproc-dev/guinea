//! guinea, assembled: the agnostic halves plus the backend this build renders
//! with.
//!
//! An application depends on this crate and nothing else. The backend appears
//! behind a feature and in exactly two items - `backend` and `Backend` -
//! which `routes!` targets, so swapping toolkits is a change to this file
//! rather than to the application. Built with `--no-default-features` what is
//! left is the router, the application runtime and the macros: no toolkit at
//! all, which is what a port to another one starts from.

pub use guinea_router::{headless, router};

#[cfg(feature = "winui")]
pub use guinea_winui as winui;

/// The backend this build renders with, as a module and as a type. `routes!`
/// targets both, so an application names the concrete backend nowhere and a
/// second backend is a change to this block.
#[cfg(feature = "winui")]
pub use guinea_winui as backend;
#[cfg(feature = "winui")]
pub type Backend = guinea_winui::WinUi;

#[cfg(feature = "winui")]
pub use guinea_winui::run;

pub use guinea_app::{app, feature, lifecycle_tracker};
pub use guinea_app::timers as reactor;

pub use guinea_codegen as codegen;
pub use guinea_core as core;
pub use guinea_core::uri;
pub use guinea_meta as meta;
