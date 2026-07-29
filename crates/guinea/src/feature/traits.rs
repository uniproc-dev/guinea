use crate::lifecycle_tracker::AppLifecycle;
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::{Addr, ManagedActor, UiThreadToken};
use guinea_core::actor::event_bus::EventBus;
use guinea_core::actor::event_bus::subscribe::Event;

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
    pub scope: std::rc::Rc<guinea_core::scope::Scope>,
    pub token: UiThreadToken,
    pub event_bus: std::rc::Rc<EventBus>,
    pub store: amethystate::DefaultStore,
    pub debug_registry: std::rc::Rc<guinea_core::actor::registry::DebugRegistry>,
}

impl FeatureInitContext {

    pub fn port<R: guinea_core::scope::Reducer>(&self) -> impl Fn(R::Push) + 'static {
        let scope = self.scope.clone();
        move |msg| scope.push::<R>(msg)
    }

    pub fn actions<R: guinea_core::scope::Reducer>(&self) -> std::rc::Rc<R::Actions> {
        self.scope.actions::<R>()
    }

    pub fn subscribe<M: Event>(&self, callback: impl Fn(M) + 'static) {
        let id = self.event_bus.subscribe_fn(callback);
        self.scope.own_subscription(self.event_bus.clone(), id);
    }

    pub fn spawn_actor<A: ManagedActor + std::fmt::Debug + 'static>(&self, actor: A) -> Addr<A> {
        let addr = Addr::new_managed_scoped(actor, self.token.clone());
        self.scope.own(addr.clone());
        self.debug_registry.register(&addr);
        addr
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
