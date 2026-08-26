//! Installing the application into the loop eframe owns.
//!
//! eframe calls two things per frame: `logic`, before any drawing and even
//! while the window is hidden, and `ui`, when there is something to draw. That
//! split lands well here - the queue actors filled is drained in the first,
//! the route tree is drawn in the second.

use std::cell::RefCell;
use std::rc::Rc;

use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;
use guinea_router::router::{NavigateHandle, RouteChain, RouteSink, Router, ToUri};

use crate::{Egui, dispatcher, nav};

/// What [`run`] calls the root it opens.
pub const MAIN: &str = "main";

/// Runs the application in a window eframe opens, starting at `initial`.
///
/// ```ignore
/// guinea_eframe::run(app, "Processes", eframe::NativeOptions::default(), initial_route())
/// ```
pub fn run<R>(
    app: GuineaApp,
    title: &str,
    options: eframe::NativeOptions,
    initial: R,
) -> anyhow::Result<()>
where
    R: RouteChain<Egui> + ToUri + Clone + PartialEq + 'static,
{
    // Before any actor exists: the first thing a feature does during install
    // may already queue work back to this thread.
    dispatcher::install();

    // Genuinely this thread: it is the one that will draw, and nothing else
    // touches the router or the scopes.
    let token = UiThreadToken::dangerously_create_token_unchecked();
    install_runtime(app.install(token.clone())?);

    let router = Rc::new(Router::<Egui>::new(token));
    guinea_app::app::roots::set_label(router.root(), MAIN);

    let route = Rc::new(RefCell::new(initial.clone()));
    nav::install(NavigateHandle::new(router.clone(), {
        let route = route.clone();
        RouteSink::new(move |next: R| *route.borrow_mut() = next)
    }));

    router.navigate(initial.clone(), &initial.to_uri())?;

    let front = Frontend {
        router: router.clone(),
    };
    let outcome = eframe::run_native(
        title,
        options,
        Box::new(|cc| {
            // Now there is a context to wake: egui sleeps between frames, and
            // work finished on another thread has to ask for one.
            dispatcher::wake_with(cc.egui_ctx.clone());
            Ok(Box::new(front))
        }),
    );

    dispatcher::forget_waker();
    nav::clear();
    shutdown_current();

    outcome.map_err(|e| anyhow::anyhow!("eframe: {e}"))
}

struct Frontend {
    router: Rc<Router<Egui>>,
}

impl eframe::App for Frontend {
    /// Before the drawing, and also while the window is hidden: an actor that
    /// finished work still gets its turn on this thread.
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        dispatcher::drain();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.router.render().draw(ui);
    }
}
