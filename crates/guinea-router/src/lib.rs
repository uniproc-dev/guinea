//! Nested routing, with no opinion about how anything is drawn.
//!
//! The router installs and tears down scopes as the route changes, and mounts
//! whatever the backend put in [`router::SegmentEntry`]. What a view is, and
//! what it is handed, is the backend's business - see [`router::Ui`].

pub mod headless;
pub mod router;

pub use router::*;
