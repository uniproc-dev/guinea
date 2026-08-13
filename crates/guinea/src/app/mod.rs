mod builder;
mod plugin;
mod registry;

pub use builder::{FeatureBuilder, PluginBuilder};
pub use plugin::{AppFeature, Plugin};

#[cfg(test)]
mod tests;
