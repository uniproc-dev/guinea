//! The services feature - a simpler sibling of `processes`: same shape
//! (port -> reducer -> view), no dispatch back into the actor.
mod actor;
mod contracts;
mod install;
mod page;

pub use page::Services;
