use std::fmt::Debug;
use std::rc::Rc;

use crate::lifecycle_tracker::AppLifecycle;
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::registry::DebugRegistry;
use guinea_core::actor::{Addr, Handler, ManagedActor, UiThreadToken};
use guinea_core::actor::event_bus::{EventBus, GlobalEventBus, GlobalEventBusSubscription};
use guinea_core::actor::event_bus::subscribe::Event;
use guinea_core::scope::{Reducer, Scope, Teardown};

pub struct AppFeatureInitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
    pub tracker: &'a AppLifecycle,
}

pub struct AppFeatureDeinitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
}

#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: Rc<Scope>,
    pub token: UiThreadToken,
    pub event_bus: Rc<EventBus>,
    pub store: amethystate::DefaultStore,
    pub debug_registry: Rc<DebugRegistry>,
}

impl FeatureInitContext {

    pub fn port<R: Reducer>(&self) -> impl Fn(R::Push) + 'static {
        // Weak, not strong: an actor storing this port closure must not keep
        // its owning page scope alive. Otherwise Scope -> Addr -> Actor -> Port
        // -> Scope forms an Rc cycle and the actor leaks across navigation.
        let scope = Rc::downgrade(&self.scope);
        move |msg| {
            if let Some(scope) = scope.upgrade() {
                scope.push::<R>(msg);
            }
        }
    }

    pub fn actions<R: Reducer>(&self) -> Rc<R::Actions> {
        self.scope.actions::<R>()
    }

    /// Seeds `R`'s reducer state for this scope, bypassing `R::State::default()`.
    /// Call this first thing in `install` - right after a synchronous settings
    /// read - so the first render already reflects real data instead of the
    /// default followed by an async actor round-trip. See [`Scope::seed`].
    pub fn seed_reducer<R: Reducer>(&self, state: R::State) {
        self.scope.seed::<R>(state);
    }

    pub fn subscribe<M: Event>(&self, callback: impl Fn(M) + 'static) {
        let id = self.event_bus.subscribe_fn(callback);
        self.scope.own_subscription(self.event_bus.clone(), id);
    }

    pub fn subscribe_global<M: Event>(&self, callback: impl Fn(M) + 'static) {
        let id = GlobalEventBus::subscribe_fn(callback);
        self.scope.own(GlobalEventBusSubscription(id));
    }

    pub fn subscribe_on_global_bus<A, M>(&self, addr: Addr<A>)
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        let id = GlobalEventBus::subscribe(addr);
        self.scope.own(GlobalEventBusSubscription(id));
    }

    pub fn spawn_actor<A: ManagedActor + Debug + 'static>(&self, actor: A) -> Addr<A> {
        let addr = Addr::new_managed_scoped(actor, self.token.clone());
        let id = addr.id();
        self.debug_registry.register(&addr);
        // Unregister from the window-wide debug snapshot registry before the
        // scope-owned Addr is disposed, otherwise the registry's cloned Addr
        // keeps the actor alive after navigation.
        self.scope.own(DebugRegistration {
            id,
            registry: self.debug_registry.clone(),
        });
        self.scope.own(addr.clone());
        addr
    }
}

struct DebugRegistration {
    id: usize,
    registry: Rc<DebugRegistry>,
}

impl Teardown for DebugRegistration {
    fn teardown(self) {
        self.registry.unregister(self.id);
    }
}

pub trait AppFeature {
    fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()>;
}

pub trait IntoAppFeature {
    type Feature: AppFeature + 'static;
    fn into_feature(self) -> Self::Feature;
}

pub struct AppFeatureFn {
    f: fn(&mut AppFeatureInitContext) -> anyhow::Result<()>,
}

impl<T: AppFeature + 'static> IntoAppFeature for T {
    type Feature = T;
    fn into_feature(self) -> Self::Feature {
        self
    }
}

impl AppFeature for AppFeatureFn {
    fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()> {
        (self.f)(ctx)
    }
}

impl IntoAppFeature for fn(&mut AppFeatureInitContext) -> anyhow::Result<()> {
    type Feature = AppFeatureFn;
    fn into_feature(self) -> Self::Feature {
        AppFeatureFn { f: self }
    }
}
