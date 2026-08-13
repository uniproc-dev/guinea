use std::ops::{Deref, DerefMut};

use guinea_core::actor::UiThreadToken;

use crate::lifecycle_tracker::AppLifecycle;

use super::builder::FeatureBuilder;
use super::plugin::{AppFeature, Plugin};
use super::runtime;

/// An application without a window, for testing plugins and features.
///
/// Installs run exactly as they would inside [`super::App::run`] - same
/// builder, same registry, same lifecycle - so a plugin can be exercised from
/// its own crate. Nothing here attests to being on a real UI thread: work that
/// actually touches the reactor still needs one.
pub struct TestApp {
    token: UiThreadToken,
    builder: FeatureBuilder,
}

impl TestApp {
    pub fn new() -> Self {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        Self {
            builder: FeatureBuilder::new(token.clone(), AppLifecycle::new()),
            token,
        }
    }

    pub fn install<P: Plugin>(&mut self, plugin: P) -> anyhow::Result<&mut Self> {
        self.builder.plugin(plugin)?;
        Ok(self)
    }

    pub fn install_feature<F: AppFeature>(&mut self, feature: F) -> anyhow::Result<&mut Self> {
        self.builder.feature(feature)?;
        Ok(self)
    }

    /// Runs cleanups in LIFO order and returns the actors still referenced
    /// afterwards - empty is what a correctly torn-down application looks like.
    pub fn shutdown(self) -> Vec<(&'static str, usize)> {
        runtime::teardown(&self.token, &self.builder)
    }
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for TestApp {
    type Target = FeatureBuilder;

    fn deref(&self) -> &FeatureBuilder {
        &self.builder
    }
}

impl DerefMut for TestApp {
    fn deref_mut(&mut self) -> &mut FeatureBuilder {
        &mut self.builder
    }
}
