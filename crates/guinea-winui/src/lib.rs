//! guinea on windows-reactor: what a view is, how a segment is mounted, and
//! the hooks a view reads state through.
//!
//! Not the run loop - the reactor keeps that. See [`bootstrap`].

mod dispatcher;
mod bootstrap;
mod winui;

pub use bootstrap::{Bootstrap, shutdown};
pub use winui::*;
