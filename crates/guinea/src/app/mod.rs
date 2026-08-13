mod builder;
mod dispatcher;
mod plugin;
mod registry;
mod runtime;

#[cfg(any(test, feature = "test-utils"))]
mod harness;

pub use builder::{FeatureBuilder, PluginBuilder};
pub use plugin::{AppFeature, Plugin};

#[cfg(any(test, feature = "test-utils"))]
pub use harness::TestApp;

pub(crate) use runtime::route_changed;

use guinea_core::actor::UiThreadToken;

use crate::lifecycle_tracker::AppLifecycle;

type Registration = Box<dyn FnOnce(&mut FeatureBuilder) -> anyhow::Result<()> + Send>;
type ReadyHook = Box<dyn FnOnce(&mut FeatureBuilder) + Send>;
type RouteHook = Box<dyn Fn(Option<&str>, &str) + Send>;

/// The application, described before there is a UI thread to build it on.
///
/// Everything registered here is replayed once inside [`App::run`], on the UI
/// thread, in registration order.
#[derive(Default)]
pub struct App {
    registrations: Vec<Registration>,
    ready: Vec<ReadyHook>,
    route_hooks: Vec<RouteHook>,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plugin<P: Plugin>(mut self, plugin: P) -> Self {
        self.registrations
            .push(Box::new(move |app| app.plugin(plugin).map(|_| ())));
        self
    }

    pub fn feature<F: AppFeature>(mut self, feature: F) -> Self {
        self.registrations
            .push(Box::new(move |app| app.feature(feature).map(|_| ())));
        self
    }

    /// Runs once everything is installed, before the first render.
    pub fn on_ready(mut self, f: impl FnOnce(&mut FeatureBuilder) + Send + 'static) -> Self {
        self.ready.push(Box::new(f));
        self
    }

    /// Runs after each successful navigation, with the previous and current
    /// paths.
    pub fn on_route_change(
        mut self,
        f: impl Fn(Option<&str>, &str) + Send + 'static,
    ) -> Self {
        self.route_hooks.push(Box::new(f));
        self
    }

    /// Takes over the window: installs everything on the UI thread, renders
    /// `root`, and tears the application down on exit.
    ///
    /// `root` is built on the UI thread, after installation. Pass
    /// [`crate::router::RouterRoot::at`] for a route-based UI, or any other
    /// component - `run` itself knows nothing about routing.
    ///
    /// Does not return - the reactor exits the process once the last window
    /// closes.
    pub fn run<C>(self, window: windows_reactor::App, root: C) -> anyhow::Result<()>
    where
        C: windows_reactor::Component + Send + 'static,
    {
        let App {
            registrations,
            ready,
            route_hooks,
        } = self;

        window
            .on_exit(runtime::shutdown_current)
            .run(move || {
                dispatcher::install();

                let token = UiThreadToken::dangerously_create_token_unchecked();
                let mut builder = FeatureBuilder::new(token.clone(), AppLifecycle::new());

                for register in registrations {
                    if let Err(err) = register(&mut builder) {
                        panic!("guinea::App: install failed: {err:#}");
                    }
                }
                for hook in ready {
                    hook(&mut builder);
                }

                runtime::install(runtime::AppRuntime {
                    token,
                    builder,
                    route_hooks: route_hooks
                        .into_iter()
                        .map(|hook| Box::new(hook) as runtime::RouteHook)
                        .collect(),
                    last_route: Default::default(),
                });

                root
            })
            .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
    }
}

#[cfg(test)]
mod tests;
