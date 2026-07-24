use crate::app::Window;
use crate::lifecycle_tracker::{AppLifecycle, WindowLifecycle};
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::UiThreadToken;
use guinea_core::actor::event_bus::EventBus;
use guinea_core::actor::event_bus::subscribe::Event;
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

#[derive(Clone)]
pub struct FeatureInitContext {
    pub scope: std::rc::Rc<guinea_core::scope::Scope>,
    pub token: UiThreadToken,
    pub event_bus: std::rc::Rc<EventBus>,
}

impl FeatureInitContext {
    
    pub fn port<R: guinea_core::scope::Reducer>(&self) -> impl Fn(R::Push) + 'static {
        let scope = self.scope.clone();
        move |msg| scope.push::<R>(msg)
    }

    
    pub fn actions<R: guinea_core::scope::Reducer>(&self) -> std::rc::Rc<R::Actions> {
        self.scope.actions::<R>()
    }

    pub fn subscribe<M: Event>(&self, callback: impl Fn(M) + 'static) {
        let id = self.event_bus.subscribe_fn(callback);
        self.scope.own_subscription(self.event_bus.clone(), id);
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
