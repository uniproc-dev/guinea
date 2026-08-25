//! The terminal front end. Same application as `processes-app`: same actors,
//! same reducers, same features - only the pages and the route tree differ.

mod layouts;
mod pages;
mod routes;

use guinea::app::GuineaApp;
use guinea::ratatui::{Flow, Tui, pressed, run};
use guinea_router::router::Router;
use ratatui::crossterm::event::{Event, KeyCode};
use routes::Route;

use processes_core::processes::contracts::{Kill, ProcessesReducer};
use processes_core::startup;

fn initial_route() -> Route {
    Route::Processes {
        context: "ubuntu".to_string(),
    }
}

fn main() -> anyhow::Result<()> {
    // To a file: stdout is the drawing surface, and a log line in the middle
    // of a frame corrupts it.
    let log = std::fs::File::create("processes-tui.log")?;
    tracing_subscriber::fmt()
        .with_writer(log)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,guinea=debug,processes_core=debug".into()),
        )
        .init();

    let app = GuineaApp::new()
        .meta(guinea::app::AppMeta::new(
            "Processes",
            "dev.uniproc.guinea.processes",
            env!("CARGO_PKG_VERSION"),
            "uniproc",
        ))
        .plugin(
            guinea_plugin_store::StorePlugin::for_app("guinea-processes-app-example", "settings")
                // JSON, so both front ends can run at once: redb locks its
                // file and the second one would refuse to start.
                .backend(guinea_plugin_store::amethystate::store::builder::Backend::Json),
        )
        .plugin(guinea_plugin_l10n::L10nPlugin::<processes_core::l10n::L10n>::new("en"))
        .feature(startup::Startup);

    run(app, initial_route(), on_key)
}

fn on_key(
    event: &Event,
    nav: &guinea_router::router::NavigateHandle<Tui, Route>,
    router: &Router<Tui>,
) -> Flow {
    let Some(code) = pressed(event) else {
        return Flow::Continue;
    };

    let context = "ubuntu".to_string();
    match code {
        KeyCode::Char('q') => return Flow::Exit,
        // Back where it came from, and only quit when there is nowhere left -
        // the jest every terminal user tries first.
        KeyCode::Esc => {
            if !nav.back() {
                return Flow::Exit;
            }
        }
        KeyCode::Char('1') => nav.to(Route::Processes { context }),
        KeyCode::Char('2') => nav.to(Route::Services { context }),
        KeyCode::Char('3') => nav.to(Route::Metrics { context }),
        // No widget to hang a handler on, so the key reaches the page's
        // actions through the scope the router installed for it.
        KeyCode::Char('k') => kill_first(router),
        _ => {}
    }

    Flow::Continue
}

fn kill_first(router: &Router<Tui>) {
    let Some(scope) = router.active_scope() else {
        return;
    };
    let state = scope.state::<ProcessesReducer>();
    let pid = pages::processes::pid_at(&state.borrow().items, 0);
    if let Some(pid) = pid {
        scope.actions::<ProcessesReducer>().emit(Kill(pid));
    }
}
