use guinea_app::app::{GuineaApp, install_runtime, shutdown_current};
use guinea_core::actor::UiThreadToken;

/// Bootstrapping, as a method on the application that is being bootstrapped.
///
/// An extension trait rather than an inherent method, because `GuineaApp`
/// lives in `guinea-app`, which has no backend to run on. The backend brings
/// the method with it - and a different backend brings its own, with the same
/// name and its own window type.
pub trait Bootstrap: Sized {
    /// Takes over the window: installs the application on the UI thread,
    /// renders `root`, and tears everything down on exit.
    ///
    /// `root` is a render function, the same one `windows_reactor::App::
    /// render` takes - the application builds its own tree and decides where
    /// a router, if any, goes inside it.
    ///
    /// Does not return: the reactor exits the process once the last window
    /// closes.
    fn run<F>(self, window: windows_reactor::App, root: F) -> anyhow::Result<()>
    where
        F: Fn(&mut windows_reactor::RenderCx) -> windows_reactor::Element + Send + 'static;
}

impl Bootstrap for GuineaApp {
    fn run<F>(self, window: windows_reactor::App, root: F) -> anyhow::Result<()>
    where
        F: Fn(&mut windows_reactor::RenderCx) -> windows_reactor::Element + Send + 'static,
    {
        run(self, window, root)
    }
}

/// The free-function form, for a caller that would rather not import the
/// trait.
pub fn run<F>(
    app: GuineaApp,
    window: windows_reactor::App,
    root: F,
) -> anyhow::Result<()>
where
    F: Fn(&mut windows_reactor::RenderCx) -> windows_reactor::Element + Send + 'static,
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
            RootFn(root)
        })
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}

/// The same wrapper `windows_reactor::App::render` uses on a render function,
/// which is private there. `run` needs it because it has its own root factory
/// - installation has to happen on the UI thread, before the first render.
struct RootFn<F>(F);

impl<F> windows_reactor::Component for RootFn<F>
where
    F: Fn(&mut windows_reactor::RenderCx) -> windows_reactor::Element + 'static,
{
    fn render(
        &self,
        _props: &(),
        cx: &mut windows_reactor::RenderCx,
    ) -> windows_reactor::Element {
        (self.0)(cx)
    }
}
