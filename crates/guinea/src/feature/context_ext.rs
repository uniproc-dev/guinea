use crate::reactor::{LoopHandle, Reactor};
use guinea_core::SharedState;
use guinea_core::actor::{Addr, Handler, ManagedActor, Message, UiThreadToken};
use guinea_core::lifecycle_tracker::LifecycleTracker;

pub trait FeatureContext {
    type Tracker: LifecycleTracker;
    fn token(&self) -> UiThreadToken;
    fn tracker(&self) -> &Self::Tracker;
    fn reactor(&self) -> &Reactor;
    fn shared(&self) -> &SharedState;
}


pub struct ActorBuilder<'a, Ctx: FeatureContext, A: ManagedActor> {
    ctx: &'a mut Ctx,
    actor: A,
}

impl<'a, Ctx: FeatureContext, A: ManagedActor> ActorBuilder<'a, Ctx, A> {
    pub fn build(self) -> Addr<A> {
        let addr = Addr::new_managed(self.actor, self.ctx.token(), self.ctx.tracker());
        self.ctx.tracker().own_actor(&addr);
        addr
    }
}

pub trait ContextActorExt: FeatureContext + Sized {
    fn spawn<A: ManagedActor>(&mut self, actor: A) -> Addr<A> {
        self.actor_builder(actor).build()
    }

    fn actor_builder<A: ManagedActor>(&mut self, actor: A) -> ActorBuilder<'_, Self, A> {
        ActorBuilder { ctx: self, actor }
    }
}

impl<Ctx: FeatureContext> ContextActorExt for Ctx {}

pub trait ContextReactorExt: FeatureContext {
    fn spawn_periodic_send<A, M>(
        &mut self,
        addr: &Addr<A>,
        interval: impl Fn() -> u64 + 'static,
        active: impl Fn() -> bool + 'static,
        msg_factory: impl Fn() -> M + Send + 'static,
    ) where
        A: Handler<M>,
        M: Message + Send + 'static,
    {
        let addr = addr.clone();
        let handle: LoopHandle = self.reactor().add_loop(interval, active, move || {
            addr.send(msg_factory());
        });
        self.tracker().track_loop(handle);
    }

    fn spawn_heartbeat<A, M>(
        &mut self,
        addr: &Addr<A>,
        interval: impl Fn() -> u64 + 'static,
        msg_factory: impl Fn() -> M + Send + 'static,
    ) where
        A: Handler<M>,
        M: Message + Send + 'static,
    {
        self.spawn_periodic_send(addr, interval, || true, msg_factory);
    }
}

impl<Ctx: FeatureContext> ContextReactorExt for Ctx {}

