use crate::actor::addr::Addr;
use crate::actor::short_type_name;
use crate::actor::traits::{Handler, Message};
use crate::trace::{DispatchMeta, is_scope_enabled};

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::rc::Weak;

use super::EventBus;

/// Identifies one subscription on one bus. Carries the event's `TypeId` so
/// removal goes straight to the right bucket instead of scanning every one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SubscriptionId {
    pub(super) seq: u64,
    pub(super) event: TypeId,
}

/// Undoes a subscription when dropped.
///
/// Holds the bus weakly: a live subscription never keeps its bus alive, and a
/// handle outliving its bus is a no-op rather than a panic. There is no way to
/// get the raw id out, so an unsubscribe cannot be forgotten - to deliberately
/// keep a subscription for the rest of the process, say [`Self::leak`].
pub struct BusSubscription {
    pub(super) bus: Weak<EventBus>,
    pub(super) id: SubscriptionId,
}

impl BusSubscription {
    /// Keeps the subscription alive for the rest of the process.
    pub fn leak(self) {
        std::mem::forget(self);
    }
}

impl Drop for BusSubscription {
    fn drop(&mut self) {
        if let Some(bus) = self.bus.upgrade() {
            bus.remove(self.id);
        }
    }
}

impl crate::scope::Teardown for BusSubscription {
    fn teardown(self) {
        drop(self);
    }
}

pub trait Event: Message + Send + Clone {}
impl<T: Message + Clone + Send> Event for T {}

pub trait UntypedSubscriber: 'static {
    fn deliver(&self, msg: Box<dyn Any>, meta: DispatchMeta);
    fn seq(&self) -> u64;
}

pub struct Subscriber<A: Handler<M>, M: Event> {
    pub(super) seq: u64,
    pub(super) addr: Addr<A>,
    pub(super) _marker: PhantomData<M>,
}

impl<A, M> UntypedSubscriber for Subscriber<A, M>
where
    A: Handler<M> + 'static,
    M: Event,
{
    fn deliver(&self, msg: Box<dyn Any>, meta: DispatchMeta) {
        if let Ok(concrete_msg) = msg.downcast::<M>() {
            if is_scope_enabled("core.bus.deliver") {
                tracing::debug!(
                    parent: &meta.span,
                    event = short_type_name::<M>(),
                    actor = short_type_name::<A>(),
                    op_id = meta.op_id,
                    correlation_id = meta.correlation_id.as_deref().unwrap_or(""),
                    "bus.deliver"
                );
            }
            self.addr
                .send_with_meta(*concrete_msg, meta.child("core.bus.deliver", None, None));
        }
    }
    fn seq(&self) -> u64 {
        self.seq
    }
}

pub struct FnSubscriber<M: Event> {
    pub(super) seq: u64,
    pub(super) callback: std::sync::Arc<dyn Fn(M) + 'static>,
}

impl<M: Event> UntypedSubscriber for FnSubscriber<M> {
    fn deliver(&self, msg: Box<dyn Any>, _: DispatchMeta) {
        if let Ok(concrete_msg) = msg.downcast::<M>() {
            (self.callback)(*concrete_msg);
        }
    }

    fn seq(&self) -> u64 {
        self.seq
    }
}
