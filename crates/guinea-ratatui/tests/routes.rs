//! `routes!` targeting a backend other than the facade's.
//!
//! An integration test rather than a unit one on purpose: inside the crate the
//! macro resolves `guinea` as `Itself`, and what an application compiles is the
//! other case.

use guinea_core::actor::UiThreadToken;
use guinea_core::uri::AppUri;
use guinea_macros::routes;
use guinea_ratatui::{LayoutCx, PageCx, Tui};
use guinea_router::router::{RouteChain, Router, ToUri};
use ratatui::layout::{Constraint, Direction, Layout as RLayout};
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, backend::TestBackend};

struct Shell;

impl guinea_ratatui::Layout for Shell {
    fn view(cx: &mut LayoutCx<'_, '_>) {
        let chunks = RLayout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(cx.area());

        cx.frame().render_widget(Paragraph::new("tabs"), chunks[0]);
        cx.outlet(chunks[1]);
    }
}

struct Processes;

impl guinea_ratatui::Page for Processes {
    fn view(cx: &mut PageCx<'_, '_>) {
        let area = cx.area();
        cx.frame().render_widget(Paragraph::new("processes"), area);
    }
}

struct Services;

impl guinea_ratatui::Page for Services {
    fn view(cx: &mut PageCx<'_, '_>) {
        let area = cx.area();
        cx.frame().render_widget(Paragraph::new("services"), area);
    }
}

routes! {
    backend = guinea_ratatui::Tui,
    Route {
        layout(Shell) {
            page(Processes, "/:host/processes") { host: String }
            page(Services, "/:host/services") { host: String }
        }
    }
}

fn draw(route: Route) -> String {
    let token = UiThreadToken::dangerously_create_token_unchecked();
    let router = Router::<Tui>::new(token);
    let uri = route.to_uri();
    router.navigate(route, &uri).expect("navigate");

    let mut terminal = Terminal::new(TestBackend::new(10, 2)).expect("terminal");
    terminal
        .draw(|frame| router.render().draw(frame, frame.area()))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_macro_builds_chains_for_a_backend_that_is_not_the_facade() {
    assert_eq!(
        draw(Route::Processes {
            host: "ubuntu".to_string()
        }),
        "tabs\nprocesses"
    );
}

#[test]
fn navigating_between_siblings_keeps_the_layout_and_swaps_the_page() {
    assert_eq!(
        draw(Route::Services {
            host: "ubuntu".to_string()
        }),
        "tabs\nservices"
    );
}

#[test]
fn a_route_still_round_trips_through_its_uri() {
    let route = Route::Services {
        host: "ubuntu".to_string(),
    };
    assert_eq!(route.to_uri(), AppUri::parse("/ubuntu/services").unwrap());
    assert_eq!(route.chain().len(), 2, "layout plus page");
}
