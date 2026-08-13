use crate::actor::addr::Addr;
use crate::actor::event_bus::subscribe::{Event, FnSubscriber, Subscriber, SubscriptionId, UntypedSubscriber};
use crate::actor::invoke_on_ui;
use crate::actor::short_type_name;
use crate::actor::traits::Handler;
use crate::trace::{DispatchMeta, current_meta, is_scope_enabled};
use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use tracing::{debug, warn};

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
    next_id: Cell<SubscriptionId>,
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

    fn next_id(&self) -> SubscriptionId {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    pub fn subscribe<A, M>(&self, addr: Addr<A>) -> SubscriptionId
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        let type_id = TypeId::of::<M>();
        let id = self.next_id();

        *self.counts.borrow_mut().entry(type_id).or_insert(0) += 1;

        let subscriber = Box::new(Subscriber {
            id,
            addr,
            _marker: std::marker::PhantomData,
        });

        self.subscribers
            .borrow_mut()
            .entry(type_id)
            .or_insert_with(Vec::new)
            .push(subscriber);

        if is_scope_enabled("core.bus.subscribe") {
            debug!(
                event = short_type_name::<M>(),
                actor = short_type_name::<A>(),
                "bus.subscribe"
            );
        }

        id
    }

    pub fn subscribe_fn<M: Event>(&self, callback: impl Fn(M) + 'static) -> SubscriptionId {
        let type_id = TypeId::of::<M>();
        let id = self.next_id();

        *self.counts.borrow_mut().entry(type_id).or_insert(0) += 1;

        let subscriber = Box::new(FnSubscriber {
            id,
            callback: Arc::new(callback),
        });

        self.subscribers
            .borrow_mut()
            .entry(type_id)
            .or_insert_with(Vec::new)
            .push(subscriber);

        if is_scope_enabled("core.bus.subscribe") {
            debug!(event = short_type_name::<M>(), "bus.subscribe_fn");
        }

        id
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

    pub fn unsubscribe(&self, id: SubscriptionId) {
        let mut subscribers = self.subscribers.borrow_mut();
        let mut found = false;

        for (type_id, list) in subscribers.iter_mut() {
            let start_len = list.len();
            list.retain(|sub| sub.id() != id);
            let removed = start_len - list.len();

            if removed > 0 {
                found = true;
                let mut counts = self.counts.borrow_mut();
                if let Some(count) = counts.get_mut(type_id) {
                    *count = count.saturating_sub(removed);
                }
            }
        }

        if !found {
            warn!(subscription_id = id, "unsubscribe: subscription not found");
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

    pub fn subscribe<A, M>(addr: Addr<A>) -> SubscriptionId
    where
        A: Handler<M> + 'static,
        M: Event,
    {
        Self::instance().subscribe(addr)
    }

    pub fn subscribe_fn<M: Event>(callback: impl Fn(M) + 'static) -> SubscriptionId {
        Self::instance().subscribe_fn(callback)
    }

    pub fn count_subscribers<M: Event>() -> usize {
        Self::instance().count_subscribers::<M>()
    }

    pub fn has_subscribers<M: Event>() -> bool {
        Self::instance().has_subscribers::<M>()
    }

    pub fn unsubscribe(id: SubscriptionId) {
        Self::instance().unsubscribe(id);
    }
}

/// A scope-owned handle to a `GlobalEventBus` subscription. Dropping/teardown
/// unsubscribes automatically, so business code does not have to remember to
/// call `GlobalEventBus::unsubscribe` manually.
pub struct GlobalEventBusSubscription(pub SubscriptionId);

impl crate::scope::Teardown for GlobalEventBusSubscription {
    fn teardown(self) {
        GlobalEventBus::unsubscribe(self.0);
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
