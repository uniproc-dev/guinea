//! Opening a window, and taking it apart again.
//!
//! This is new. The old adapter had no run layer at all - `App::new().render(f)`
//! took a bare render function with nowhere to put a `GuineaApp`, so installing
//! the application had to happen *inside the first render*, through a
//! `Bootstrap` trait the caller had to remember to chain. The consequence was
//! that this backend, alone among the five, never labelled its root: there was
//! no place to do it.
//!
//! `App::run_component::<C>(input)` gives that place back. Installing happens
//! in the root component's `create`, teardown in its `Drop`, and the shape
//! matches every other backend's `run`.

use std::cell::{Cell, RefCell};

use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;
use guinea_router::router::RouteChain;
use windows_reactor::{Component, ComponentContext, View, ViewContext, WindowVisuals};

use crate::winui::{RouterRoot, WinUi};

/// The label this backend gives its first window, matching the other four.
pub const MAIN: &str = "main";

thread_local! {
    /// The application, between [`run`] and the root component's `create`.
    ///
    /// `App::run_component` takes only `C::Input`, and `Input` has to be
    /// `Clone + PartialEq` - which a `GuineaApp` is not, being a recipe full of
    /// boxed plugins. So it is handed over on the side, on the one thread that
    /// will read it, and taken exactly once.
    static PENDING: RefCell<Option<GuineaApp>> = const { RefCell::new(None) };

    /// How many windows are standing.
    ///
    /// The application is installed once per UI thread and torn down once, but
    /// there are as many roots as there are windows. Without counting, closing
    /// the second window would run cleanups for the first one too.
    static STANDING: Cell<usize> = const { Cell::new(0) };
}

/// A second window, showing the same route tree from `initial`.
///
/// ```ignore
/// cx.open_window(guinea_winui::window(
///     Window::new().title("processes (2)").client_size(420.0, 420.0),
///     Route::Processes { context: context() },
/// ));
/// ```
///
/// Its own router, and therefore its own root: scopes, event bus and debug
/// registry are the window's, and go when it does. What it shares with the
/// first window is the application - plugins are installed once per thread.
pub fn window<R>(window: Window, initial: R) -> View
where
    R: RouteChain<WinUi> + Clone + PartialEq + 'static,
{
    View::component::<Root<R>>(Opening {
        window,
        initial: Starting::new(move || initial),
    })
}

/// Runs `app` in a window, starting at `initial`.
///
/// ```ignore
/// guinea_winui::run(
///     GuineaApp::new().plugin(StorePlugin::for_app("app", "settings")).feature(Startup),
///     Window::new().title("Processes").client_size(420.0, 420.0),
///     initial_route,
/// )
/// ```
///
/// `initial` is a closure rather than a value for the same reason as in the
/// other backends: where an application starts is often something only the
/// installed plugins know, and they are not installed until the component is
/// created.
pub fn run<R>(
    app: GuineaApp,
    window: Window,
    initial: impl FnOnce() -> R + 'static,
) -> anyhow::Result<()>
where
    R: RouteChain<WinUi> + Clone + PartialEq + 'static,
{
    PENDING.with(|pending| *pending.borrow_mut() = Some(app));

    windows_reactor::App::run_component::<Root<R>>(Opening {
        window,
        initial: Starting::new(initial),
    })
    .map_err(|error| anyhow::anyhow!("windows-reactor: {error}"))
}

/// How the window looks, declared by the application and applied by the root.
///
/// Reactor moved this out of an `App` builder and into each window's root
/// component, which is the better place for it: a second window is a second
/// component with a title of its own.
#[derive(Clone, PartialEq)]
pub struct Window {
    title: String,
    size: Option<(f64, f64)>,
}

impl Window {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            size: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn client_size(mut self, width: f64, height: f64) -> Self {
        self.size = Some((width, height));
        self
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

/// The closure that says where to start, in something `Input` can hold.
///
/// `Input` must be `Clone + PartialEq`, and a `FnOnce` is neither. It is only
/// ever read once, by `create`, so the box is taken out on first use and the
/// two compare equal for as long as they both exist - there is exactly one of
/// these per window, and re-publishing the root must not read as a change.
struct Starting<R>(RefCell<Option<Box<dyn FnOnce() -> R>>>);

impl<R> Starting<R> {
    fn new(initial: impl FnOnce() -> R + 'static) -> Self {
        Self(RefCell::new(Some(Box::new(initial))))
    }

    fn take(&self) -> Option<R> {
        self.0.borrow_mut().take().map(|start| start())
    }
}

impl<R> Clone for Starting<R> {
    /// Cloning hands the closure on rather than duplicating it: only one of
    /// the copies can start the window, and it is whichever `create` reads.
    fn clone(&self) -> Self {
        Self(RefCell::new(self.0.borrow_mut().take()))
    }
}

impl<R> PartialEq for Starting<R> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, PartialEq)]
struct Opening<R> {
    window: Window,
    initial: Starting<R>,
}

/// The window's root: the application, the chrome, and the route tree.
struct Root<R: RouteChain<WinUi> + Clone + PartialEq + 'static> {
    route: R,
}

impl<R> Component for Root<R>
where
    R: RouteChain<WinUi> + Clone + PartialEq + 'static,
{
    type Input = Opening<R>;
    type Message = ();

    fn create(input: &Opening<R>, _cx: &ComponentContext<Self>) -> Self {
        // Guarded per UI thread, not per window: a second window is a second
        // root component, and installing twice would re-run every plugin -
        // opening the store's database again, for one, which fails outright.
        if !guinea_app::app::is_installed()
            && let Some(app) = PENDING.with(|pending| pending.borrow_mut().take())
        {
            // Genuinely the UI thread: a component is created on the one
            // thread that draws.
            let token = UiThreadToken::dangerously_create_token_unchecked();
            let runtime = app
                .install(token)
                .unwrap_or_else(|error| panic!("guinea: installing the application: {error:#}"));
            install_runtime(runtime);
        }

        STANDING.with(|standing| standing.set(standing.get() + 1));

        Self {
            route: input
                .initial
                .take()
                .expect("the window's starting route is read once, here"),
        }
    }

    fn view(&self, input: &Opening<R>, cx: &mut ViewContext<Self>) -> View {
        cx.window_title(input.window.title.clone());
        if let Some((width, height)) = input.window.size {
            cx.window_visuals(WindowVisuals::new().client_size(width, height));
        }

        View::component::<RouterRoot<R>>(self.route.clone())
    }

    fn update(&mut self, _message: (), _cx: &ComponentContext<Self>) {}
}

impl<R: RouteChain<WinUi> + Clone + PartialEq + 'static> Drop for Root<R> {
    /// Where teardown finally hangs.
    ///
    /// The old adapter had to put it on `App::on_exit`, and said why: the
    /// reactor exited the process rather than unmounting the tree, so a
    /// cleanup effect would never run. Both halves of that changed - a closing
    /// window drops its component tree, and `on_exit` is gone - so cleanup
    /// belongs here, where the window's own lifetime ends.
    ///
    /// The last window's, though. The application is one per UI thread however
    /// many windows show it, and tearing it down when the second one closes
    /// would take the first one's actors with it.
    fn drop(&mut self) {
        let last = STANDING.with(|standing| {
            let left = standing.get().saturating_sub(1);
            standing.set(left);
            left == 0
        });

        if last {
            shutdown_current();
        }
    }
}
