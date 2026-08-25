//! Everything the processes example is, apart from how it looks.
//!
//! Actors, reducers, messages and the features that install them - none of it
//! mentions a toolkit, so both front ends link this crate and differ only in
//! their pages and their route tree.

pub mod events;
pub mod l10n;
pub mod metrics;
pub mod processes;
pub mod services;
pub mod startup;
pub mod tabs;
