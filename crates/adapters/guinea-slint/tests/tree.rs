//! The router driving a real Slint tree.
//!
//! An integration test rather than a unit one: the components come from the
//! `slint!` macro, which generates code referring to the `slint` crate by
//! name - the same thing an application compiles.

use guinea_app::feature::FeatureInitContext;
use guinea_core::actor::UiThreadToken;
use guinea_core::scope::{NoopActions, Reducer};
use guinea_core::uri::AppUri;
use guinea_router::router::{Router, SegmentEntry};
use guinea_slint::{LayoutCx, PageCx, Slint, layout_entry, segment_entry};
use i_slint_backend_testing::{ElementHandle, ElementRoot};
use slint::ComponentHandle;

slint::slint! {
    export global TitleModel {
        in property <string> label;
    }

    component Leaf {
        Text {
            text: TitleModel.label;
            accessible-role: text;
            accessible-label: TitleModel.label;
        }
    }

    export component Shell inherits Window {
        in property <int> page;

        VerticalLayout {
            Text {
                text: "tabs";
                accessible-role: text;
                accessible-label: "tabs";
            }
            if root.page == 0: Leaf { }
        }
    }
}

/// A reducer with nothing but a string in it - enough to watch a binding
/// reach a global.
struct Title;

impl Reducer for Title {
    type State = String;
    type Push = String;
    type Group = ();
    type Actions = NoopActions;

    fn reduce(state: &mut Self::State, msg: Self::Push) {
        *state = msg;
    }
}

struct Tabs;

impl guinea_slint::Layout for Tabs {
    fn bind(_cx: LayoutCx) {}
}

struct Processes;

impl guinea_slint::Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        ctx.seed_reducer::<Title>(uri.segment(0).unwrap_or("none").to_string());
        Ok(())
    }

    fn bind(cx: PageCx) {
        let root = cx.root::<Shell>();
        cx.bind::<Title>(move |title| root.global::<TitleModel>().set_label(title.into()));
    }
}

const CHAIN: [SegmentEntry<Slint>; 2] = [layout_entry::<Tabs>(), segment_entry::<Processes>()];

fn mounted() -> (Shell, Router<Slint>) {
    i_slint_backend_testing::init_no_event_loop();

    let shell = Shell::new().expect("window");
    guinea_slint::testing::set_root(shell.clone_strong());

    let token = UiThreadToken::dangerously_create_token_unchecked();
    let router = Router::<Slint>::new(token);
    let uri = AppUri::parse("/ubuntu").unwrap();
    router.activate(&uri, &CHAIN).expect("activate");

    shell.set_page(0);
    shell.show().expect("show");

    (shell, router)
}

fn labels(shell: &Shell) -> Vec<String> {
    shell
        .root_element()
        .query_descendants()
        .find_all()
        .into_iter()
        .filter_map(|element: ElementHandle| element.accessible_label().map(|l| l.to_string()))
        .collect()
}

#[test]
fn installing_a_chain_wires_the_page_that_is_showing() {
    // The router installed the page's feature and its binding reached the
    // global the branch reads - in a toolkit where the branch itself was
    // there all along.
    let (shell, _router) = mounted();

    assert_eq!(labels(&shell), vec!["tabs".to_string(), "ubuntu".to_string()]);
}

#[test]
fn state_reaches_the_global_after_the_page_was_installed() {
    let (shell, router) = mounted();

    let scope = router.active_scope().expect("a page is mounted");
    scope.push::<Title>("fedora".to_string());

    assert_eq!(
        labels(&shell),
        vec!["tabs".to_string(), "fedora".to_string()],
        "the binding outlived the call that made it"
    );
}

#[test]
fn a_binding_survives_the_branch_being_destroyed_and_rebuilt() {
    let (shell, router) = mounted();

    // What every navigation does: Slint drops the subtree and builds it
    // again. The global is a singleton, so what was bound to it still holds.
    shell.set_page(1);
    shell.set_page(0);

    let scope = router.active_scope().expect("a page is mounted");
    scope.push::<Title>("debian".to_string());

    assert_eq!(labels(&shell), vec!["tabs".to_string(), "debian".to_string()]);
}
