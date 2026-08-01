use std::any::{Any, TypeId};
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::task::JoinHandle;

use crate::actor::Addr;
use crate::actor::event_bus::EventBus;
use crate::actor::event_bus::subscribe::SubscriptionId;

static NEXT_SUBSCRIBER_ID: AtomicU64 = AtomicU64::new(0);

pub struct Subscription {
    unsubscribe: Option<Box<dyn FnOnce()>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

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

pub trait Reducer: 'static {
    type State: Default + 'static;
    type Push: 'static;
    type Actions: Default + 'static;

    fn reduce(state: &mut Self::State, msg: Self::Push);
}

#[derive(Default)]
pub struct NoopActions;

struct Cell {
    state: Rc<dyn Any>,
    actions: RefCell<Option<Rc<dyn Any>>>,
    listeners: RefCell<Vec<(u64, Rc<dyn Fn()>)>>,
}

impl Cell {
    fn empty<F: Reducer>() -> Self {
        Cell {
            state: Rc::new(RefCell::new(F::State::default())),
            actions: RefCell::new(None),
            listeners: RefCell::new(Vec::new()),
        }
    }
}

#[derive(Default)]
pub struct Scope {
    cells: RefCell<HashMap<TypeId, Cell>>,
    teardowns: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        for teardown in self.teardowns.get_mut().drain(..) {
            teardown();
        }
    }
}

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state<F: Reducer>(&self) -> Rc<RefCell<F::State>> {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        cell.state
            .clone()
            .downcast::<RefCell<F::State>>()
            .expect("Scope cell type mismatch for this TypeId - unreachable, keyed by F")
    }
    
    pub fn peek<F: Reducer>(&self) -> Option<Rc<RefCell<F::State>>> {
        let cells = self.cells.borrow();
        let cell = cells.get(&TypeId::of::<F>())?;
        Some(
            cell.state
                .clone()
                .downcast::<RefCell<F::State>>()
                .expect("Scope cell type mismatch for this TypeId - unreachable, keyed by F"),
        )
    }

    /// Sets `F`'s starting state instead of `F::State::default()`. Call
    /// before anything else touches `F` in this scope - overwrites any
    /// existing cell, dropping its listeners.
    pub fn seed<F: Reducer>(&self, state: F::State) {
        self.cells.borrow_mut().insert(
            TypeId::of::<F>(),
            Cell {
                state: Rc::new(RefCell::new(state)),
                actions: RefCell::new(None),
                listeners: RefCell::new(Vec::new()),
            },
        );
    }

    pub fn actions<F: Reducer>(&self) -> Rc<F::Actions> {
        let mut cells = self.cells.borrow_mut();
        let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
        let mut slot = cell.actions.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(F::Actions::default()) as Rc<dyn Any>);
        }
        slot.clone()
            .unwrap()
            .downcast::<F::Actions>()
            .expect("Scope cell actions-storage type mismatch - unreachable, keyed by F")
    }

    pub fn push<F: Reducer>(&self, msg: F::Push) {
        let state = self.state::<F>();
        {
            let mut state = state.borrow_mut();
            F::reduce(&mut state, msg);
        }

        let listeners: Vec<Rc<dyn Fn()>> = {
            let cells = self.cells.borrow();
            cells
                .get(&TypeId::of::<F>())
                .map(|cell| cell.listeners.borrow().iter().map(|(_, f)| f.clone()).collect())
                .unwrap_or_default()
        };
        for listener in listeners {
            listener();
        }
    }
    pub fn subscribe<F: Reducer>(
        self: &Rc<Self>,
        callback: impl Fn() + 'static,
    ) -> Subscription {
        let id = NEXT_SUBSCRIBER_ID.fetch_add(1, Ordering::Relaxed);
        {
            let mut cells = self.cells.borrow_mut();
            let cell = cells.entry(TypeId::of::<F>()).or_insert_with(Cell::empty::<F>);
            cell.listeners.borrow_mut().push((id, Rc::new(callback)));
        }

        let store = self.clone();
        Subscription {
            unsubscribe: Some(Box::new(move || {
                if let Some(cell) = store.cells.borrow_mut().get_mut(&TypeId::of::<F>()) {
                    cell.listeners.borrow_mut().retain(|(lid, _)| *lid != id);
                }
            })),
        }
    }

    pub fn own_subscription(&self, bus: Rc<EventBus>, id: SubscriptionId) {
        self.teardowns
            .borrow_mut()
            .push(Box::new(move || bus.unsubscribe(id)));
    }

    /// Returns a shallow clone of every reducer state currently held by this
    /// scope, keyed by the reducer's `TypeId`. Used by the router to cache
    /// page state in memory while the page is not mounted.
    pub fn snapshot_states(&self) -> HashMap<TypeId, Rc<dyn Any>> {
        self.cells
            .borrow()
            .iter()
            .map(|(type_id, cell)| (*type_id, cell.state.clone()))
            .collect()
    }

    /// Pre-populates reducer cells with previously cached states. This is the
    /// inverse of [`snapshot_states`]: when a page is remounted, restoring its
    /// state before `install` runs lets the page find ready data instead of
    /// defaults.
    pub fn restore_states(&self, states: HashMap<TypeId, Rc<dyn Any>>) {
        let mut cells = self.cells.borrow_mut();
        for (type_id, state) in states {
            cells.insert(
                type_id,
                Cell {
                    state,
                    actions: RefCell::new(None),
                    listeners: RefCell::new(Vec::new()),
                },
            );
        }
    }

    /// Binds any [`Teardown`] resource to this scope's lifetime: it is torn
    /// down when the scope is.
    pub fn own<R: Teardown>(&self, resource: R) {
        self.teardowns.borrow_mut().push(Box::new(move || resource.teardown()));
    }
}

/// A resource whose lifetime can be bound to a [`Scope`] via [`Scope::own`].
/// Implement per resource kind; the "blanket" over arbitrary `T` is the
/// [`DropGuard`] newtype, so specialized teardowns never overlap.
pub trait Teardown: 'static {
    fn teardown(self);
}

impl Teardown for JoinHandle<()> {
    fn teardown(self) {
        self.abort();
    }
}

impl<A: 'static> Teardown for Addr<A> {
    fn teardown(self) {
        self.dispose();
        drop(self);
    }
}

impl Teardown for crate::signal::SignalSubscription {
    fn teardown(self) {
        drop(self);
    }
}

/// Blanket teardown for resources that just need dropping
/// (`scope.own(DropGuard(resource))` when no specialized impl exists).
pub struct DropGuard<T: 'static>(pub T);

impl<T: 'static> Teardown for DropGuard<T> {
    fn teardown(self) {
        drop(self.0);
    }
}

pub struct GlobalScope;

impl GlobalScope {
    pub fn instance() -> Rc<Scope> {
        thread_local! {
            static SCOPE: Rc<Scope> = Rc::new(Scope::new());
        }
        SCOPE.with(|scope| scope.clone())
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

    impl Reducer for Counter {
        type State = CounterState;
        type Push = CounterMsg;
        type Actions = NoopActions;

        fn reduce(state: &mut Self::State, msg: Self::Push) {
            match msg {
                CounterMsg::Set(v) => state.value = v,
            }
        }
    }

    #[test]
    fn state_survives_unmount_remount_within_a_live_store() {
        let store = Scope::new();

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

    #[test]
    fn push_notifies_subscribers_and_unsubscribe_stops_it() {
        let store = Rc::new(Scope::new());
        let seen = Rc::new(RefCell::new(Vec::new()));

        let seen_for_sub = seen.clone();
        let store_for_sub = store.clone();
        let sub = store.subscribe::<Counter>(move || {
            seen_for_sub.borrow_mut().push(store_for_sub.state::<Counter>().borrow().value);
        });

        store.push::<Counter>(CounterMsg::Set(1));
        store.push::<Counter>(CounterMsg::Set(2));
        assert_eq!(*seen.borrow(), vec![1, 2]);

        drop(sub);
        store.push::<Counter>(CounterMsg::Set(3));
        assert_eq!(
            *seen.borrow(),
            vec![1, 2],
            "no further notifications after the Subscription is dropped"
        );
    }

    #[tokio::test]
    async fn dropping_the_store_aborts_owned_tasks() {
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let flag = ran_to_completion.clone();

        let store = Scope::new();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            flag.store(true, Ordering::SeqCst);
        });
        store.own(handle);

        // Scope teardown, well before the task's sleep elapses.
        drop(store);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !ran_to_completion.load(Ordering::SeqCst),
            "task should have been aborted when its owning cell was dropped"
        );
    }

    #[test]
    fn own_actor_disposes_the_registry_entry_on_cell_drop() {
        let token = crate::actor::UiThreadToken::dangerously_create_token_unchecked();
        let addr = Addr::new_scoped((), token);
        let counter = addr.strong_count_ptr();

        let store = Scope::new();
        store.own(addr.clone());
        drop(addr);

        // Only our own local `counter` handle plus the REGISTRY's - the
        // Scope hasn't torn down yet, so it hasn't disposed it.
        assert!(
            Rc::strong_count(&counter) > 1,
            "REGISTRY should still hold the actor alive while its Scope is alive"
        );

        drop(store);

        assert_eq!(
            Rc::strong_count(&counter),
            1,
            "dropping the Scope's cell should dispose the REGISTRY entry, \
             leaving only this test's own counter handle"
        );
    }
}
