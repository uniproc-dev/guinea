use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use guinea_core::actor::Addr;

use super::actor::{Refresh, ServiceActor};
use super::contracts::ServicesReducer;

pub fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
    let addr = Addr::new_managed_scoped(ServiceActor::new(ctx.port::<ServicesReducer>()), ctx.token.clone());

    addr.send(Refresh);

    ctx.scope.own(addr);
    Ok(())
}
