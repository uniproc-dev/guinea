//! The WinUI front end. Everything that is not drawing lives in
//! `processes-core`, which the terminal front end links just the same.

mod layouts;
mod pages;
mod routes;

use routes::Route;

use guinea::app::GuineaApp;
use guinea::winui::{Window, run};
use processes_core::startup;

fn initial_route() -> Route {
    Route::Processes {
        context: "ubuntu".to_string(),
    }
}

fn main() -> anyhow::Result<()> {
    // To a file, like the terminal front end: a windowed application has no
    // console to watch, and its stdout is block buffered - a line written now
    // would show up only when it exits.
    let log = std::fs::File::create("processes-app.log")?;
    tracing_subscriber::fmt()
        .with_writer(log)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,guinea=debug,processes_core=debug".into()),
        )
        .init();

    let app = GuineaApp::new()
        .meta(guinea::app_meta!())
        .plugin(
            guinea_plugin_store::StorePlugin::for_app("guinea-processes-app-example", "settings")
                // JSON, so both front ends can run at once: redb locks its
                // file and the second one would refuse to start.
                .backend(guinea_plugin_store::amethystate::store::builder::Backend::Json),
        )
        .plugin(guinea_plugin_l10n::L10nPlugin::<processes_core::l10n::L10n>::new("en"))
        .feature(startup::Startup);

    run(
        app,
        Window::new()
            .title(guinea::app_meta!().window_title)
            .client_size(420.0, 420.0),
        initial_route,
    )
}
