mod builder;
mod plugin;
mod registry;
mod runtime;

#[cfg(any(test, feature = "test-utils"))]
mod harness;

pub use builder::{FeatureBuilder, PluginBuilder};
pub use plugin::{AppFeature, Plugin};

#[cfg(any(test, feature = "test-utils"))]
pub use harness::TestApp;

pub use runtime::{
    AppRuntime, app_services, install_runtime, shutdown_current,
};

use guinea_core::actor::UiThreadToken;

use crate::lifecycle_tracker::AppLifecycle;

pub type Registration = Box<dyn FnOnce(&mut FeatureBuilder) -> anyhow::Result<()> + Send>;
pub type ReadyHook = Box<dyn FnOnce(&mut FeatureBuilder) + Send>;

/// The application, described before there is a UI thread to build it on.
///
/// Everything registered here is replayed once by the backend adapter, on the
/// UI thread, in registration order. Named `GuineaApp` rather than `App`
/// because every backend has an `App` of its own and the two appear in the
/// same `main`.
#[derive(Default)]
pub struct GuineaApp {
    registrations: Vec<Registration>,
    ready: Vec<ReadyHook>,
}

impl GuineaApp {
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

    /// Replays the recipe: installs every plugin and feature in registration
    /// order, then runs the ready hooks.
    ///
    /// For backend adapters. The caller must already be on the UI thread -
    /// that is what `token` attests to - and must hand the result to
    /// [`install_runtime`] so teardown can find it.
    pub fn install(self, token: UiThreadToken) -> anyhow::Result<AppRuntime> {
        let mut builder = FeatureBuilder::new(token.clone(), AppLifecycle::new());

        for register in self.registrations {
            register(&mut builder)?;
        }
        for hook in self.ready {
            hook(&mut builder);
        }

        Ok(AppRuntime { token, builder })
    }
}

#[cfg(test)]
mod tests;
