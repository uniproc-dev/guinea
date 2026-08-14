//! Reading a reducer's state without being a UI hook.
//!
//! A hook is one way to consume state, not the only one: a reconciler wants a
//! snapshot plus a subscription scoped to the component instance, an immediate
//! mode backend polls once a frame, and a retained-property backend wants a
//! synchronous first call followed by pushes. [`ReducerBinding`] is the
//! primitive all three are built on.

use std::cell::{Ref, RefCell};
use std::rc::{Rc, Weak};

use crate::scope::{DropGuard, Reducer, Scope, Subscription};

/// A handle to one reducer's state and actions inside one scope.
///
/// Holds the scope weakly and the state cell strongly: reading keeps working
/// after the scope is torn down (the last frame of a page being navigated away
/// from still renders), while a binding stored in an actor cannot keep its own
/// scope alive. That cycle - `Scope -> cells -> listeners -> closure ->
/// Rc<Scope>` - is the reason this type exists rather than each consumer
/// wiring it by hand.
pub struct ReducerBinding<R: Reducer> {
    owner: Weak<Scope>,
    state: Rc<RefCell<R::State>>,
    actions: Rc<R::Actions>,
}

impl<R: Reducer> Clone for ReducerBinding<R> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            state: self.state.clone(),
            actions: self.actions.clone(),
        }
    }
}

impl<R: Reducer> ReducerBinding<R> {
    pub(crate) fn new(scope: &Rc<Scope>) -> Self {
        Self {
            owner: Rc::downgrade(scope),
            state: scope.state::<R>(),
            actions: scope.actions::<R>(),
        }
    }

    /// Borrows the state in place - no clone, for backends that read every
    /// frame.
    pub fn peek(&self) -> Ref<'_, R::State> {
        self.state.borrow()
    }

    /// A snapshot, for backends that memoise on equality.
    pub fn get(&self) -> R::State
    where
        R::State: Clone,
    {
        self.state.borrow().clone()
    }

    pub fn actions(&self) -> Rc<R::Actions> {
        self.actions.clone()
    }

    /// Sends a message to the reducer. A no-op once the owning scope is gone.
    pub fn push(&self, msg: R::Push) {
        if let Some(scope) = self.owner.upgrade() {
            scope.push::<R>(msg);
        }
    }

    /// Calls `f` after every change, until the returned [`Subscription`] is
    /// dropped.
    ///
    /// The caller owns the lifetime, which is what a reconciler needs: the
    /// subscription must end with the component instance, not with the scope,
    /// or unmounted components accumulate as live listeners.
    pub fn on_change(&self, f: impl Fn(&R::State) + 'static) -> Subscription {
        let Some(scope) = self.owner.upgrade() else {
            return Subscription::inert();
        };

        let state = self.state.clone();
        scope.subscribe::<R>(move || f(&state.borrow()))
    }

    /// Like [`Self::on_change`], but the subscription lives as long as the
    /// scope - for backends with no effect system to hang a cleanup on.
    pub fn on_change_owned(&self, f: impl Fn(&R::State) + 'static) {
        let Some(scope) = self.owner.upgrade() else {
            return;
        };

        let subscription = self.on_change(f);
        scope.own(DropGuard(subscription));
    }

    /// Calls `f` immediately with the current state, then after every change.
    pub fn bind(&self, f: impl Fn(&R::State) + 'static) -> Subscription {
        f(&self.state.borrow());
        self.on_change(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::NoopActions;
    use std::cell::Cell;

    struct Counter;

    impl Reducer for Counter {
        type State = u32;
        type Push = u32;
        type Group = ();
        type Actions = NoopActions;

        fn reduce(state: &mut u32, msg: u32) {
            *state += msg;
        }
    }

    #[test]
    fn a_binding_does_not_keep_its_scope_alive() {
        let scope = Rc::new(Scope::new());
        let binding = scope.binding::<Counter>();

        assert_eq!(Rc::strong_count(&scope), 1, "the binding holds a Weak");

        binding.on_change_owned(|_| {});
        drop(scope);

        assert_eq!(*binding.peek(), 0, "the state cell outlives the scope");
        binding.push(1);
        assert_eq!(*binding.peek(), 0, "pushing into a dead scope is a no-op");
    }

    #[test]
    fn subscribing_does_not_keep_the_scope_alive_either() {
        let scope = Rc::new(Scope::new());
        let _subscription = scope.binding::<Counter>().on_change(|_| {});

        assert_eq!(
            Rc::strong_count(&scope),
            1,
            "an outstanding subscription must not raise the scope's strong count"
        );
    }

    #[test]
    fn dropping_a_subscription_after_its_scope_is_harmless() {
        let subscription = {
            let scope = Rc::new(Scope::new());
            scope.binding::<Counter>().on_change(|_| {})
        };

        drop(subscription);
    }

    #[test]
    fn on_change_sees_every_push_until_dropped() {
        let scope = Rc::new(Scope::new());
        let binding = scope.binding::<Counter>();

        let seen = Rc::new(Cell::new(0u32));
        let recorder = seen.clone();
        let subscription = binding.on_change(move |state| recorder.set(*state));

        binding.push(2);
        binding.push(3);
        assert_eq!(seen.get(), 5);

        drop(subscription);
        binding.push(4);
        assert_eq!(seen.get(), 5, "no delivery after the subscription is dropped");
    }

    #[test]
    fn bind_calls_back_before_anything_changes() {
        let scope = Rc::new(Scope::new());
        let binding = scope.binding::<Counter>();
        binding.push(7);

        let seen = Rc::new(Cell::new(None));
        let recorder = seen.clone();
        let _subscription = binding.bind(move |state| recorder.set(Some(*state)));

        assert_eq!(seen.get(), Some(7));
    }
}
