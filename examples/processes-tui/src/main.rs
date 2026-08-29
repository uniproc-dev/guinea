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
use processes_core::processes::contracts::{Kill, Processes as Running};
use processes_core::services::contracts::Services;
use processes_core::tabs::contracts::Tabs;
use processes_core::startup;

use cursor::{Cursor, Move};

/// Where the store keeps the route between runs.
const LAST_ROUTE: &str = "route";

/// Where the last run left off, or the first tab.
///
/// Runs after the plugins are installed, which is the whole reason `run` takes
/// a closure: the store does not exist until the store plugin provides it.
///
/// Every failure ends here rather than propagating. A saved route outlives the
/// build that wrote it, so one that no longer parses is an ordinary thing to
/// find on the way in - the application starts where it always did.
fn initial_route() -> Route {
    guinea_plugin_store::amethystate::global_store()
        .get::<String>(LAST_ROUTE)
        .ok()
        .flatten()
        .as_deref()
        .and_then(Route::restore)
        .unwrap_or(Route::Processes {
            context: "ubuntu".to_string(),
        })
}

/// Writes the route down, if it agreed to survive a restart.
///
/// The router hands over a string and has no opinion about where it goes -
/// this is the half that does.
fn remember(route: &Route) {
    let Some(saved) = route.save() else {
        return;
    };

    if let Err(error) = guinea_plugin_store::amethystate::global_store().set(LAST_ROUTE, &saved) {
        tracing::warn!(%error, "the route could not be remembered");
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

    run(app, initial_route, on_key)
}

fn on_key(
    event: &Event,
    nav: &guinea_router::router::NavigateHandle<Tui, Route>,
    router: &Router<Tui>,
) -> Flow {
    let Some(code) = pressed(event) else {
        return Flow::Continue;
    };

    // What the layout was reached with, read back from where its install put
    // it - a key handler lives outside the tree and has no params of its own.
    let context = router
        .scope_at(0)
        .map(|tabs| tabs.state::<Tabs>().borrow().context.clone())
        .unwrap_or_default();

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

    // One place rather than beside every `nav.to`: whatever the key did to the
    // route - including `back` - is where the next run should start.
    if let Some(route) = router.current_route::<Route>() {
        remember(&route);
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
    if let Some(state) = scope.peek::<Running>() {
        return state.borrow().items.len();
    }
    if let Some(state) = scope.peek::<Services>() {
        return state.borrow().items.len();
    }
    0
}

fn kill_focused(router: &Router<Tui>) {
    let Some(scope) = router.active_scope() else {
        return;
    };
    let Some(state) = scope.peek::<Running>() else {
        return;
    };
    let focused = scope.state::<Cursor>().borrow().row;
    let pid = processes_core::processes::pid_at(&state.borrow().items, focused);
    if let Some(pid) = pid {
        scope.binding::<Running>().dispatch().emit(Kill(pid));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea::app::TestApp;

    /// The two halves of `restorable` against a real store, since separately
    /// they both pass while agreeing about nothing.
    ///
    /// One test rather than three: the store is a process-wide global and may
    /// be initialised once, so tests that each wanted their own would collide
    /// in the one process `cargo test` gives them.
    #[test]
    fn a_saved_route_is_where_the_next_run_starts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut app = TestApp::new();
        app.install(guinea_plugin_store::StorePlugin::at(dir.path().join("store")))
            .expect("the store plugin");

        assert!(
            matches!(initial_route(), Route::Processes { .. }),
            "nothing was saved yet, so the application starts where it always did"
        );

        remember(&Route::Metrics {
            context: "fedora".to_string(),
        });

        assert_eq!(
            initial_route(),
            Route::Metrics {
                context: "fedora".to_string()
            }
        );

        // A saved route outlives the build that wrote it, and one that no
        // longer exists is an ordinary thing to find on the way in.
        guinea_plugin_store::amethystate::global_store()
            .set(LAST_ROUTE, &r#"{"route":"Removed","fields":{}}"#.to_string())
            .expect("set");

        assert!(matches!(initial_route(), Route::Processes { .. }));
    }
}
