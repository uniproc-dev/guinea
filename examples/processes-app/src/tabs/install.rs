use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use crate::events::ProcessKilled;

use super::contracts::{TabsMsg, TabsReducer};

pub fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
    let count = ctx.scope.peek::<TabsReducer>().map_or(0, |s| s.borrow().install_count);
    ctx.scope.push::<TabsReducer>(TabsMsg::Installed(count + 1));
    
    let scope = ctx.scope.clone();
    ctx.subscribe::<ProcessKilled>(move |ev: ProcessKilled| {
        scope.push::<TabsReducer>(TabsMsg::LocalKill(ev.name));
    });
    
    let scope = ctx.scope.clone();
    ctx.subscribe_global::<ProcessKilled>(move |_| {
        scope.push::<TabsReducer>(TabsMsg::GlobalKill);
    });

    Ok(())
}
