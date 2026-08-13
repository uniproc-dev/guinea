//! What the compiler resolves for a feature's UI contract.
//!
//! ```text
//! messages! { pub Processes { Kill(u32), Refresh } }
//!   ├─ struct Kill, struct Refresh
//!   ├─ struct Processes
//!   ├─ impl ActionsGroup for Processes
//!   │     Dispatch = GroupDispatch<Processes>
//!   │     Members  = (Kill, (Refresh, ()))
//!   └─ impl InGroup<Processes> for Kill, for Refresh
//!
//! #[dispatch(Processes)]
//!   └─ impl Reducer  Group = Processes
//!                    Actions = <Processes as ActionsGroup>::Dispatch
//!
//! ctx.wire::<R, A>(&addr)
//!   needs R::Actions: WireTarget<A, R::Group>
//!     └─ impl for GroupDispatch<G>   needs G::Members: WireMembers<A, G>
//!         └─ (Kill, ..)              needs A: Handler<Kill>      ─┐ missing
//!             └─ (Refresh, ())       needs A: Handler<Refresh>   ─┤ #[handler]
//!                 └─ ()              recursion base               │ breaks
//!                                                                 └─ the chain
//!
//! dispatch.emit(msg)  needs M: InGroup<G>
//!   Kill       impl exists
//!   FetchDone  no impl - declared outside the group - does not compile
//! ```
//!
//! Group coverage is not checked: actors are seen one at a time, so a member
//! nobody wired surfaces at runtime as a `warn!` in `core.actor.dispatch`.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::actor::short_type_name;
use crate::actor::traits::{Handler, Message};
use crate::actor::Addr;

/// A feature's UI contract: the messages the view may emit.
pub trait ActionsGroup: 'static {
    type Dispatch: Default + 'static;
    /// Cons-list of members: `(A, (B, ()))`.
    type Members: 'static;
}

/// Proof that a message belongs to `G`.
pub trait InGroup<G: ActionsGroup>: Message {}

impl ActionsGroup for () {
    type Dispatch = crate::scope::NoopActions;
    type Members = ();
}

/// Routes `emit::<M>` to whichever actor was wired for `M`.
pub struct GroupDispatch<G> {
    senders: RefCell<HashMap<TypeId, Box<dyn Any>>>,
    _group: PhantomData<G>,
}

impl<G> Default for GroupDispatch<G> {
    fn default() -> Self {
        Self {
            senders: RefCell::new(HashMap::new()),
            _group: PhantomData,
        }
    }
}

impl<G: ActionsGroup> GroupDispatch<G> {
    pub fn register<M>(&self, sender: impl Fn(M) + 'static)
    where
        M: InGroup<G>,
    {
        let boxed: Box<dyn Any> = Box::new(Rc::new(sender) as Rc<dyn Fn(M)>);
        self.senders.borrow_mut().insert(TypeId::of::<M>(), boxed);
    }

    /// Warns rather than failing silently when nothing is wired for `M`.
    pub fn emit<M>(&self, msg: M)
    where
        M: InGroup<G>,
    {
        let senders = self.senders.borrow();
        let sender = senders
            .get(&TypeId::of::<M>())
            .and_then(|boxed| boxed.downcast_ref::<Rc<dyn Fn(M)>>());

        match sender {
            Some(sender) => sender(msg),
            None => tracing::warn!(
                target: "core.actor.dispatch",
                message = short_type_name::<M>(),
                group = short_type_name::<G>(),
                "no actor is wired for this message",
            ),
        }
    }

    pub fn is_wired<M>(&self) -> bool
    where
        M: InGroup<G>,
    {
        self.senders.borrow().contains_key(&TypeId::of::<M>())
    }
}

/// Walks a group's members, wiring each to `addr`.
pub trait WireMembers<A, G: ActionsGroup> {
    fn wire(dispatch: &GroupDispatch<G>, addr: &Addr<A>);
}

impl<A, G: ActionsGroup> WireMembers<A, G> for () {
    fn wire(_dispatch: &GroupDispatch<G>, _addr: &Addr<A>) {}
}

impl<A, G, H, T> WireMembers<A, G> for (H, T)
where
    A: Handler<H> + 'static,
    G: ActionsGroup,
    H: InGroup<G>,
    T: WireMembers<A, G>,
{
    fn wire(dispatch: &GroupDispatch<G>, addr: &Addr<A>) {
        let target = addr.clone();
        dispatch.register::<H>(move |msg| target.send(msg));
        T::wire(dispatch, addr);
    }
}

/// Ties a reducer's actions object to its group's member list.
pub trait WireTarget<A, G: ActionsGroup> {
    fn wire_all(&self, addr: &Addr<A>);
}

impl<A, G> WireTarget<A, G> for GroupDispatch<G>
where
    G: ActionsGroup,
    G::Members: WireMembers<A, G>,
{
    fn wire_all(&self, addr: &Addr<A>) {
        <G::Members as WireMembers<A, G>>::wire(self, addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::{Context, UiThreadToken};
    use std::cell::RefCell;

    crate::messages! {
        pub Probe {
            Ping(u32),
            Reset,
        }
    }

    #[allow(dead_code)]
    mod internal {
        crate::messages! { Internal }
    }

    #[derive(Debug, Default)]
    struct Counter {
        log: Rc<RefCell<Vec<String>>>,
    }

    impl Handler<Ping> for Counter {
        fn handle(&mut self, msg: Ping, _ctx: &Context<Self>) {
            self.log.borrow_mut().push(format!("ping:{}", msg.0));
        }
    }

    impl Handler<Reset> for Counter {
        fn handle(&mut self, _msg: Reset, _ctx: &Context<Self>) {
            self.log.borrow_mut().push("reset".to_string());
        }
    }

    fn spawn(log: Rc<RefCell<Vec<String>>>) -> Addr<Counter> {
        Addr::new_scoped(
            Counter { log },
            UiThreadToken::dangerously_create_token_unchecked(),
        )
    }

    type ProbeMembers = <Probe as ActionsGroup>::Members;

    #[test]
    fn wiring_a_group_routes_every_member_to_the_actor() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let addr = spawn(log.clone());

        let dispatch = GroupDispatch::<Probe>::default();
        <ProbeMembers as WireMembers<Counter, Probe>>::wire(&dispatch, &addr);

        dispatch.emit(Ping(7));
        dispatch.emit(Reset);

        assert_eq!(&*log.borrow(), &["ping:7".to_string(), "reset".to_string()]);
    }

    #[test]
    fn wire_target_covers_the_whole_group() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let addr = spawn(log.clone());

        let dispatch = GroupDispatch::<Probe>::default();
        dispatch.wire_all(&addr);

        assert!(dispatch.is_wired::<Ping>());
        assert!(dispatch.is_wired::<Reset>());

        dispatch.emit(Reset);
        assert_eq!(&*log.borrow(), &["reset".to_string()]);
    }

    #[test]
    fn emitting_an_unwired_member_warns_instead_of_panicking() {
        let dispatch = GroupDispatch::<Probe>::default();
        assert!(!dispatch.is_wired::<Ping>());
        dispatch.emit(Ping(1));
    }

    #[test]
    fn group_members_are_a_cons_list_in_declaration_order() {
        fn assert_members<T: 'static>() {
            assert_eq!(
                std::any::TypeId::of::<T>(),
                std::any::TypeId::of::<ProbeMembers>()
            );
        }
        assert_members::<(Ping, (Reset, ()))>();
    }
}
