use guinea_core::actor::{Addr, ManagedActor, UiThreadToken};
use guinea_core::lifecycle_tracker::LifecycleTracker;

pub struct AddrBuilder<'a, L: LifecycleTracker> {
    token: UiThreadToken,
    tracker: &'a L,
}

#[must_use = "ManagedAddrBuilder does nothing unless you call .finish()"]
pub struct ManagedAddrBuilder<A: ManagedActor> {
    addr: Addr<A>,
}

impl<'a, L: LifecycleTracker> AddrBuilder<'a, L> {
    pub fn new(token: UiThreadToken, tracker: &'a L) -> Self {
        Self { token, tracker }
    }

    pub fn managed<A: ManagedActor>(&self, actor: A) -> ManagedAddrBuilder<A> {
        let addr = Addr::new_managed(actor, self.token.clone(), self.tracker);
        ManagedAddrBuilder { addr }
    }
}

impl<A: ManagedActor> ManagedAddrBuilder<A> {
    pub fn finish(self) -> Addr<A> {
        self.addr
    }
}

impl<A: ManagedActor> From<ManagedAddrBuilder<A>> for Addr<A> {
    fn from(builder: ManagedAddrBuilder<A>) -> Self {
        builder.addr
    }
}
