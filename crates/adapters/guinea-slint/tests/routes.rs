//! `routes!` targeting Slint, and navigation moving the tree from branch to
//! branch.

use guinea_core::actor::UiThreadToken;
use guinea_macros::routes;
use guinea_router::router::{RouteChain, Router, ToUri};
use guinea_slint::{LayoutCx, PageCx, Slint};
use i_slint_backend_testing::{ElementHandle, ElementRoot};
use slint::ComponentHandle;

slint::slint! {
    export global PageModel {
        in property <string> processes;
        in property <string> services;
    }

    component ProcessesView {
        Text {
            text: PageModel.processes;
            accessible-role: text;
            accessible-label: PageModel.processes;
        }
    }

    component ServicesView {
        Text {
            text: PageModel.services;
            accessible-role: text;
            accessible-label: PageModel.services;
        }
    }

    export component Host inherits Window {
        in property <int> page;

        VerticalLayout {
            Text {
                text: "tabs";
                accessible-role: text;
                accessible-label: "tabs";
            }
            if root.page == 0: ProcessesView { }
            if root.page == 1: ServicesView { }
        }
    }
}

struct Shell;

impl guinea_slint::Layout for Shell {
    fn bind(_cx: LayoutCx) {}
}

struct Processes;

impl guinea_slint::Page for Processes {
    fn bind(cx: PageCx) {
        cx.root::<Host>()
            .global::<PageModel>()
            .set_processes("processes".into());
    }
}

struct Services;

impl guinea_slint::Page for Services {
    fn bind(cx: PageCx) {
        cx.root::<Host>()
            .global::<PageModel>()
            .set_services("services".into());
    }
}

routes! {
    backend = guinea_slint::Slint,
    Route {
        layout(Shell) {
            page(Processes, "/:host/processes") { host: String }
            page(Services, "/:host/services") { host: String }
        }
    }
}

fn labels(host: &Host) -> Vec<String> {
    host.root_element()
        .query_descendants()
        .find_all()
        .into_iter()
        .filter_map(|element: ElementHandle| element.accessible_label().map(|l| l.to_string()))
        .collect()
}

fn shown(route: Route, page: i32) -> Vec<String> {
    i_slint_backend_testing::init_no_event_loop();

    let host = Host::new().expect("window");
    guinea_slint::testing::set_root(host.clone_strong());

    let token = UiThreadToken::dangerously_create_token_unchecked();
    let router = Router::<Slint>::new(token);
    let uri = route.to_uri();
    router.navigate(route, &uri).expect("navigate");

    host.set_page(page);
    host.show().expect("show");

    labels(&host)
}

#[test]
fn the_macro_builds_chains_for_slint() {
    assert_eq!(
        shown(
            Route::Processes {
                host: "ubuntu".to_string()
            },
            0
        ),
        vec!["tabs".to_string(), "processes".to_string()]
    );
}

#[test]
fn the_other_branch_is_wired_by_its_own_route() {
    assert_eq!(
        shown(
            Route::Services {
                host: "ubuntu".to_string()
            },
            1
        ),
        vec!["tabs".to_string(), "services".to_string()]
    );
}

#[test]
fn a_route_still_round_trips_through_its_uri() {
    let route = Route::Services {
        host: "ubuntu".to_string(),
    };
    assert_eq!(
        route.to_uri(),
        guinea_core::uri::AppUri::parse("/ubuntu/services").unwrap()
    );
    assert_eq!(route.chain().len(), 2, "layout plus page");
}
