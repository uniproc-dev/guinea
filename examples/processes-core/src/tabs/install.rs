use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use crate::events::ProcessKilled;

use super::contracts::{TabsMsg, TabsReducer};

pub fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
    let count = ctx.scope.peek::<TabsReducer>().map_or(0, |s| s.borrow().install_count);
    ctx.port::<TabsReducer>()(TabsMsg::Installed(count + 1));

    let port = ctx.port::<TabsReducer>();
    ctx.subscribe::<ProcessKilled>(move |ev: ProcessKilled| {
        port(TabsMsg::LocalKill(ev.name));
    });

    let port = ctx.port::<TabsReducer>();
    ctx.subscribe_global::<ProcessKilled>(move |_| {
        port(TabsMsg::GlobalKill);
    });

    Ok(())
}
