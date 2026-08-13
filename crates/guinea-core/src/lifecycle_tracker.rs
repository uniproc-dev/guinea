use crate::actor::addr::Addr;
use crate::actor::event_bus::subscribe::BusSubscription;

pub trait LifecycleTracker {
    fn track_loop<T: 'static>(&self, handle: T);
    fn track_actor<A: 'static>(&self, addr: &Addr<A>);
    /// Takes ownership of a subscription, ending it at teardown.
    fn track_sub(&self, subscription: BusSubscription);

    /// Disposes the actor on shutdown.
    fn own_actor<A: 'static>(&self, addr: &Addr<A>) {
        let _ = addr;
    }
}

pub struct NullTracker;

impl LifecycleTracker for NullTracker {
    fn track_loop<T: 'static>(&self, _handle: T) {}
    fn track_actor<A: 'static>(&self, _addr: &Addr<A>) {}

    /// Nothing here owns a teardown, so the subscription is leaked rather than
    /// dropped - an untracked actor keeps receiving events, as it did before
    /// subscriptions became handles.
    fn track_sub(&self, subscription: BusSubscription) {
        subscription.leak();
    }
}
