use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use guinea_core::actor::Addr;

use super::actor::{Kill, ProcessActor, Refresh};
use super::contracts::{ProcessesBinder, ProcessesReducer};

pub fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {

    let addr = Addr::new_managed_scoped(
        ProcessActor::new(
            uri.segment(0).expect("route always carries a :context segment").to_string(),
            ctx.port::<ProcessesReducer>(),
            ctx.event_bus.clone(),
        ),
        ctx.token.clone(),
    );

    ProcessesBinder::new(&addr, &ctx.actions::<ProcessesReducer>())
        .on_kill::<Kill>()
        .build();

    addr.send(Refresh);

    ctx.scope.own(addr);
    Ok(())
}
