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
use guinea_core::feature::{Claim, Exported};
use guinea_core::guard::{Ask, Verdict};
use guinea_core::scope::{DropGuard, Reducer, Scope, Teardown};

pub struct AppFeatureDeinitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
}

#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: Rc<Scope>,
    pub ancestors: Rc<[Rc<Scope>]>,
    /// Which root this feature is being installed into - the second window,
    /// or the only one. What a service shared between roots uses to tell
    /// callers apart.
    pub root: crate::app::roots::RootId,
    pub token: UiThreadToken,
    pub event_bus: Rc<EventBus>,
    pub debug_registry: Rc<DebugRegistry>,
    /// What plugins provided during application startup.
    pub services: SharedState,
}

/// A named unit with its own lifetime, its own state, and one bit saying it is
/// installed here.
///
/// The layer that joins the two halves: it claims reducers and gives them
/// something to drive them, and it is the only thing that may know about both.
/// What it publishes is [`Exports`](Feature::Exports) - everything else it
/// claims stays its own.
///
/// ```ignore
/// pub struct Processes {
///     listing: Bound<contracts::Processes>,
/// }
///
/// impl Feature for Processes {
///     type Params = str;
///     type Exports = (contracts::Processes,);
///
///     fn install(cx: &FeatureInitContext, context: &str) -> anyhow::Result<Self> {
///         let listing = cx.state::<contracts::Processes>()
///             .driven_by(|push| ProcessActor::new(context.to_string(), push, cx.event_bus.clone()));
///
///         listing.emit(Refresh);
///         Ok(Self { listing })
///     }
/// }
/// ```
///
/// It is returned rather than dropped so that a segment installing two
/// features can wire them to each other - which is usually why it installs two.
pub trait Feature: Sized + 'static {
    /// What the segment hands it - typically what the route captured.
    type Params: ?Sized;

    /// The reducers segments below may read. `()` for a feature that publishes
    /// nothing, `(A,)` for one, `(A, B)` for two.
    type Exports: Exported;

    fn install(cx: &FeatureInitContext, params: &Self::Params) -> anyhow::Result<Self>;
}

impl FeatureInitContext {
    /// Installs `F` here, and publishes what it exports.
    ///
    /// Twice for the same feature in one scope is a setup bug rather than
    /// something to merge silently, so it panics - the same rule as before,
    /// now with something real behind the bit.
    pub fn install<F: Feature>(&self, params: &F::Params) -> anyhow::Result<F> {
        self.scope.mark_feature_installed::<F>();
        F::Exports::mark(&self.scope);

        // Its own corner of the scope, so that two instances of one feature
        // answering the same action type do not become one.
        self.scope.open_section();
        let installed = F::install(self, params);
        self.scope.close_section();

        // After the body, because only the body can claim anything. An export
        // the feature never claimed would otherwise be read below as the
        // reducer's `Default`, for as long as the application runs, and look
        // exactly like a feature that has not pushed an update yet.
        if installed.is_ok()
            && let Some(name) = F::Exports::unclaimed(&self.scope)
        {
            panic!(
                "{} exports {name}, but nothing in it claimed that reducer - \
                 either claim it with `cx.state::<{name}>()`, or take it out of `Exports`",
                std::any::type_name::<F>()
            );
        }

        installed
    }

    /// Claims `R` for this scope, and says what else is true of it.
    ///
    /// The one way in. Ownership used to be a side effect of which of four
    /// calls a feature happened to make - `port`, `actions`, `wire`,
    /// `seed_reducer` - so a reducer could be claimed by accident and an actor
    /// could be left unwired without anything failing to build. Here the claim
    /// is the call, and continuing it is optional:
    ///
    /// ```ignore
    /// let processes = cx.state::<Processes>()
    ///     .driven_by(|push| ProcessActor::new(context.to_string(), push, cx.event_bus.clone()));
    ///
    /// processes.emit(Refresh);
    /// ```
    ///
    /// Ending it at [`plain`](Claim::plain) is not a half-written feature - it
    /// is state the UI owns, and `emit` on it does not compile.
    pub fn state<R: Reducer>(&self) -> Claim<'_, R> {
        Claim::new(&self.scope, &self.token)
    }

    /// Says this segment answers `M`, and how.
    ///
    /// The way in for a domain that does not use an actor - a task holding a
    /// `RefCell`, a channel, a plain closure. Nothing the UI touches can tell
    /// the difference, which is the point: how a domain implements its logic
    /// is its own business.
    ///
    /// `actor!` calls this for every handler it lists, so a feature with an
    /// actor never writes it by hand.
    pub fn answers<M: guinea_core::actor::traits::Message>(
        &self,
        answer: impl Fn(M) + 'static,
    ) {
        self.scope.answers(answer);
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

    /// Reacts to what happens to `R`, wherever `R` lives.
    ///
    /// The coherence rule between two pieces of state that reference each
    /// other rather than nest: a cursor into a list the domain refreshes has
    /// to hear that the list was replaced, and a rename is not a replacement.
    /// The update itself is what tells those apart, so this is handed the
    /// update rather than told that something moved.
    ///
    /// Runs before anything is asked to redraw, and the subscription is owned
    /// by this scope - the rule dies with the segment that declared it.
    pub fn observe<R: Reducer>(&self, callback: impl Fn(&R::Update) + 'static) {
        let owner = if self.scope.has_feature::<R>() {
            self.scope.clone()
        } else {
            self.ancestors
                .iter()
                .rev()
                .find(|scope| scope.exports::<R>())
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "observing {} here found no scope that owns it: this segment did not \
                         claim it, and no ancestor exported it",
                        std::any::type_name::<R>()
                    )
                })
        };

        self.scope.own(DropGuard(owner.observe::<R>(callback)));
    }

    /// Asked before this segment is torn down by a navigation.
    ///
    /// Declared here, on the way in, because on the way out the scope exists
    /// and the guard can read its own state - which is what "unsaved changes"
    /// is. Entering is the asymmetric case: there is nothing to read yet, so
    /// an enter guard belongs in the route declaration instead.
    pub fn on_leave(&self, guard: impl Fn() -> Verdict + 'static) {
        self.scope.on_leave(guard);
    }

    /// Refuse to leave while `dirty` says so.
    ///
    /// No question and no dialog - for the case where leaving is simply not
    /// allowed yet. When the user should get a say, use
    /// [`confirm_leave`](Self::confirm_leave).
    pub fn block_leave_while<R: Reducer>(&self, dirty: impl Fn(&R) -> bool + 'static) {
        let state = self.scope.state::<R>();
        self.scope.on_leave(move || {
            if dirty(&state.borrow()) {
                Verdict::Block
            } else {
                Verdict::Allow
            }
        });
    }

    /// Ask before leaving while `dirty` says so.
    ///
    /// `question` is a closure rather than a value because it is built at the
    /// moment of asking: the language may have changed since install, and so
    /// may whatever the text names.
    pub fn confirm_leave<R: Reducer>(
        &self,
        dirty: impl Fn(&R) -> bool + 'static,
        question: impl Fn(&R) -> Ask + 'static,
    ) {
        let state = self.scope.state::<R>();
        self.scope.on_leave(move || {
            let state = state.borrow();
            if dirty(&state) {
                Verdict::ask(question(&state))
            } else {
                Verdict::Allow
            }
        });
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
