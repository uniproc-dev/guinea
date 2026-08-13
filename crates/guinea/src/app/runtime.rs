use std::cell::RefCell;

use guinea_core::actor::UiThreadToken;

use crate::feature::AppFeatureDeinitContext;

use super::builder::FeatureBuilder;

pub(crate) type RouteHook = Box<dyn Fn(Option<&str>, &str)>;

pub(crate) struct AppRuntime {
    pub(crate) token: UiThreadToken,
    pub(crate) builder: FeatureBuilder,
    pub(crate) route_hooks: Vec<RouteHook>,
    pub(crate) last_route: RefCell<Option<String>>,
}

thread_local! {
    static RUNTIME: RefCell<Option<AppRuntime>> = const { RefCell::new(None) };
}

pub(crate) fn install(runtime: AppRuntime) {
    RUNTIME.with(|slot| *slot.borrow_mut() = Some(runtime));
}

pub(crate) fn route_changed(to: &str) {
    RUNTIME.with(|slot| {
        let slot = slot.borrow();
        let Some(runtime) = slot.as_ref() else { return };

        let from = runtime.last_route.borrow().clone();
        for hook in &runtime.route_hooks {
            hook(from.as_deref(), to);
        }
        *runtime.last_route.borrow_mut() = Some(to.to_string());
    });
}

/// Runs cleanups and reports actors that outlived them. Called from the
/// reactor's exit callback, on the UI thread.
pub(crate) fn shutdown_current() {
    let Some(runtime) = RUNTIME.with(|slot| slot.borrow_mut().take()) else {
        return;
    };

    teardown(&runtime.token, &runtime.builder);
}

pub(crate) fn teardown(
    token: &UiThreadToken,
    builder: &FeatureBuilder,
) -> Vec<(&'static str, usize)> {
    let lifecycle = builder.lifecycle().clone();
    let mut ctx = AppFeatureDeinitContext {
        token: token.clone(),
        reactor: crate::feature::FeatureContext::reactor(&**builder),
        shared: crate::feature::FeatureContext::shared(&**builder),
    };

    lifecycle.shutdown(token, &mut ctx)
}
