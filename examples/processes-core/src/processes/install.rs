use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use guinea_core::actor::Addr;

use super::actor::ProcessActor;
use super::contracts::{ProcessesReducer, Refresh};

pub fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {

    let addr = Addr::new_managed_scoped(
        ProcessActor::new(
            uri.segment(0).expect("route always carries a :context segment").to_string(),
            ctx.port::<ProcessesReducer>(),
            ctx.event_bus.clone(),
        ),
        ctx.token.clone(),
    );

    ctx.wire::<ProcessesReducer, _>(&addr);

    addr.send(Refresh);

    ctx.scope.own(addr);
    Ok(())
}
