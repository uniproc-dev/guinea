//! What closing a root takes with it.
//!
//! The question a second window asks: when one goes away, does its half of
//! the application go with it, and does the other half survive? Answered on
//! the headless backend, where a root is a router and nothing else - no
//! window, no toolkit, no shell deciding when to exit.

use std::cell::Cell;
use std::rc::Rc;

use guinea_app::app::roots;
use guinea_app::feature::FeatureInitContext;
use guinea_core::actor::UiThreadToken;
use guinea_core::scope::{DropGuard, NoopActions, Reducer};
use guinea_core::uri::AppUri;
use guinea_router::headless::{Headless, HeadlessCx, Page, segment_entry};
use guinea_router::router::{Router, SegmentEntry};

thread_local! {
    /// Set when a page's scope is torn down.
    static TORN_DOWN: Cell<bool> = const { Cell::new(false) };
}

struct Marker;

impl Reducer for Marker {
    type State = u32;
    type Push = u32;
    type Group = ();
    type Actions = NoopActions;

    fn reduce(state: &mut Self::State, msg: Self::Push) {
        *state = msg;
    }
}

/// Notes its own teardown, the way an actor's `dispose` would.
struct Tombstone;

impl Drop for Tombstone {
    fn drop(&mut self) {
        TORN_DOWN.with(|torn| torn.set(true));
    }
}

struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        ctx.seed_reducer::<Marker>(1);
        ctx.scope.own(DropGuard(Tombstone));
        Ok(())
    }

    fn view(_cx: &mut HeadlessCx) {}
}

const CHAIN: [SegmentEntry<Headless>; 1] = [segment_entry::<Processes>()];

fn open() -> Rc<Router<Headless>> {
    let token = UiThreadToken::dangerously_create_token_unchecked();
    let router = Rc::new(Router::<Headless>::new(token));
    router
        .activate(&AppUri::parse("/processes").unwrap(), &CHAIN)
        .expect("activate");
    router
}

#[test]
fn a_router_is_a_root_while_it_lives() {
    TORN_DOWN.with(|torn| torn.set(false));

    let router = open();
    let id = router.root();

    assert!(roots::is_open(id));
    assert!(roots().contains(&id));

    drop(router);

    assert!(!roots::is_open(id), "the entry goes with the host");
    assert!(
        TORN_DOWN.with(Cell::get),
        "and so do the scopes under it - closing a window has to end its features"
    );
}

#[test]
fn closing_one_root_leaves_the_other_running() {
    // The case a second window is opened for: two roots, sharing the process
    // and nothing else.
    let first = open();
    let second = open();
    let (gone, staying) = (first.root(), second.root());

    assert_ne!(gone, staying, "each host is its own root");

    drop(first);

    assert!(!roots::is_open(gone));
    assert!(roots::is_open(staying));
    assert!(
        second.active_scope().is_some(),
        "the surviving root still has its chain installed"
    );
}

fn roots() -> Vec<roots::RootId> {
    roots::roots()
}
