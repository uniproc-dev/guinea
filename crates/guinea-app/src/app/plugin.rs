use std::any::TypeId;

use super::builder::{FeatureBuilder, PluginBuilder};

/// Reusable, application-agnostic: the application knows the plugin, never the
/// other way round.
pub trait Plugin: Send + 'static {
    /// Identity for diagnostics and for installing at most once.
    const ID: &'static str;

    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()>;
}

/// This application's own wiring; sees everything a plugin sees and more.
pub trait AppFeature: Send + 'static {
    fn install(self, app: &mut FeatureBuilder) -> anyhow::Result<()>;
}

pub(crate) trait ErasedPlugin: Send {
    fn id(&self) -> &'static str;
    fn concrete(&self) -> TypeId;
    fn build_boxed(self: Box<Self>, app: &mut PluginBuilder) -> anyhow::Result<()>;
}

impl<P: Plugin> ErasedPlugin for P {
    fn id(&self) -> &'static str {
        P::ID
    }

    fn concrete(&self) -> TypeId {
        TypeId::of::<P>()
    }

    fn build_boxed(self: Box<Self>, app: &mut PluginBuilder) -> anyhow::Result<()> {
        Plugin::build(*self, app)
    }
}

pub(crate) trait ErasedFeature: Send {
    fn name(&self) -> &'static str;
    fn concrete(&self) -> TypeId;
    fn install_boxed(self: Box<Self>, app: &mut FeatureBuilder) -> anyhow::Result<()>;
}

impl<F: AppFeature> ErasedFeature for F {
    fn name(&self) -> &'static str {
        std::any::type_name::<F>()
    }

    fn concrete(&self) -> TypeId {
        TypeId::of::<F>()
    }

    fn install_boxed(self: Box<Self>, app: &mut FeatureBuilder) -> anyhow::Result<()> {
        AppFeature::install(*self, app)
    }
}
