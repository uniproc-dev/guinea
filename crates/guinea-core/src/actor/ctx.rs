use crate::actor::addr::{Addr, REGISTRY};
use crate::actor::event_bus::{EventBus, GlobalEventBus};
use crate::actor::event_bus::subscribe::Event;
use crate::actor::traits::{Handler, Message};
use crate::actor::{AllowedSignal, ManagedActor, invoke_on_ui, short_type_name};
use crate::trace::{DispatchMeta, current_meta, install_current_meta};
use std::marker::PhantomData;
use tokio::sync::oneshot;

pub struct Context<A: 'static, M = ()> {
    pub(super) addr: Addr<A>,
    pub msg: M,
}

impl<A: 'static, M> Context<A, M> {
    pub(crate) fn new(addr: Addr<A>, msg: M) -> Self {
        Self { addr, msg }
    }

    pub fn addr(&self) -> Addr<A> {
        self.addr.clone()
    }

    /// The same context without its message.
    pub fn detach(&self) -> Context<A, ()> {
        Context {
            addr: self.addr.clone(),
            msg: (),
        }
    }

    /// Sends to this actor's own queue; drained by the same `process_queue`.
    pub fn send<Out>(&self, msg: Out)
    where
        Out: Message,
        A: Handler<Out>,
    {
        self.addr.send(msg);
    }

    pub fn publish<E>(&self, msg: E)
    where
        A: ManagedActor,
        E: Event,
        A::Signals: AllowedSignal<E>,
    {
        GlobalEventBus::instance().publish(msg);
    }

    pub fn publish_local<E>(&self, bus: &EventBus, msg: E)
    where
        A: ManagedActor,
        E: Event,
        A::Signals: AllowedSignal<E>,
    {
        bus.publish(msg);
    }

    pub fn spawn_bg<Out, Fut>(&self, fut: Fut)
    where
        Out: Message + 'static + Send,
        A: Handler<Out>,
        Fut: Future<Output = Out> + 'static + Send,
    {
        let id = self.addr.id;
        let meta = current_meta().unwrap_or_else(|| DispatchMeta::capture_or_root("core.actor.bg"));
        let span = tracing::debug_span!(
            parent: &meta.span,
            "actor.bg",
            actor = short_type_name::<A>(),
            result = short_type_name::<Out>(),
            op_id = meta.op_id,
            correlation_id = meta.correlation_id.as_deref().unwrap_or(""),
        );

        #[cfg(feature = "test-utils")]
        use crate::actor::event_bus::ACTIVE_TASKS;

        #[cfg(feature = "test-utils")]
        ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            let _meta_guard = install_current_meta(meta.clone());
            let result = {
                let _enter = span.enter();
                fut.await
            };

            let return_task = move || {
                REGISTRY.with(|reg| {
                    if let Some(boxed_addr) = reg.borrow().get(&id) {
                        if let Some(addr) = boxed_addr.downcast_ref::<Addr<A>>() {
                            addr.send_with_meta(
                                result,
                                meta.child("core.actor.bg.result", None, None),
                            );
                        }
                    }

                    #[cfg(feature = "test-utils")]
                    ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                });
            };

            invoke_on_ui(return_task);
        });
    }

    pub fn spawn_bg_detached<Fut>(&self, fut: Fut)
    where
        Fut: Future<Output = ()> + 'static + Send,
    {
        let meta = current_meta().unwrap_or_else(|| DispatchMeta::capture_or_root("core.actor.bg"));
        let span = tracing::debug_span!(
            parent: &meta.span,
            "actor.bg.detached",
            actor = short_type_name::<A>(),
            op_id = meta.op_id,
            correlation_id = meta.correlation_id.as_deref().unwrap_or(""),
        );

        #[cfg(feature = "test-utils")]
        use crate::actor::event_bus::ACTIVE_TASKS;

        #[cfg(feature = "test-utils")]
        ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            let _meta_guard = install_current_meta(meta.clone());
            {
                let _enter = span.enter();
                fut.await;
            }

            #[cfg(feature = "test-utils")]
            ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }
}

pub struct AsyncContext<A: 'static> {
    actor_id: usize,
    _phantom: PhantomData<A>,
}

impl<A: 'static> Clone for AsyncContext<A> {
    fn clone(&self) -> Self {
        Self {
            actor_id: self.actor_id,
            _phantom: PhantomData,
        }
    }
}

unsafe impl<A: 'static> Send for AsyncContext<A> {}
unsafe impl<A: 'static> Sync for AsyncContext<A> {}

impl<A: 'static> AsyncContext<A> {
    pub(crate) fn new(actor_id: usize) -> Self {
        Self {
            actor_id,
            _phantom: PhantomData,
        }
    }

    pub fn publish<M>(&self, msg: M)
    where
        A: ManagedActor,
        M: Event,
        A::Signals: AllowedSignal<M>,
    {
        GlobalEventBus::instance().publish(msg);
    }

    pub fn publish_local<M>(&self, bus: &EventBus, msg: M)
    where
        A: ManagedActor,
        M: Event,
        A::Signals: AllowedSignal<M>,
    {
        bus.publish(msg);
    }

    pub async fn apply<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut A, &Context<A>) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let id = self.actor_id;

        invoke_on_ui(move || {
            REGISTRY.with(|reg| {
                let reg_borrow = reg.borrow();
                if let Some(boxed_addr) = reg_borrow.get(&id) {
                    if let Some(addr) = boxed_addr.downcast_ref::<Addr<A>>() {
                        addr.apply(move |actor, ctx| {
                            let result = f(actor, ctx);
                            let _ = tx.send(result);
                        });
                    }
                }
            });
        });

        rx.await
            .expect("Actor target dropped or UI thread panicked")
    }

    pub fn send<M>(&self, msg: M)
    where
        M: Message + Send,
        A: Handler<M>,
    {
        let id = self.actor_id;
        let meta = current_meta()
            .unwrap_or_else(|| DispatchMeta::capture_or_root("core.actor.async.send"));

        invoke_on_ui(move || {
            REGISTRY.with(|reg| {
                if let Some(addr) = reg
                    .borrow()
                    .get(&id)
                    .and_then(|a| a.downcast_ref::<Addr<A>>())
                {
                    addr.send_with_meta(msg, meta);
                }
            });
        });
    }
}

impl<A: 'static, M> Context<A, M> {
    pub fn async_ctx(&self) -> AsyncContext<A> {
        AsyncContext::new(self.addr.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::UiThreadToken;
    use std::cell::RefCell;
    use std::rc::Rc;

    crate::messages! { First, Second }

    struct Chain {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Handler<First> for Chain {
        fn handle(&mut self, ctx: Context<Self, First>) {
            self.log.borrow_mut().push("first");
            ctx.send(Second);
        }
    }

    impl Handler<Second> for Chain {
        fn handle(&mut self, _ctx: Context<Self, Second>) {
            self.log.borrow_mut().push("second");
        }
    }

    #[test]
    fn send_from_a_handler_is_drained_by_the_same_queue() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let addr = Addr::new_scoped(
            Chain { log: log.clone() },
            UiThreadToken::dangerously_create_token_unchecked(),
        );

        addr.send(First);

        assert_eq!(&*log.borrow(), &["first", "second"]);
    }

    #[test]
    fn detach_keeps_the_address_and_drops_the_message() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let addr = Addr::new_scoped(
            Chain { log },
            UiThreadToken::dangerously_create_token_unchecked(),
        );

        let ctx = Context::new(addr.clone(), First);
        let bare = ctx.detach();

        assert_eq!(bare.addr().id(), addr.id());
    }
}
