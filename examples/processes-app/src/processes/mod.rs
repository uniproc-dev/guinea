//! The processes feature: contracts (port/actions/reducer), the actor behind
//! the port, its install, and the page - self-contained in one folder.
mod actor;
mod contracts;
mod install;
mod page;

pub use page::Processes;
