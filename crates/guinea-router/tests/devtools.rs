//! What devtools read from a live router.

use std::any::Any;
use std::rc::Rc;

use guinea_app::feature::{FeatureInitContext, Segment};
use guinea_core::actor::UiThreadToken;
use guinea_core::feature::Bound;
use guinea_core::scope::Reducer;
use guinea_router::headless::{Headless, HeadlessCx, Layout, Page, layout_entry, segment_entry};
use guinea_router::devtools;
use guinea_router::router::{RouteChain, Router, SegmentEntry};

#[derive(Default, Clone, Debug)]
struct Counter {
    seen: u32,
}

impl Reducer for Counter {
    type Update = u32;

    fn reduce(&mut self, seen: u32) {
        self.seen = seen;
    }
}

struct Frame;

impl Layout for Frame {
    type Params = ();
    type Installs = ();

    fn install(_ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut HeadlessCx<Self>) {
        cx.outlet();
    }
}

struct Detail;

impl Page for Detail {
    type Params = ();
    type Installs = Bound<Counter>;

    fn install(ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<Bound<Counter>> {
        Ok(ctx.state::<Counter>().seed(Counter { seen: 3 }).plain())
    }

    fn view(_cx: &mut HeadlessCx<Self>) {}
}

impl Segment for Frame {
    type Installs = ();
    type Above = ();
}

impl Segment for Detail {
    type Installs = Bound<Counter>;
    type Above = (Frame, ());
}

const CHAIN: [SegmentEntry<Headless>; 2] = [layout_entry::<Frame>(), segment_entry::<Detail>()];

struct Route {
    id: u32,
}

impl RouteChain<Headless> for Route {
    fn chain(&self) -> &'static [SegmentEntry<Headless>] {
        &CHAIN
    }

    fn params(&self) -> Vec<Box<dyn Any>> {
        vec![Box::new(()), Box::new(())]
    }

    fn name(&self) -> &'static str {
        "Detail"
    }

    fn describe(&self) -> String {
        format!("Route {{ id: {} }}", self.id)
    }
}

#[test]
fn a_router_shows_up_once_it_has_navigated_and_goes_when_dropped() {
    let token = UiThreadToken::dangerously_create_token_unchecked();
    let router = Rc::new(Router::<Headless>::new(token));
    let before = devtools::routers().len();

    router.navigate(Route { id: 7 }).expect("navigate");
    router.navigate(Route { id: 8 }).expect("navigate again");

    let views = devtools::routers();
    assert_eq!(views.len(), before + 1, "registered once, not per navigation");

    let view = views.last().unwrap();
    assert_eq!(view.root, router.root());
    assert_eq!(view.backend, "Headless");
    assert_eq!(view.route.as_deref(), Some("Route { id: 8 }"));

    let names: Vec<&str> = view.segments.iter().map(|segment| segment.name).collect();
    assert_eq!(names, ["Frame", "Detail"]);

    let detail = &view.segments[1].states;
    assert_eq!(detail.len(), 1);
    assert!(detail[0].type_name.ends_with("Counter"));
    assert!(detail[0].state.contains("seen: 3"), "{}", detail[0].state);

    drop(views);
    drop(router);
    assert_eq!(devtools::routers().len(), before);
}
