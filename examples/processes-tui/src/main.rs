//! The terminal front end. Same application as `processes-app`: same actors,
//! same reducers, same features - only the pages and the route tree differ.

mod cursor;
mod layouts;
mod pages;
mod routes;

use guinea::app::GuineaApp;
use guinea::ratatui::{Flow, Tui, pressed, run};
use guinea_router::router::Router;
use ratatui::crossterm::event::{Event, KeyCode};
use routes::Route;

use guinea_core::scope::Scope;
use guinea_plugin_l10n::Localization;
use processes_core::l10n::L10n;
use processes_core::processes::contracts::{Kill, ProcessesReducer};
use processes_core::services::contracts::ServicesReducer;
use processes_core::startup;

use cursor::{Cursor, Move};

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
        KeyCode::Up => move_focus(router, -1),
        KeyCode::Down => move_focus(router, 1),
        KeyCode::Char('k') => kill_focused(router),
        KeyCode::Char('l') => toggle_language(),
        _ => {}
    }

    Flow::Continue
}

/// The language lives in the process, not in a window, so this reaches the
/// WinUI front end too: flip it here and the next run of `processes-app`
/// starts in the language the terminal left it in.
fn toggle_language() {
    let next = if L10n::current().tag() == "ru" {
        "en"
    } else {
        "ru"
    };
    if let Some(strings) = L10n::for_tag(next) {
        guinea_plugin_l10n::L10n::<L10n>::load(strings);
    }
}

fn move_focus(router: &Router<Tui>, delta: isize) {
    let Some(scope) = router.active_scope() else {
        return;
    };
    let len = rows_on_screen(&scope);
    scope.push::<Cursor>(Move { delta, len });
}

/// How long the list the active page is drawing is - asked of the page's own
/// scope rather than of the route, so a page without a list simply has none.
fn rows_on_screen(scope: &Scope) -> usize {
    if let Some(state) = scope.peek::<ProcessesReducer>() {
        return state.borrow().items.len();
    }
    if let Some(state) = scope.peek::<ServicesReducer>() {
        return state.borrow().items.len();
    }
    0
}

fn kill_focused(router: &Router<Tui>) {
    let Some(scope) = router.active_scope() else {
        return;
    };
    let Some(state) = scope.peek::<ProcessesReducer>() else {
        return;
    };
    let focused = cursor::focused(*scope.state::<Cursor>().borrow(), state.borrow().items.len());
    let pid = processes_core::processes::pid_at(&state.borrow().items, focused);
    if let Some(pid) = pid {
        scope.actions::<ProcessesReducer>().emit(Kill(pid));
    }
}
