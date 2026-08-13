#![cfg_attr(coverage, feature(coverage_attribute))]
#![cfg_attr(coverage, coverage(off))]

pub mod l10n;
pub mod trace;
pub mod util;

pub use util::{suggest_closest, write_if_changed};

pub use guinea_meta_build as meta;
