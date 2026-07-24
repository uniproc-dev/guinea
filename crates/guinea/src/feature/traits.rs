use crate::lifecycle_tracker::AppLifecycle;
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::UiThreadToken;
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

/// Given (by reference) to a segment's `Page::install` and, through it, to the
/// domain `install` loaders it calls. Carries the segment's `Scope` (where the
/// feature's cells/actors live) and a UI-thread token for constructing actors.
/// `install` is plain synchronous setup, never `async` ("a future never
/// crosses the contract"): a loader constructs its actor with state starting
/// at `Load::Loading` and returns immediately; resolved data arrives later as
/// an ordinary push into `Scope` (`ctx.port::<R>()`), one delivery mechanism
/// for both initial and subsequent updates.
#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: std::rc::Rc<guinea_core::scope::Scope>,
    pub token: UiThreadToken,
    /// The window-scoped `EventBus` - for actors reaching other features
    /// within the same window (they have no other way to reach across
    /// feature boundaries). Cross-*window* communication is a separate,
    /// deliberate opt-in via `GlobalEventBus::instance()`, not this.
    pub event_bus: std::rc::Rc<EventBus>,
}

impl FeatureInitContext {
    /// The port sink for reducer `R`: everything an actor pushes through it
    /// lands in `R`'s cell via `reduce`. Removes the "clone the scope, build a
    /// `move |msg| scope.push::<R>(msg)` closure" ceremony from every loader.
    /// Satisfies any `#[port]` trait through the blanket `impl<F: Fn(Msg)>`.
    pub fn port<R: guinea_core::scope::Reducer>(&self) -> impl Fn(R::Push) + 'static {
        let scope = self.scope.clone();
        move |msg| scope.push::<R>(msg)
    }

    /// Reducer `R`'s actions-storage object - the same `Rc<R::Actions>` a view
    /// resolves through `use_reducer`. A loader passes `&ctx.actions::<R>()`
    /// straight into its binder (`Rc` deref-coerces to `&R::Actions`) to wire
    /// the view -> domain handlers, without reaching through `ctx.scope`.
    pub fn actions<R: guinea_core::scope::Reducer>(&self) -> std::rc::Rc<R::Actions> {
        self.scope.actions::<R>()
    }

    /// Subscribes on the window-scoped bus and hands the subscription's
    /// lifetime to this segment's `Scope`, so it's dropped automatically on
    /// teardown - one call instead of "subscribe, then remember to
    /// `own_subscription` yourself".
    pub fn subscribe<M: Event>(&self, callback: impl Fn(M) + 'static) {
        let id = self.event_bus.subscribe_fn(callback);
        self.scope.own_subscription(self.event_bus.clone(), id);
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
