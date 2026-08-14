use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;

/// Installs the application, once, from inside the root render function.
///
/// The same shape windows-reactor itself uses for one-time UI-thread setup:
/// the reactor owns the window and the run loop, and everything that has to
/// happen on that thread happens in the first render. guinea does not wrap
/// `App::run`, so an application builds its window exactly as it would
/// without guinea:
///
/// ```ignore
/// fn main() -> anyhow::Result<()> {
///     windows_reactor::App::new()
///         .title("app")
///         .on_exit(guinea::shutdown)
///         .render(root)
///         .map_err(|e| anyhow::anyhow!("{e:?}"))
/// }
///
/// fn root(cx: &mut RenderCx) -> Element {
///     guinea::bootstrap(cx, || GuineaApp::new().plugin(..).feature(..));
///     RouterRx::<Route>::render(cx, initial_route())
/// }
/// ```
///
/// `build` runs on the first render and never again. Pair it with
/// [`shutdown`] on `App::on_exit`, which is where teardown has to hang: the
/// reactor exits the process rather than unmounting the tree, so a cleanup
/// effect would never run.
pub fn bootstrap(cx: &mut windows_reactor::RenderCx, build: impl FnOnce() -> GuineaApp) {
    let installed = cx.use_ref(false);
    if *installed.borrow() {
        return;
    }
    *installed.borrow_mut() = true;

    crate::dispatcher::install();

    // Genuinely the UI thread: a render function only ever runs there.
    let token = UiThreadToken::dangerously_create_token_unchecked();

    // Rendering has no way to report an error - the reactor is mid-frame and
    // the process has no application yet, so there is nothing to fall back to.
    let runtime = build()
        .install(token)
        .unwrap_or_else(|err| panic!("guinea::bootstrap: install failed: {err:#}"));

    install_runtime(runtime);
}

/// Runs cleanups and reports actors that outlived them. Give this to
/// `windows_reactor::App::on_exit`.
pub fn shutdown() {
    shutdown_current();
}
