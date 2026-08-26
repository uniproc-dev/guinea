//! guinea, assembled: the agnostic halves plus whichever backends this build
//! renders with.
//!
//! An application depends on this crate and nothing else. A backend arrives as
//! a feature, and while exactly one is enabled it also arrives as [`Backend`]
//! and [`backend`] - which `routes!` targets by default, so a single-backend
//! application names its toolkit nowhere.
//!
//! Enable two and that shorthand goes away on purpose: there is no sensible
//! answer to "the backend" any more, and every route tree has to say which one
//! it is for:
//!
//! ```ignore
//! routes! {
//!     backend = guinea::ratatui::Tui,
//!     Route { .. }
//! }
//! ```
//!
//! With no backend at all what is left is the router, the application runtime
//! and the macros - which is what a port to another toolkit starts from.

pub use guinea_router::{headless, router};

#[cfg(feature = "winui")]
pub use guinea_winui as winui;

#[cfg(feature = "ratatui")]
pub use guinea_ratatui as ratatui;

#[cfg(feature = "slint")]
pub use guinea_slint as slint;

#[cfg(feature = "eframe")]
pub use guinea_eframe as eframe;

/// The backend this build renders with, as a module and as a type.
///
/// Defined only while exactly one backend feature is on. `routes!` falls back
/// to these when a route tree does not name a backend itself.
#[cfg(all(
    feature = "winui",
    not(any(feature = "ratatui", feature = "slint", feature = "eframe"))
))]
pub use guinea_winui as backend;
#[cfg(all(
    feature = "winui",
    not(any(feature = "ratatui", feature = "slint", feature = "eframe"))
))]
pub type Backend = guinea_winui::WinUi;

#[cfg(all(
    feature = "ratatui",
    not(any(feature = "winui", feature = "slint", feature = "eframe"))
))]
pub use guinea_ratatui as backend;
#[cfg(all(
    feature = "ratatui",
    not(any(feature = "winui", feature = "slint", feature = "eframe"))
))]
pub type Backend = guinea_ratatui::Tui;

#[cfg(all(
    feature = "slint",
    not(any(feature = "winui", feature = "ratatui", feature = "eframe"))
))]
pub use guinea_slint as backend;
#[cfg(all(
    feature = "slint",
    not(any(feature = "winui", feature = "ratatui", feature = "eframe"))
))]
pub type Backend = guinea_slint::Slint;

#[cfg(all(
    feature = "eframe",
    not(any(feature = "winui", feature = "ratatui", feature = "slint"))
))]
pub use guinea_eframe as backend;
#[cfg(all(
    feature = "eframe",
    not(any(feature = "winui", feature = "ratatui", feature = "slint"))
))]
pub type Backend = guinea_eframe::Egui;

/// Stands in for `Backend` when more than one backend is enabled.
///
/// It deliberately implements nothing: the error an application gets is that
/// its routes are not for a backend, which is exactly the mistake - and the
/// `Ui` trait's own diagnostic says how to name one.
#[cfg(any(
    all(
        feature = "winui",
        any(feature = "ratatui", feature = "slint", feature = "eframe")
    ),
    all(feature = "ratatui", any(feature = "slint", feature = "eframe")),
    all(feature = "slint", feature = "eframe"),
))]
pub enum Backend {}

#[cfg(any(
    all(
        feature = "winui",
        any(feature = "ratatui", feature = "slint", feature = "eframe")
    ),
    all(feature = "ratatui", any(feature = "slint", feature = "eframe")),
    all(feature = "slint", feature = "eframe"),
))]
pub mod backend {
    //! Empty on purpose: with more than one backend enabled there is no "the"
    //! backend, so `routes!` has to be told which one - `backend =
    //! guinea::winui::WinUi`, `guinea::ratatui::Tui` or `guinea::slint::Slint`.
}

#[cfg(feature = "winui")]
pub use guinea_winui::{Bootstrap, shutdown};

pub use guinea_app::{app, app_meta, feature, lifecycle_tracker};
pub use guinea_app::timers as reactor;

pub use guinea_codegen as codegen;
pub use guinea_core as core;
pub use guinea_core::uri;
pub use guinea_meta as meta;
