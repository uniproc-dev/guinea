use crate::actor::addr::Addr;
use crate::actor::event_bus::subscribe::Event;
use crate::actor::event_bus::{GlobalEventBus, RpcCall, RpcRequest};
use crate::actor::traits::Handler;
use crate::lifecycle_tracker::LifecycleTracker;

pub trait EventSubscription<A> {
    fn subscribe_into(addr: Addr<A>, tracker: &impl LifecycleTracker);
}

impl<A, M> EventSubscription<A> for M
where
    M: Event,
    A: Handler<M> + 'static,
{
    fn subscribe_into(addr: Addr<A>, tracker: &impl LifecycleTracker) {
        tracker.track_sub(GlobalEventBus::instance().subscribe::<A, M>(addr));
    }
}

impl<A> EventSubscription<A> for () {
    fn subscribe_into(_: Addr<A>, _: &impl LifecycleTracker) {}
}

pub trait EventBatch<A, L: LifecycleTracker> {
    fn subscribe_batch(builder: EventBusBuilder<A, L>) -> EventBusBuilder<A, L>;
}

pub struct EventBusBuilder<'a, A: 'static, L: LifecycleTracker> {
    pub(crate) addr: Addr<A>,
    pub(crate) tracker: &'a L,
}

impl<'a, A: 'static, L: LifecycleTracker> EventBusBuilder<'a, A, L> {
    pub fn batch<T>(self) -> Self
    where
        T: EventBatch<A, L>,
    {
        T::subscribe_batch(self)
    }

    pub fn subscribe<M>(self) -> Self
    where
        M: Event,
        A: Handler<M> + 'static,
    {
        self.tracker
            .track_sub(GlobalEventBus::instance().subscribe::<A, M>(self.addr.clone()));
        self
    }

    pub fn rpc<Req>(self) -> Self
    where
        Req: RpcCall,
        A: Handler<RpcRequest<Req>> + 'static,
    {
        self.tracker.track_sub(
            GlobalEventBus::instance().subscribe::<A, RpcRequest<Req>>(self.addr.clone()),
        );
        self
    }
}

impl<A, L, M> EventBatch<A, L> for M
where
    A: Handler<M> + 'static,
    L: LifecycleTracker,
    M: Event,
{
    fn subscribe_batch(builder: EventBusBuilder<A, L>) -> EventBusBuilder<A, L> {
        builder.subscribe::<M>()
    }
}
