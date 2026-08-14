use std::sync::Arc;

use guinea_router::router::Router;

use crate::{RouterRoot, WinUi};

/// A window that knows what it renders.
///
/// `run` takes one of these rather than a window and a root side by side: the
/// two are configured together, so they read together.
pub trait Windowed {
    type Root: windows_reactor::Component + Send + 'static;

    fn into_parts(self) -> (windows_reactor::App, Self::Root);
}

/// Says what a window renders. An extension trait because `windows_reactor::
/// App` is not ours to add methods to.
pub trait WindowExt: Sized {
    /// Renders `root` - a plain component, for an application with one screen.
    fn root<C>(self, root: C) -> RootedWindow<C>
    where
        C: windows_reactor::Component + Send + 'static;

    /// Renders the route tree, starting at `initial`.
    fn router<R>(self, initial: R) -> RoutedWindow<R>;
}

impl WindowExt for windows_reactor::App {
    fn root<C>(self, root: C) -> RootedWindow<C>
    where
        C: windows_reactor::Component + Send + 'static,
    {
        RootedWindow { window: self, root }
    }

    fn router<R>(self, initial: R) -> RoutedWindow<R> {
        RoutedWindow {
            window: self,
            initial,
            setup: Vec::new(),
        }
    }
}

pub struct RootedWindow<C> {
    window: windows_reactor::App,
    root: C,
}

impl<C> Windowed for RootedWindow<C>
where
    C: windows_reactor::Component + Send + 'static,
{
    type Root = C;

    fn into_parts(self) -> (windows_reactor::App, C) {
        (self.window, self.root)
    }
}

/// A window rendering a route tree, and the place to configure the router that
/// drives it.
pub struct RoutedWindow<R> {
    window: windows_reactor::App,
    initial: R,
    setup: Vec<Arc<dyn Fn(&Router<WinUi>) + Send + Sync>>,
}

impl<R> RoutedWindow<R> {
    /// Runs `hook` after each navigation, with the previous path (`None` for
    /// the first) and the new one.
    pub fn on_route_change(
        self,
        hook: impl Fn(Option<&str>, &str) + Send + Sync + 'static,
    ) -> Self {
        // The `Arc` is what makes this work without asking the caller for a
        // `Clone` closure: `with_router` may run for more than one router, and
        // each needs its own handle on the same hook.
        let hook = Arc::new(hook);
        self.with_router(move |router| {
            let hook = hook.clone();
            router.on_route_change(move |from, to| hook(from, to));
        })
    }

    /// Everything else the router can be told, for what has no sugar here.
    /// Runs once, when the router is created.
    pub fn with_router(mut self, setup: impl Fn(&Router<WinUi>) + Send + Sync + 'static) -> Self {
        self.setup.push(Arc::new(setup));
        self
    }
}

impl<R> Windowed for RoutedWindow<R>
where
    R: guinea_router::router::RouteChain<WinUi>
        + guinea_router::router::ToUri
        + Clone
        + PartialEq
        + Send
        + 'static,
{
    type Root = RouterRoot<R>;

    fn into_parts(self) -> (windows_reactor::App, RouterRoot<R>) {
        let setup = self.setup;
        let root = RouterRoot::at(self.initial).setup(move |router| {
            for step in &setup {
                step(router);
            }
        });
        (self.window, root)
    }
}
