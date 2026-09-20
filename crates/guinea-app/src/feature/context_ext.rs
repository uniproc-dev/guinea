use std::panic::Location;

use crate::timers::{self, Period, Timer};
use guinea_core::SharedState;
use guinea_core::actor::{Addr, Handler, ManagedActor, UiThreadToken};
use guinea_core::lifecycle_tracker::LifecycleTracker;

pub trait FeatureContext {
    type Tracker: LifecycleTracker;
    fn token(&self) -> UiThreadToken;
    fn tracker(&self) -> &Self::Tracker;
    fn shared(&self) -> &SharedState;

    /// The feature being installed, which owns what is spawned now.
    fn installing(&self) -> Option<&'static str> {
        None
    }
}

pub struct ActorBuilder<'a, Ctx: FeatureContext, A: ManagedActor> {
    ctx: &'a mut Ctx,
    actor: A,
}

impl<'a, Ctx: FeatureContext, A: ManagedActor + std::fmt::Debug> ActorBuilder<'a, Ctx, A> {
    pub fn build(self) -> Addr<A> {
        let addr = Addr::new_managed(self.actor, self.ctx.token(), self.ctx.tracker());
        self.ctx.tracker().own_actor(&addr);
        crate::app::actors::register(&addr, self.ctx.installing());
        addr
    }
}

pub trait ContextActorExt: FeatureContext + Sized {
    fn spawn<A: ManagedActor + std::fmt::Debug>(&mut self, actor: A) -> Addr<A> {
        self.actor_builder(actor).build()
    }

    fn actor_builder<A: ManagedActor>(&mut self, actor: A) -> ActorBuilder<'_, Self, A> {
        ActorBuilder { ctx: self, actor }
    }
}

impl<Ctx: FeatureContext> ContextActorExt for Ctx {}

/// Timers that live as long as the application.
pub trait ContextTimersExt: FeatureContext {
    /// Sends `message()` to `addr` every `period`.
    ///
    /// Known by where it is called from, and by the feature being installed.
    #[track_caller]
    fn every<A, M>(
        &mut self,
        period: impl Into<Period>,
        addr: &Addr<A>,
        message: impl Fn() -> M + 'static,
    ) -> Timer
    where
        A: Handler<M>,
        M: Send + 'static,
    {
        let addr = addr.clone();
        start(self, Location::caller(), period.into(), move || addr.send(message()))
    }

    /// Runs `run` every `period`.
    #[track_caller]
    fn repeat(&mut self, period: impl Into<Period>, run: impl FnMut() + 'static) -> Timer {
        start(self, Location::caller(), period.into(), run)
    }
}

fn start<Ctx: FeatureContext + ?Sized>(
    ctx: &Ctx,
    place: &'static Location<'static>,
    period: Period,
    run: impl FnMut() + 'static,
) -> Timer {
    let (ticking, timer) = timers::start(place, ctx.installing(), None, period, run);
    ctx.tracker().track_loop(ticking);

    timer
}

impl<Ctx: FeatureContext> ContextTimersExt for Ctx {}
