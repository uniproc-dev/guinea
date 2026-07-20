use crate::app::Window;
use crate::feature::{WindowFeature, WindowFeatureInitContext};
use forsl_core::l10n::L10nSink;
use std::marker::PhantomData;

pub struct L10nBootstrap<TWindow, S, P, PortCtor> {
    build: fn() -> S,
    port_ctor: PortCtor,
    _marker: PhantomData<(TWindow, P)>,
}

impl<TWindow, S, P, PortCtor> L10nBootstrap<TWindow, S, P, PortCtor> {
    pub fn new(build: fn() -> S, port_ctor: PortCtor) -> Self {
        Self { build, port_ctor, _marker: PhantomData }
    }
}

impl<TWindow, S, P, PortCtor> WindowFeature<TWindow> for L10nBootstrap<TWindow, S, P, PortCtor>
where
    TWindow: Window,
    P: L10nSink<S>,
    PortCtor: Fn(&TWindow) -> P,
{
    fn install(&mut self, ctx: &mut WindowFeatureInitContext<TWindow>) -> anyhow::Result<()> {
        let port = (self.port_ctor)(ctx.ui);
        port.load((self.build)());
        Ok(())
    }
}
