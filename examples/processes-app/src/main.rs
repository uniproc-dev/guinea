mod events;
mod l10n;
mod metrics;
mod processes;
mod routes;
mod services;
mod startup;
mod tabs;

use routes::Route;

guinea::meta::manifest!();

use guinea::app::GuineaApp;
use guinea::Bootstrap;

fn initial_route() -> Route {
    Route::Processes {
        context: "ubuntu".to_string(),
    }
}

fn root(cx: &mut windows_reactor::RenderCx) -> windows_reactor::Element {
    GuineaApp::new()
        .meta(guinea::app_meta!())
        .plugin(guinea_plugin_store::StorePlugin::for_app(
            "guinea-processes-app-example",
            "settings",
        ))
        .plugin(guinea_plugin_l10n::L10nPlugin::<l10n::L10n>::new("en"))
        .feature(startup::Startup)
        .bootstrap(cx);

    guinea::winui::RouterRx::<Route>::render(cx, initial_route())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,guinea=debug,guinea_processes_app_example=debug".into()),
        )
        .init();

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    windows_reactor::App::new()
        .title(WINDOW_TITLE)
        .inner_size(420.0, 420.0)
        .on_exit(guinea::shutdown)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
