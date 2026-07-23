use std::any::{Any, TypeId};
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tokio::task::JoinHandle;

use crate::actor::Addr;

/// A read-only view of a cell's live `State` - exposes `.borrow()` (a
/// `Ref`, read-only by construction) and nothing else. `Store::state`
/// returns the bare `Rc<RefCell<F::State>>` because `push`/`reduce` (Store's
/// own internals) genuinely need mutable access; anything handed to view
/// code (`PageCx::use_feature`) must NOT carry `borrow_mut` along with it -
/// `Rc<RefCell<T>>` alone doesn't stop a view from mutating state directly,
/// bypassing `reduce` and the whole exclusive-write-in-the-cell invariant
/// this design depends on. This wrapper is the difference between "a
/// reactive read handle" and "a mutable hole with extra steps."
pub struct StateHandle<T>(Rc<RefCell<T>>);

impl<T> StateHandle<T> {
    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.borrow()
    }
}

impl<T> Clone for StateHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> From<Rc<RefCell<T>>> for StateHandle<T> {
    fn from(inner: Rc<RefCell<T>>) -> Self {
        Self(inner)
    }
}

/// The view-facing shape of a feature: what a `Store` cell folds its Port
/// stream into, and how. Lives on the feature marker type produced by
/// `#[feature(...)]` - the marker is the `Store` lookup key (`TypeId`), this
/// trait is what tells the cell how to fold incoming pushes into `State`.
///
/// Deliberately knows nothing about `Bindings` (see `FeatureBindings`
/// below): a reducer's only job is "mutate `State` from an incoming `Push`
/// message" - that's a complete, self-contained concern. Dispatch (view ->
/// domain) is a wholly separate flow that a reducer has no reason to be
/// aware of, and not every feature has one (a purely read-only feature is
/// State/Push/reduce and nothing else).
pub trait FeatureState: 'static {
    /// The view-shaped snapshot a cell holds and `use_feature` reads. Default
    /// is what a freshly created cell starts with, before any push has
    /// arrived - fields that represent async data should be `Load<T>`
    /// (defaulting to `Loading`) rather than requiring a separate "not ready"
    /// signal from the cell itself.
    type State: Default + 'static;
    /// The feature's `#[port]` message type.
    type Push: 'static;

    /// Folds one incoming Port message into the cell's `State`, in place.
    /// Runs inside the cell - never in view code, never in the actor - so
    /// state survives a consumer's unmount/remount cycle and is available
    /// immediately to a late-arriving reader.
    fn reduce(state: &mut Self::State, msg: Self::Push);
}

/// The dispatch (view -> domain) side of a feature - separate from
/// `FeatureState` on purpose, and not every feature implements it: a
/// feature with no user-triggerable actions (pure display, fed only by
/// Port pushes) has no bindings and no reason to name this trait at all.
pub trait FeatureBindings: 'static {
    /// The concrete `<Feature>Bindings` struct `generate_feature_bindings_adapter`
    /// produces - one registration slot per bindings method plus `emit_*`
    /// invoke methods.
    type Bindings: Default + 'static;
}

/// One feature's slot inside a `Store`: its folded `State`, the concrete
/// registration/invoke storage `#[bindings]` generates for it, and whatever
/// background work (the async loader, `spawn_bg` tasks forwarded here by the
/// actor) is scoped to its lifetime.
struct Cell {
    // Concretely `Rc<RefCell<F::State>>` for whichever `F` owns this cell;
    // erased so `Store` can hold cells for unrelated features in one map.
    state: Rc<dyn Any>,
    // Concretely `Rc<B>` where `B` is the storage struct `#[bindings]`
    // generates for this feature (one `RefCell<Option<Box<dyn Fn(Args)>>>`
    // slot per bindings method). `None` until the actor's install code calls
    // `Store::bindings` for the first time - the actor and the view resolve
    // the *same* `Rc<B>` from here, which is the whole bridge: the actor
    // registers handlers into it, a view's invoke-handle calls into it.
    bindings: RefCell<Option<Rc<dyn Any>>>,
    tasks: Vec<JoinHandle<()>>,
    // Runs the actor's `Addr::dispose()` on teardown - see `Addr::dispose`
    // for why this is required and not automatic: the thread-local actor
    // `REGISTRY` stashes its own strong clone forever, so no `Addr<A>` clone
    // going out of scope (including this cell's own) ever drops the actor by
    // itself. `Option` because a cell can exist before any actor is
    // installed into it (e.g. a view resolving `state`/`bindings` before the
    // route loader has run).
    actor_teardown: Option<Box<dyn FnOnce()>>,
}

impl Cell {
    fn empty<F: FeatureState>() -> Self {
        Cell {
            state: Rc::new(RefCell::new(F::State::default())),
            bindings: RefCell::new(None),
            tasks: Vec::new(),
            actor_teardown: None,
        }
    }
}

impl Drop for Cell {
    fn drop(&mut self) {
        // Structured concurrency: nothing scoped to a cell outlives it.
        // Closing a route/context aborts its in-flight loader and any
        // background work the actor hung off it - no manual cancellation
        // wiring required at call sites.
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(teardown) = self.actor_teardown.take() {
            teardown();
        }
    }
}

/// A container of feature cells owning their lifecycle, keyed by feature
/// type. Not a functional unit - it does not know about dispatch, push, or
/// reduce beyond routing to the right cell. One `Store` per lifecycle-tier
/// owner (app / route segment / component); teardown is `Drop`, cascading
/// into every cell's actor and in-flight tasks.
#[derive(Default)]
pub struct Store {
    cells: RefCell<HashMap<TypeId, Cell>>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// The cell's shared state, creating it with `F::State::default()` on
    /// first access. Returns the same `Rc` across calls for as long as this
    /// `Store` is alive - a component that unmounts and remounts while its
    /// owning scope is still live sees the value it left behind, not a fresh
    /// default.
    pub fn state<F: FeatureState>(&self) -> Rc<RefCell<F::State>> {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        cell.state
            .clone()
            .downcast::<RefCell<F::State>>()
            .expect("Store cell type mismatch for this TypeId - unreachable, keyed by F")
    }

    /// The feature's bindings-storage object - the concrete struct
    /// `#[bindings]` generates, holding one registration slot per bindings
    /// method. Creates it with `F::Bindings::default()` on first access
    /// (from whichever side, actor install or view, happens to call this
    /// first) and returns the same `Rc` afterwards, so both sides share one
    /// instance: the actor registers handlers into it at install, a view's
    /// invoke-handle calls into it whenever a binding method fires. Only
    /// callable for features that actually implement `FeatureBindings` -
    /// a purely read-only feature never needs to.
    pub fn bindings<F: FeatureState + FeatureBindings>(&self) -> Rc<F::Bindings> {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        let mut slot = cell.bindings.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(F::Bindings::default()) as Rc<dyn Any>);
        }
        slot.clone()
            .unwrap()
            .downcast::<F::Bindings>()
            .expect("Store cell bindings-storage type mismatch - unreachable, keyed by F")
    }

    /// Folds one Port message into `F`'s cell, creating the cell first if
    /// this is the first push the feature has ever produced for this scope.
    pub fn push<F: FeatureState>(&self, msg: F::Push) {
        let state = self.state::<F>();
        let mut state = state.borrow_mut();
        F::reduce(&mut state, msg);
    }

    /// Ties a background task's lifetime to `F`'s cell: aborted when the
    /// cell is dropped (scope teardown), not just when the task finishes on
    /// its own. Used for the feature's async loader and any `spawn_bg`-style
    /// work the actor wants scoped to this feature rather than to itself.
    pub fn own_task<F: FeatureState>(&self, task: JoinHandle<()>) {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        cell.tasks.push(task);
    }

    /// Ties an actor's lifetime to `F`'s cell: disposed (see `Addr::dispose`)
    /// when the cell is dropped. This, not merely dropping a held `Addr`
    /// clone, is what actually makes "close a route/context" kill its
    /// actor - the thread-local actor registry otherwise keeps it alive
    /// forever regardless of how many other handles go away.
    pub fn own_actor<F: FeatureState, A: 'static>(&self, addr: Addr<A>) {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        cell.actor_teardown = Some(Box::new(move || addr.dispose()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Counter;

    #[derive(Default, Clone, PartialEq, Debug)]
    struct CounterState {
        value: i32,
    }

    enum CounterMsg {
        Set(i32),
    }

    impl FeatureState for Counter {
        type State = CounterState;
        type Push = CounterMsg;

        fn reduce(state: &mut Self::State, msg: Self::Push) {
            match msg {
                CounterMsg::Set(v) => state.value = v,
            }
        }
    }

    #[test]
    fn state_survives_unmount_remount_within_a_live_store() {
        let store = Store::new();

        // "mount" #1: read default, then a push arrives.
        let first_read = store.state::<Counter>();
        assert_eq!(first_read.borrow().value, 0);
        store.push::<Counter>(CounterMsg::Set(42));

        // "unmount": drop the only handle a view held.
        drop(first_read);

        // "remount": a fresh `use_feature` call resolves the cell again -
        // must see the value left behind, not a new default.
        let second_read = store.state::<Counter>();
        assert_eq!(second_read.borrow().value, 42);
    }

    #[tokio::test]
    async fn dropping_the_store_aborts_owned_tasks() {
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let flag = ran_to_completion.clone();

        let store = Store::new();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            flag.store(true, Ordering::SeqCst);
        });
        store.own_task::<Counter>(handle);

        // Scope teardown, well before the task's sleep elapses.
        drop(store);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !ran_to_completion.load(Ordering::SeqCst),
            "task should have been aborted when its owning cell was dropped"
        );
    }

    struct NoopTracker;
    impl crate::lifecycle_tracker::LifecycleTracker for NoopTracker {
        fn track_loop<T: 'static>(&self, _handle: T) {}
        fn track_actor<A: 'static>(&self, _addr: &Addr<A>) {}
        fn track_sub(&self, _id: crate::actor::event_bus::subscribe::SubscriptionId) {}
    }

    #[test]
    fn own_actor_disposes_the_registry_entry_on_cell_drop() {
        let token = crate::actor::UiThreadToken::dangerously_create_token_unchecked();
        let addr = Addr::new((), token, &NoopTracker);
        let counter = addr.strong_count_ptr();

        let store = Store::new();
        store.own_actor::<Counter, ()>(addr.clone());
        drop(addr);

        // Only our own local `counter` handle plus the REGISTRY's - the
        // Store hasn't torn down yet, so it hasn't disposed it.
        assert!(
            Rc::strong_count(&counter) > 1,
            "REGISTRY should still hold the actor alive while its Store is alive"
        );

        drop(store);

        assert_eq!(
            Rc::strong_count(&counter),
            1,
            "dropping the Store's cell should dispose the REGISTRY entry, \
             leaving only this test's own counter handle"
        );
    }
}
