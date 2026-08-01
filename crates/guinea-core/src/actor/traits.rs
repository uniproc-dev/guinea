use crate::actor::Context;
use crate::actor::event_bus::builder::EventSubscription;

pub trait Message: 'static {}

pub trait Handler<M: Message>: 'static {
    fn handle(&mut self, msg: M, ctx: &Context<Self>)
    where
        Self: Sized;
}

pub trait DirectHandler<A> {}
impl<A, M> DirectHandler<A> for M
where
    A: Handler<M> + 'static,
    M: Message,
{
}

pub trait ManagedActor: Sized + 'static {
    type Bus: EventSubscription<Self>;
    type Handlers: DirectHandler<Self>;
    type Signals;
}

pub trait AllowedSignal<M: Message> {}
impl<M: Message> AllowedSignal<M> for M {}
