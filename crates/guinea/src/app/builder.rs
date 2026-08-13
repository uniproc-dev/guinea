use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Context as _;
use guinea_core::SharedState;
use guinea_core::actor::event_bus::GlobalEventBus;
use guinea_core::actor::event_bus::subscribe::Event;
use guinea_core::actor::{Addr, Handler, UiThreadToken};
use guinea_core::lifecycle_tracker::LifecycleTracker;

use crate::feature::{AppFeatureDeinitContext, FeatureContext};
use crate::lifecycle_tracker::AppLifecycle;
use crate::reactor::Reactor;

use super::plugin::{AppFeature, ErasedFeature, ErasedPlugin, Plugin};
use super::registry::{Admission, Registry, Unit};

/// What a plugin may do during installation.
pub struct PluginBuilder {
    token: UiThreadToken,
    reactor: Reactor,
    shared: SharedState,
    lifecycle: AppLifecycle,
    registry: Rc<RefCell<Registry>>,
}

/// Everything a plugin may do, plus this application's own wiring.
pub struct FeatureBuilder {
    inner: PluginBuilder,
}

impl PluginBuilder {
    pub(crate) fn new(token: UiThreadToken, lifecycle: AppLifecycle) -> Self {
        Self {
            token,
            reactor: Reactor::new(),
            shared: SharedState::new(),
            lifecycle,
            registry: Rc::new(RefCell::new(Registry::default())),
        }
    }

    pub(crate) fn lifecycle(&self) -> &AppLifecycle {
        &self.lifecycle
    }

    pub fn plugin<P: Plugin>(&mut self, plugin: P) -> anyhow::Result<&mut Self> {
        self.install_plugin(Box::new(plugin))?;
        Ok(self)
    }

    pub fn provide<T: Send + Sync + 'static>(&self, value: T) -> &Self {
        self.provide_arc(Arc::new(value))
    }

    pub fn provide_arc<T: Send + Sync + 'static>(&self, value: Arc<T>) -> &Self {
        if self.shared.insert_arc(value).is_some() {
            tracing::warn!(
                service = std::any::type_name::<T>(),
                by = self.registry.borrow().current(),
                "service replaced"
            );
        }
        self
    }

    pub fn require<T: Send + Sync + 'static>(&self) -> anyhow::Result<Arc<T>> {
        let service = std::any::type_name::<T>();
        let who = self.registry.borrow().current();

        match self.shared.try_get::<T>() {
            Ok(Some(value)) => Ok(value),
            Ok(None) => anyhow::bail!(
                "`{who}` requires service `{service}`, but nothing provided it - \
                 install the plugin that provides it first, usually by calling \
                 `b.plugin(..)?` at the top of `{who}`"
            ),
            Err(poisoned) => Err(anyhow::Error::new(poisoned))
                .with_context(|| format!("`{who}` requires service `{service}`")),
        }
    }

    pub fn try_require<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.shared.get::<T>()
    }

    pub fn subscribe_global<M: Event>(&self, callback: impl Fn(M) + 'static) -> &Self {
        self.lifecycle
            .track_sub(GlobalEventBus::subscribe_fn(callback));
        self
    }

    pub fn subscribe_actor<A, M>(&self, addr: &Addr<A>) -> &Self
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        self.lifecycle
            .track_sub(GlobalEventBus::subscribe::<A, M>(addr.clone()));
        self
    }

    pub fn on_cleanup(
        &self,
        f: impl for<'a> FnOnce(&mut AppFeatureDeinitContext<'a>) -> anyhow::Result<()> + 'static,
    ) -> &Self {
        self.lifecycle.on_cleanup(f);
        self
    }

    fn install_plugin(&mut self, plugin: Box<dyn ErasedPlugin>) -> anyhow::Result<()> {
        let (id, concrete) = (plugin.id(), plugin.concrete());

        match self.registry.borrow().admit_plugin(id, concrete)? {
            Admission::AlreadyInstalled => return Ok(()),
            Admission::Proceed => {}
        }

        self.registry.borrow_mut().enter(Unit::Plugin(id));
        let outcome = plugin
            .build_boxed(self)
            .with_context(|| format!("plugin `{id}` failed to build"));

        let mut registry = self.registry.borrow_mut();
        registry.leave();
        if outcome.is_ok() {
            registry.mark_plugin(id, concrete);
        }
        outcome
    }
}

impl FeatureBuilder {
    pub(crate) fn new(token: UiThreadToken, lifecycle: AppLifecycle) -> Self {
        Self {
            inner: PluginBuilder::new(token, lifecycle),
        }
    }

    pub fn feature<F: AppFeature>(&mut self, feature: F) -> anyhow::Result<&mut Self> {
        self.install_feature(Box::new(feature))?;
        Ok(self)
    }

    fn install_feature(&mut self, feature: Box<dyn ErasedFeature>) -> anyhow::Result<()> {
        let (name, concrete) = (feature.name(), feature.concrete());

        match self.registry.borrow().admit_feature(concrete, name) {
            Admission::AlreadyInstalled => return Ok(()),
            Admission::Proceed => {}
        }

        self.registry.borrow_mut().enter(Unit::Feature(name));
        let outcome = feature
            .install_boxed(self)
            .with_context(|| format!("feature `{name}` failed to install"));

        let mut registry = self.registry.borrow_mut();
        registry.leave();
        if outcome.is_ok() {
            registry.mark_feature(concrete);
        }
        outcome
    }
}

impl Deref for FeatureBuilder {
    type Target = PluginBuilder;

    fn deref(&self) -> &PluginBuilder {
        &self.inner
    }
}

impl DerefMut for FeatureBuilder {
    fn deref_mut(&mut self) -> &mut PluginBuilder {
        &mut self.inner
    }
}

impl FeatureContext for PluginBuilder {
    type Tracker = AppLifecycle;

    fn token(&self) -> UiThreadToken {
        self.token.clone()
    }

    fn tracker(&self) -> &AppLifecycle {
        &self.lifecycle
    }

    fn reactor(&self) -> &Reactor {
        &self.reactor
    }

    fn shared(&self) -> &SharedState {
        &self.shared
    }
}
