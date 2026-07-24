use super::{Addr, Handler, Message};

pub struct UiBinder<'a, A: 'static, Ac> {
    addr: Addr<A>,
    actions: &'a Ac,
}

impl<'a, A: 'static, Ac> UiBinder<'a, A, Ac> {
    pub fn new(addr: &Addr<A>, actions: &'a Ac) -> Self {
        Self {
            addr: addr.clone(),
            actions,
        }
    }

    pub fn on0<M>(self, reg: impl FnOnce(&Ac, Box<dyn Fn() + 'static>), msg: M) -> Self
    where
        M: Message + Clone,
        A: Handler<M>,
    {
        let a = self.addr.clone();
        reg(self.actions, Box::new(move || a.send(msg.clone())));
        self
    }

    pub fn on1<M, T: 'static>(
        self,
        reg: impl FnOnce(&Ac, Box<dyn Fn(T) + 'static>),
        ctor: impl Fn(T) -> M + 'static,
    ) -> Self
    where
        M: Message,
        A: Handler<M>,
    {
        reg(self.actions, Box::new(self.addr.handler_with(ctor)));
        self
    }

    pub fn on2<M, T1: 'static, T2: 'static>(
        self,
        reg: impl FnOnce(&Ac, Box<dyn Fn(T1, T2) + 'static>),
        ctor: impl Fn(T1, T2) -> M + 'static,
    ) -> Self
    where
        M: Message,
        A: Handler<M>,
    {
        reg(self.actions, Box::new(self.addr.handler_with2(ctor)));
        self
    }

    pub fn raw(self, f: impl FnOnce(&Addr<A>, &Ac)) -> Self {
        f(&self.addr, self.actions);
        self
    }
}
