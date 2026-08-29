//! A named unit with its own lifetime, and one expression for the whole of it.
//!
//! The actor is created by the very call that claims the reducer, so the pair
//! is known by construction: there is nothing to wire and nothing to leave
//! unwired. `context` is what the route captured, handed over typed - the
//! feature never sees an address, so there is no segment index to get wrong.

use guinea::feature::{Feature, FeatureInitContext};
use guinea_macros::installs;
use guinea_core::feature::Bound;

use super::actor::ProcessActor;
use super::contracts::{self, Refresh};

pub struct ProcessesFeature {
    /// Held, not dropped: a feature that a segment wired to another one is
    /// reached through what `install` returned.
    _listing: Bound<contracts::Processes>,
}

#[installs]
impl Feature for ProcessesFeature {
    /// One reducer, and pages below may read it. Anything else this feature
    /// claimed would stay its own.
    type Exports = (contracts::Processes,);

    fn install(cx: &FeatureInitContext, context: &str) -> anyhow::Result<Self> {
        let listing = cx.state::<contracts::Processes>().driven_by(|push| {
            ProcessActor::new(context.to_string(), push, cx.event_bus.clone())
        });

        listing.emit(Refresh);
        Ok(Self { _listing: listing })
    }
}
