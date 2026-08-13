use crate::actor::addr::Addr;
use crate::actor::event_bus::subscribe::SubscriptionId;

pub trait LifecycleTracker {
    fn track_loop<T: 'static>(&self, handle: T);
    fn track_actor<A: 'static>(&self, addr: &Addr<A>);
    fn track_sub(&self, id: SubscriptionId);

    /// Disposes the actor on shutdown.
    fn own_actor<A: 'static>(&self, addr: &Addr<A>) {
        let _ = addr;
    }
}

pub struct NullTracker;

impl LifecycleTracker for NullTracker {
    fn track_loop<T: 'static>(&self, _handle: T) {}
    fn track_actor<A: 'static>(&self, _addr: &Addr<A>) {}
    fn track_sub(&self, _id: SubscriptionId) {}
}
