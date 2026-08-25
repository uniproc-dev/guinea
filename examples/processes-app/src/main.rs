//! The WinUI front end. Everything that is not drawing lives in
//! `processes-core`, which the terminal front end links just the same.

mod layouts;
mod pages;
mod routes;

use routes::Route;

use guinea::Bootstrap;
use guinea::app::GuineaApp;
use processes_core::startup;

fn initial_route() -> Route {
    Route::Processes {
        context: "ubuntu".to_string(),
    }
}

fn root(cx: &mut windows_reactor::RenderCx) -> windows_reactor::Element {
    GuineaApp::new()
        .meta(guinea::app_meta!())
        .plugin(
            guinea_plugin_store::StorePlugin::for_app("guinea-processes-app-example", "settings")
                // JSON, so both front ends can run at once: redb locks its
                // file and the second one would refuse to start.
                .backend(guinea_plugin_store::amethystate::store::builder::Backend::Json),
        )
        .plugin(guinea_plugin_l10n::L10nPlugin::<processes_core::l10n::L10n>::new("en"))
        .feature(startup::Startup)
        .bootstrap(cx);

    guinea::winui::RouterRx::<Route>::render(cx, initial_route())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,guinea=debug,processes_core=debug".into()),
        )
        .init();

    windows_reactor::App::new()
        .title(guinea::app_meta!().window_title)
        .inner_size(420.0, 420.0)
        .on_exit(guinea::shutdown)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
