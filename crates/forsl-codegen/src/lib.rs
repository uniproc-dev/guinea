#![cfg_attr(coverage, feature(coverage_attribute))]
#![cfg_attr(coverage, coverage(off))]

pub mod actor_manifest;
pub mod bindings_gen;
pub mod contracts;
pub mod slint_parser;
pub mod stub_gen;
pub mod trace;
pub mod util;

pub use util::{suggest_closest, write_if_changed};
