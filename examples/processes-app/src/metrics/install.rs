use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use guinea_core::actor::Addr;

use super::actor::{MetricsActor, Tick};
use super::contracts::MetricsReducer;

pub fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
    let addr = Addr::new_managed_scoped(MetricsActor::new(ctx.port::<MetricsReducer>()), ctx.token.clone());

    addr.send(Tick);

    ctx.scope.own_actor(addr);
    Ok(())
}
