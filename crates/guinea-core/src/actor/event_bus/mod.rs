use crate::actor::addr::Addr;
use crate::actor::event_bus::subscribe::{
    BusSubscription, Event, FnSubscriber, Subscriber, SubscriptionId, UntypedSubscriber,
};
use crate::actor::invoke_on_ui;
use crate::actor::short_type_name;
use crate::actor::traits::Handler;
use crate::trace::{DispatchMeta, current_meta, is_scope_enabled};
use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use tracing::debug;

pub mod builder;
pub mod rpc;
pub mod subscribe;
pub use rpc::{AsyncBus, RpcCall, RpcRequest, RpcResponse};

#[cfg(feature = "test-utils")]
pub static TEST_TASK_QUEUE: std::sync::LazyLock<std::sync::Mutex<Vec<Box<dyn FnOnce() + Send>>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(feature = "test-utils")]
pub static ACTIVE_TASKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub struct EventBus {
    subscribers: RefCell<HashMap<TypeId, Vec<Box<dyn UntypedSubscriber>>>>,
    counts: RefCell<HashMap<TypeId, usize>>,
    next_id: Cell<u64>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: RefCell::new(HashMap::new()),
            counts: RefCell::new(HashMap::new()),
            next_id: Cell::new(0),
        }
    }

    fn next_id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    pub fn subscribe<A, M>(self: &Rc<Self>, addr: Addr<A>) -> BusSubscription
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        let seq = self.next_id();

        if is_scope_enabled("core.bus.subscribe") {
            debug!(
                event = short_type_name::<M>(),
                actor = short_type_name::<A>(),
                "bus.subscribe"
            );
        }

        self.insert::<M>(Box::new(Subscriber {
            seq,
            addr,
            _marker: std::marker::PhantomData,
        }))
    }

    pub fn subscribe_fn<M: Event>(
        self: &Rc<Self>,
        callback: impl Fn(M) + 'static,
    ) -> BusSubscription {
        let seq = self.next_id();

        if is_scope_enabled("core.bus.subscribe") {
            debug!(event = short_type_name::<M>(), "bus.subscribe_fn");
        }

        self.insert::<M>(Box::new(FnSubscriber {
            seq,
            callback: Arc::new(callback),
        }))
    }

    fn insert<M: Event>(self: &Rc<Self>, subscriber: Box<dyn UntypedSubscriber>) -> BusSubscription {
        let event = TypeId::of::<M>();
        let id = SubscriptionId {
            seq: subscriber.seq(),
            event,
        };

        *self.counts.borrow_mut().entry(event).or_insert(0) += 1;
        self.subscribers
            .borrow_mut()
            .entry(event)
            .or_default()
            .push(subscriber);

        BusSubscription {
            bus: Rc::downgrade(self),
            id,
        }
    }

    pub fn count_subscribers<M: Event>(&self) -> usize {
        let type_id = TypeId::of::<M>();
        *self.counts.borrow().get(&type_id).unwrap_or(&0)
    }

    pub fn has_subscribers<M: Event>(&self) -> bool {
        self.count_subscribers::<M>() > 0
    }

    pub fn publish<M: Event>(&self, msg: M) {
        let meta =
            current_meta().unwrap_or_else(|| DispatchMeta::capture_or_root("core.bus.publish"));

        if !self.has_subscribers::<M>() {
            debug!(
                parent: &meta.span,
                event = short_type_name::<M>(),
                op_id = meta.op_id,
                correlation_id = meta.correlation_id.as_deref().unwrap_or(""),
                "no subscribers"
            );
            return;
        }

        if is_scope_enabled("core.bus.publish") {
            debug!(
                parent: &meta.span,
                event = short_type_name::<M>(),
                op_id = meta.op_id,
                correlation_id = meta.correlation_id.as_deref().unwrap_or(""),
                "bus.publish"
            );
        }

        let type_id = TypeId::of::<M>();
        if let Some(subs) = self.subscribers.borrow().get(&type_id) {
            for sub in subs {
                sub.deliver(
                    Box::new(msg.clone()),
                    meta.child("core.bus.publish", None, None),
                );
            }
        }
    }

    pub(super) fn remove(&self, id: SubscriptionId) {
        let mut subscribers = self.subscribers.borrow_mut();
        let Some(list) = subscribers.get_mut(&id.event) else {
            return;
        };

        let before = list.len();
        list.retain(|sub| sub.seq() != id.seq);
        let removed = before - list.len();

        if removed > 0
            && let Some(count) = self.counts.borrow_mut().get_mut(&id.event)
        {
            *count = count.saturating_sub(removed);
        }
    }
}

pub struct GlobalEventBus;

impl GlobalEventBus {
    pub(crate) fn instance() -> Rc<EventBus> {
        thread_local! {
            static BUS: Rc<EventBus> = Rc::new(EventBus::new());
        }
        BUS.with(|bus| bus.clone())
    }

    /// Publishes the event on the UI thread's global event bus.
    ///
    /// The global event bus lives on the UI thread, so this call is redirected
    /// there via the UI dispatcher. It is safe to call from any thread.
    pub fn publish<M: Event>(msg: M) {
        invoke_on_ui(move || {
            Self::instance().publish(msg);
        });
    }

    pub fn subscribe<A, M>(addr: Addr<A>) -> BusSubscription
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        Self::instance().subscribe(addr)
    }

    pub fn subscribe_fn<M: Event>(callback: impl Fn(M) + 'static) -> BusSubscription {
        Self::instance().subscribe_fn(callback)
    }

    pub fn count_subscribers<M: Event>() -> usize {
        Self::instance().count_subscribers::<M>()
    }

    pub fn has_subscribers<M: Event>() -> bool {
        Self::instance().has_subscribers::<M>()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell as StdCell;

    crate::messages! { Ping, Pong }

    #[test]
    fn dropping_the_handle_ends_the_subscription() {
        let bus = Rc::new(EventBus::new());
        let seen = Rc::new(StdCell::new(0));

        let counter = seen.clone();
        let sub = bus.subscribe_fn(move |_: Ping| counter.set(counter.get() + 1));

        bus.publish(Ping);
        assert_eq!(seen.get(), 1);

        drop(sub);
        assert_eq!(bus.count_subscribers::<Ping>(), 0);

        bus.publish(Ping);
        assert_eq!(seen.get(), 1, "no delivery after the handle is dropped");
    }

    #[test]
    fn removal_only_touches_its_own_event() {
        let bus = Rc::new(EventBus::new());

        let ping = bus.subscribe_fn(|_: Ping| {});
        let _pong = bus.subscribe_fn(|_: Pong| {});

        drop(ping);

        assert_eq!(bus.count_subscribers::<Ping>(), 0);
        assert_eq!(bus.count_subscribers::<Pong>(), 1);
    }

    #[test]
    fn a_handle_from_one_bus_cannot_disturb_another() {
        let first = Rc::new(EventBus::new());
        let second = Rc::new(EventBus::new());

        let sub = first.subscribe_fn(|_: Ping| {});
        let _same_seq_elsewhere = second.subscribe_fn(|_: Ping| {});

        drop(sub);

        assert_eq!(first.count_subscribers::<Ping>(), 0);
        assert_eq!(
            second.count_subscribers::<Ping>(),
            1,
            "both subscriptions were the first on their own bus, and once shared a raw id"
        );
    }

    #[test]
    fn a_handle_outliving_its_bus_is_harmless() {
        let sub = {
            let bus = Rc::new(EventBus::new());
            bus.subscribe_fn(|_: Ping| {})
        };

        drop(sub);
    }

    #[test]
    fn leak_keeps_the_subscription() {
        let bus = Rc::new(EventBus::new());
        bus.subscribe_fn(|_: Ping| {}).leak();

        assert_eq!(bus.count_subscribers::<Ping>(), 1);
    }
}

#[cfg(feature = "test-utils")]
impl EventBus {
    pub fn queue_test_task(task: Box<dyn FnOnce() + Send>) {
        TEST_TASK_QUEUE.lock().unwrap().push(task);
    }
    pub fn process_queue() {
        let tasks: Vec<_> = std::mem::take(&mut *TEST_TASK_QUEUE.lock().unwrap());
        for task in tasks {
            task();
        }
    }

    pub fn is_queue_empty() -> bool {
        TEST_TASK_QUEUE.lock().unwrap().is_empty()
    }

    pub fn task_count() -> usize {
        ACTIVE_TASKS.load(std::sync::atomic::Ordering::SeqCst)
    }
}
