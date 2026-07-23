use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tokio::task::JoinHandle;

/// The view-facing shape of a feature: what a `Store` cell folds its Port
/// stream into, and how. Lives on the feature marker type produced by
/// `#[feature(...)]` - the marker is the `Store` lookup key (`TypeId`), this
/// trait is what tells the cell how to fold incoming pushes into `State`.
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
}

impl Cell {
    fn empty<F: FeatureState>() -> Self {
        Cell {
            state: Rc::new(RefCell::new(F::State::default())),
            bindings: RefCell::new(None),
            tasks: Vec::new(),
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
    /// method. Creates it with `B::default()` on first access (from whichever
    /// side, actor install or view, happens to call this first) and returns
    /// the same `Rc` afterwards, so both sides share one instance: the actor
    /// registers handlers into it at install, a view's invoke-handle calls
    /// into it whenever a binding method fires.
    pub fn bindings<F: FeatureState, B: Default + 'static>(&self) -> Rc<B> {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        let mut slot = cell.bindings.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(B::default()) as Rc<dyn Any>);
        }
        slot.clone()
            .unwrap()
            .downcast::<B>()
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
}
