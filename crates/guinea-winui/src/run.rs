use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;

use crate::window::Windowed;

/// Bootstrapping, as a method on the application that is being bootstrapped.
///
/// An extension trait rather than an inherent method, because `GuineaApp`
/// lives in `guinea-app`, which has no backend to run on. The backend brings
/// the method with it - and a different backend brings its own, with the same
/// name and its own window type.
pub trait Bootstrap: Sized {
    /// Takes over the window: installs the application on the UI thread,
    /// renders what the window says it renders, and tears everything down on
    /// exit.
    ///
    /// The window carries its own root - `.router(initial)` for a route tree,
    /// `.root(component)` for a single screen - so nothing here knows about
    /// routing.
    ///
    /// Does not return: the reactor exits the process once the last window
    /// closes.
    fn run(self, window: impl Windowed) -> anyhow::Result<()>;
}

impl Bootstrap for GuineaApp {
    fn run(self, window: impl Windowed) -> anyhow::Result<()> {
        run(self, window)
    }
}

/// The free-function form, for a caller that would rather not import the
/// trait.
pub fn run(app: GuineaApp, window: impl Windowed) -> anyhow::Result<()> {
    let (window, root) = window.into_parts();
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
