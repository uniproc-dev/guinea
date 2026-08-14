use std::cell::RefCell;

use guinea_core::SharedState;
use guinea_core::actor::UiThreadToken;

use crate::feature::AppFeatureDeinitContext;

use super::builder::FeatureBuilder;

pub type RouteHook = Box<dyn Fn(Option<&str>, &str)>;

/// An installed application: everything the recipe built, plus the hooks that
/// outlive installation. Held on the UI thread until the process exits.
pub struct AppRuntime {
    pub(crate) token: UiThreadToken,
    pub(crate) builder: FeatureBuilder,
    pub(crate) route_hooks: Vec<RouteHook>,
    pub(crate) last_route: RefCell<Option<String>>,
}

thread_local! {
    static RUNTIME: RefCell<Option<AppRuntime>> = const { RefCell::new(None) };
}

/// Hands the runtime to the thread that will tear it down. Call once, from
/// the backend adapter, after [`crate::app::App::install`].
pub fn install_runtime(runtime: AppRuntime) {
    RUNTIME.with(|slot| *slot.borrow_mut() = Some(runtime));
}

/// The services plugins provided during installation.
///
/// Empty when there is no installed application - a router built directly in a
/// test, say. Callers get "nothing provided that" rather than a panic, which is
/// the same answer they would get from an application that installed no
/// plugins.
pub fn app_services() -> SharedState {
    RUNTIME.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|runtime| crate::feature::FeatureContext::shared(&*runtime.builder).clone())
            .unwrap_or_default()
    })
}

pub fn route_changed(to: &str) {
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
pub fn shutdown_current() {
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
