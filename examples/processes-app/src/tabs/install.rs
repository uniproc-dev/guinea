use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;

use super::contracts::TabsReducer;

/// No actor - just this feature's own reducer, in the Scope that
/// `Router::navigate` keeps alive across `Processes <-> Services` (only the
/// leaf position tears down and reinstalls, not this ancestor).
pub fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
    let count = ctx.scope.peek::<TabsReducer>().map_or(0, |s| s.borrow().install_count);
    ctx.scope.push::<TabsReducer>(count + 1);
    Ok(())
}
