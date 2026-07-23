//! Lives here (not in `context`, where the rest of the page/tab-status
//! machinery does) because it's also a port-message payload type used by
//! `app-contracts` - and `app-contracts` shouldn't need `context` (which
//! pulls in the full Slint/winit backend via `guinea`) just for this one
//! enum. `context::page_status` re-exports this type so nothing there needs
//! to change.

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum PageStatus {
    #[default]
    Inactive,
    Loading,
    Ready,
    Error,
}
