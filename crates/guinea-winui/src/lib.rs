//! guinea on windows-reactor: what a view is, how a segment is mounted, the
//! hooks a view reads state through, and the run loop.

mod dispatcher;
mod run;
mod winui;

pub use run::{Bootstrap, run};
pub use winui::*;
