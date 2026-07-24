use crate::app::Window;
use crate::lifecycle_tracker::{AppLifecycle, WindowLifecycle};
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::UiThreadToken;
use std::marker::PhantomData;

pub struct WindowFeatureInitContext<'a, TWindow: Window> {
    pub window_id: usize,
    pub ui: &'a TWindow,
    pub shared: &'a SharedState,
    pub reactor: &'a Reactor,
    pub tracker: &'a WindowLifecycle<TWindow>,
    pub token: UiThreadToken,
}

impl<'a, TWindow: Window> WindowFeatureInitContext<'a, TWindow> {
    pub fn token(&self) -> UiThreadToken {
        self.ui.new_token()
    }
}

pub struct WindowFeatureDeinitContext<'a, TWindow: Window> {
    pub ui: &'a TWindow,
    pub shared: &'a SharedState,
    pub reactor: &'a Reactor,
    pub token: UiThreadToken,
}

pub struct AppFeatureInitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
    pub tracker: &'a AppLifecycle,
}

pub struct AppFeatureDeinitContext<'a> {
    pub token: UiThreadToken,
    pub reactor: &'a Reactor,
    pub shared: &'a SharedState,
}

/// Given (by reference) to a segment's `Page::install` and, through it, to the
/// domain `install` loaders it calls. Carries the segment's `Scope` (where the
/// feature's cells/actors live) and a UI-thread token for constructing actors.
/// `install` is plain synchronous setup, never `async` ("a future never
/// crosses the contract"): a loader constructs its actor with state starting
/// at `Load::Loading` and returns immediately; resolved data arrives later as
/// an ordinary push into `Scope` (`ctx.port::<R>()`), one delivery mechanism
/// for both initial and subsequent updates.
#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: std::rc::Rc<guinea_core::scope::Scope>,
    pub token: UiThreadToken,
}

impl FeatureInitContext {
    /// The port sink for reducer `R`: everything an actor pushes through it
    /// lands in `R`'s cell via `reduce`. Removes the "clone the scope, build a
    /// `move |msg| scope.push::<R>(msg)` closure" ceremony from every loader.
    /// Satisfies any `#[port]` trait through the blanket `impl<F: Fn(Msg)>`.
    pub fn port<R: guinea_core::scope::Reducer>(&self) -> impl Fn(R::Push) + 'static {
        let scope = self.scope.clone();
        move |msg| scope.push::<R>(msg)
    }

    /// Reducer `R`'s actions-storage object - the same `Rc<R::Actions>` a view
    /// resolves through `use_reducer`. A loader passes `&ctx.actions::<R>()`
    /// straight into its binder (`Rc` deref-coerces to `&R::Actions`) to wire
    /// the view -> domain handlers, without reaching through `ctx.scope`.
    pub fn actions<R: guinea_core::scope::Reducer>(&self) -> std::rc::Rc<R::Actions> {
        self.scope.actions::<R>()
    }
}

pub trait WindowFeature<TWindow: Window> {
    fn install(&mut self, ctx: &mut WindowFeatureInitContext<TWindow>) -> anyhow::Result<()>;
}

pub trait AppFeature {
    fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()>;
}

pub trait IntoAppFeature {
    type Feature: AppFeature + 'static;
    fn into_feature(self) -> Self::Feature;
}

pub struct AppFeatureFn {
    f: fn(&mut AppFeatureInitContext) -> anyhow::Result<()>,
}

impl<T: AppFeature + 'static> IntoAppFeature for T {
    type Feature = T;
    fn into_feature(self) -> Self::Feature {
        self
    }
}

impl AppFeature for AppFeatureFn {
    fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()> {
        (self.f)(ctx)
    }
}

impl IntoAppFeature for fn(&mut AppFeatureInitContext) -> anyhow::Result<()> {
    type Feature = AppFeatureFn;
    fn into_feature(self) -> Self::Feature {
        AppFeatureFn { f: self }
    }
}

pub trait FromWindow<TWindow> {
    fn from_window(ui: &TWindow) -> Self;
}

pub trait IntoWindowFeature<TWindow: Window> {
    type Feature: WindowFeature<TWindow> + 'static;
    fn into_feature(self) -> Self::Feature;
}

// Lets `App::window_feature` also take a plain `Fn() -> F` builder (no port
// arguments), not just the `fn(&mut WindowFeatureInitContext, ports...)`
// form the macro below produces - test harnesses build features this way
// since they construct them directly rather than pulling ports off a window.
impl<TWindow, F, B> IntoWindowFeature<TWindow> for B
where
    TWindow: Window,
    F: WindowFeature<TWindow> + 'static,
    B: Fn() -> F + Clone + 'static,
{
    type Feature = F;
    fn into_feature(self) -> Self::Feature {
        self()
    }
}

macro_rules! impl_window_feature_fn {
    ($($name:ident, $($port:ident),*);*) => {
        $(
            pub struct $name<TWindow: Window, $($port),*> {
                f: fn(&mut WindowFeatureInitContext<TWindow>, $($port),*) -> anyhow::Result<()>,
                _marker: PhantomData<(TWindow, $($port),*)>,
            }

            impl<TWindow, $($port),*> WindowFeature<TWindow> for $name<TWindow, $($port),*>
            where
                TWindow: Window,
                $($port: FromWindow<TWindow> + Clone + 'static),*
            {
                fn install(&mut self, ctx: &mut WindowFeatureInitContext<TWindow>) -> anyhow::Result<()> {
                    $(
                        let $port = <$port as FromWindow<TWindow>>::from_window(ctx.ui);
                    )*
                    (self.f)(ctx, $($port),*)
                }
            }

            impl<TWindow, $($port),*> IntoWindowFeature<TWindow> for fn(&mut WindowFeatureInitContext<TWindow>, $($port),*) -> anyhow::Result<()>
            where
                TWindow: Window,
                $($port: FromWindow<TWindow> + Clone + 'static),*
            {
                type Feature = $name<TWindow, $($port),*>;
                fn into_feature(self) -> Self::Feature {
                    $name {
                        f: self,
                        _marker: PhantomData,
                    }
                }
            }
        )*
    };
}

impl_window_feature_fn! {
    WindowFeatureFn0, ;
    WindowFeatureFn1, P1;
    WindowFeatureFn2, P1, P2;
    WindowFeatureFn3, P1, P2, P3;
    WindowFeatureFn4, P1, P2, P3, P4;
    WindowFeatureFn5, P1, P2, P3, P4, P5;
    WindowFeatureFn6, P1, P2, P3, P4, P5, P6;
    WindowFeatureFn7, P1, P2, P3, P4, P5, P6, P7;
    WindowFeatureFn8, P1, P2, P3, P4, P5, P6, P7, P8
}
