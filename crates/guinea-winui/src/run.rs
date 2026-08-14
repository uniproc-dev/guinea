use guinea_app::app::{App, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;

/// Takes over the window: installs the application on the UI thread, renders
/// `root`, and tears everything down on exit.
///
/// `root` is built on the UI thread, after installation. Pass
/// [`crate::RouterRoot::at`] for a route-based UI, or any other
/// component - nothing here knows about routing.
///
/// Does not return: the reactor exits the process once the last window closes.
pub fn run<C>(app: App, window: windows_reactor::App, root: C) -> anyhow::Result<()>
where
    C: windows_reactor::Component + Send + 'static,
{
    window
        .on_exit(shutdown_current)
        .run(move || {
            crate::dispatcher::install();

            // Genuinely the UI thread: this is the reactor's own root factory.
            let token = UiThreadToken::dangerously_create_token_unchecked();

            // The factory's return value goes to WinUI as an HRESULT, so an
            // installation error cannot be handed back to the caller.
            let runtime = app
                .install(token)
                .unwrap_or_else(|err| panic!("guinea::run: install failed: {err:#}"));

            install_runtime(runtime);
            root
        })
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
