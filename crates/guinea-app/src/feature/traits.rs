use std::fmt::Debug;
use std::rc::Rc;
use std::sync::Arc;

use crate::timers::Reactor;
use anyhow::Context as _;
use guinea_core::SharedState;
use guinea_core::actor::registry::DebugRegistry;
use guinea_core::actor::{Addr, Handler, ManagedActor, UiThreadToken};
use guinea_core::actor::event_bus::{EventBus, GlobalEventBus};
use guinea_core::actor::event_bus::subscribe::Event;
use guinea_core::scope::{Reducer, Scope, Teardown};

pub struct AppFeatureDeinitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
}

#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: Rc<Scope>,
    pub ancestors: Rc<[Rc<Scope>]>,
    pub token: UiThreadToken,
    pub event_bus: Rc<EventBus>,
    pub debug_registry: Rc<DebugRegistry>,
    /// What plugins provided during application startup.
    pub services: SharedState,
}

impl FeatureInitContext {
    pub fn install<F>(&self, install: F) -> anyhow::Result<()>
    where
        F: Fn(&FeatureInitContext) -> anyhow::Result<()> + 'static,
    {
        install(self)?;
        self.scope.mark_feature_installed::<F>();
        Ok(())
    }

    pub fn inherit<F>(&self, _install: F)
    where
        F: Fn(&FeatureInitContext) -> anyhow::Result<()> + 'static,
    {
        let found = self.ancestors.iter().any(|s| s.has_feature::<F>());
        assert!(
            found,
            "inherit() found no ancestor scope that installed this feature - \
             an ancestor must call ctx.install(...) for it first"
        );
    }

    pub fn port<R: Reducer>(&self) -> impl Fn(R::Push) + 'static {
        // A feature manages exactly one reducer, wired via exactly this
        // call - `ctx.port::<R>()` inside `install()`, before any render
        // happens for this segment. That makes it a reliable, non-racy
        // "this scope owns R" signal for `resolve_owner` to check, unlike
        // guessing from whether R's state cell happens to exist yet
        // (which depends on render/actor-response timing, not ownership).
        self.scope.note_reducer_owner::<R>();

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
        // Same "this scope owns R" signal as `port` - a feature dispatching
        // through `ctx.actions::<R>()` directly (without ever calling
        // `ctx.port::<R>()`) is just as much a declaration of ownership.
        self.scope.note_reducer_owner::<R>();
        self.scope.actions::<R>()
    }

    /// Seeds `R`'s reducer state for this scope, bypassing `R::State::default()`.
    /// Call this first thing in `install` - right after a synchronous settings
    /// read - so the first render already reflects real data instead of the
    /// default followed by an async actor round-trip. See [`Scope::seed`].
    pub fn seed_reducer<R: Reducer>(&self, state: R::State) {
        self.scope.note_reducer_owner::<R>();
        self.scope.seed::<R>(state);
    }

    /// A service a plugin provided at startup.
    ///
    /// The counterpart of `PluginBuilder::provide`: this is how a page reaches
    /// the store, or anything else an application-level plugin set up.
    pub fn require<T: Send + Sync + 'static>(&self) -> anyhow::Result<Arc<T>> {
        let service = std::any::type_name::<T>();
        match self.services.try_get::<T>() {
            Ok(Some(value)) => Ok(value),
            Ok(None) => anyhow::bail!(
                "no plugin provided service `{service}` - install the plugin that \
                 provides it on the application, before the window opens"
            ),
            Err(poisoned) => Err(anyhow::Error::new(poisoned))
                .with_context(|| format!("requiring service `{service}`")),
        }
    }

    pub fn try_require<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services.get::<T>()
    }

    pub fn subscribe<M: Event>(&self, callback: impl Fn(M) + 'static) {
        self.scope
            .own_subscription(self.event_bus.subscribe_fn(callback));
    }

    pub fn subscribe_global<M: Event>(&self, callback: impl Fn(M) + 'static) {
        self.scope.own(GlobalEventBus::subscribe_fn(callback));
    }

    pub fn subscribe_on_global_bus<A, M>(&self, addr: Addr<A>)
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        self.scope.own(GlobalEventBus::subscribe::<A, M>(addr));
    }

    /// Wires every message of `R`'s group to `addr`.
    pub fn wire<R, A>(&self, addr: &Addr<A>)
    where
        R: Reducer,
        R::Actions: guinea_core::actor::group::WireTarget<A, R::Group>,
        A: 'static,
    {
        use guinea_core::actor::group::WireTarget;
        self.actions::<R>().wire_all(addr);
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
