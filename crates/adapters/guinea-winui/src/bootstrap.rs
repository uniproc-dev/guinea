use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;

/// Installs the application, once, from inside the root render function.
///
/// Ends the same chain that describes it:
///
/// ```ignore
/// GuineaApp::new()
///     .plugin(StorePlugin::for_app("app", "settings"))
///     .feature(Startup)
///     .bootstrap(cx);
/// ```
///
/// The same shape windows-reactor itself uses for one-time UI-thread setup:
/// the reactor owns the window and the run loop, and everything that has to
/// happen on that thread happens in the first render. guinea does not wrap
/// `App::run`, so an application builds its window exactly as it would
/// without guinea:
///
/// The description is rebuilt on every render of this function and thrown
/// away after the first - the recipe is cheap, and paying for it buys a call
/// order that reads the same way the rest of the builder does. Only the first
/// one is installed. Pair it with
/// [`shutdown`] on `App::on_exit`, which is where teardown has to hang: the
/// reactor exits the process rather than unmounting the tree, so a cleanup
/// effect would never run.
pub trait Bootstrap {
    fn bootstrap(self, cx: &mut windows_reactor::RenderCx);
}

impl Bootstrap for GuineaApp {
    fn bootstrap(self, cx: &mut windows_reactor::RenderCx) {
        bootstrap(cx, self);
    }
}

fn bootstrap(_cx: &mut windows_reactor::RenderCx, app: GuineaApp) {
    // Guarded per UI thread, not per component: an application that opens a
    // second window renders the same root there, and installing twice would
    // re-run every plugin - opening the store's database a second time, for
    // one, which fails outright.
    if guinea_app::app::is_installed() {
        return;
    }

    crate::dispatcher::install();

    // Genuinely the UI thread: a render function only ever runs there.
    let token = UiThreadToken::dangerously_create_token_unchecked();

    // Rendering has no way to report an error - the reactor is mid-frame and
    // the process has no application yet, so there is nothing to fall back to.
    let runtime = app
        .install(token)
        .unwrap_or_else(|err| panic!("guinea::bootstrap: install failed: {err:#}"));

    install_runtime(runtime);
}

/// Runs cleanups and reports actors that outlived them. Give this to
/// `windows_reactor::App::on_exit`.
pub fn shutdown() {
    shutdown_current();
}
