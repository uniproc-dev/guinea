use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;
use guinea_core::actor::event_bus::GlobalEventBus;

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
    let id = GlobalEventBus::instance().subscribe_fn(move |_: ProcessKilled| {
        scope.push::<TabsReducer>(TabsMsg::GlobalKill);
    });
    ctx.scope.own_subscription(GlobalEventBus::instance(), id);

    Ok(())
}
