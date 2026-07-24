//! The shared tab shell - a `Layout` (has `outlet()`), no actor: just proves
//! its own `Scope` persists across `Processes <-> Services` navigation.
mod contracts;
mod install;
mod page;

pub use page::TabsLayout;
