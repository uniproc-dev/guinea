use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::actor::UiThreadToken;

use crate::lifecycle_tracker::AppLifecycle;

use super::{AppFeature, FeatureBuilder, Plugin, PluginBuilder};

thread_local! {
    static TRACE: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn trace(step: &'static str) {
    TRACE.with(|t| t.borrow_mut().push(step));
}

fn taken() -> Vec<&'static str> {
    TRACE.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

fn builder() -> FeatureBuilder {
    TRACE.with(|t| t.borrow_mut().clear());
    FeatureBuilder::new(
        UiThreadToken::dangerously_create_token_unchecked(),
        AppLifecycle::new(),
    )
}

struct Settings;
impl Plugin for Settings {
    const ID: &'static str = "test.settings";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        trace("settings");
        app.provide(Store("db"));
        Ok(())
    }
}

struct Store(&'static str);

struct Updater;
impl Plugin for Updater {
    const ID: &'static str = "test.updater";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.plugin(Settings)?;
        let store = app.require::<Store>()?;
        trace(store.0);
        Ok(())
    }
}

struct Left;
impl Plugin for Left {
    const ID: &'static str = "test.left";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.plugin(Settings)?;
        trace("left");
        Ok(())
    }
}

struct Colliding;
impl Plugin for Colliding {
    const ID: &'static str = "test.settings";
    fn build(self, _app: &mut PluginBuilder) -> anyhow::Result<()> {
        Ok(())
    }
}

struct CycleA;
struct CycleB;

impl Plugin for CycleA {
    const ID: &'static str = "test.cycle.a";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.plugin(CycleB)?;
        Ok(())
    }
}

impl Plugin for CycleB {
    const ID: &'static str = "test.cycle.b";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.plugin(CycleA)?;
        Ok(())
    }
}

struct Orphan;
impl Plugin for Orphan {
    const ID: &'static str = "test.orphan";
    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.require::<Store>()?;
        Ok(())
    }
}

struct Startup;
impl AppFeature for Startup {
    fn install(self, app: &mut FeatureBuilder) -> anyhow::Result<()> {
        trace("startup");
        app.plugin(Settings)?;
        Ok(())
    }
}

#[test]
fn a_plugin_pulled_twice_is_built_once() {
    let mut app = builder();
    app.plugin(Updater).unwrap();
    app.plugin(Left).unwrap();

    assert_eq!(taken(), vec!["settings", "db", "left"]);
}

#[test]
fn a_diamond_builds_the_shared_dependency_once() {
    let mut app = builder();
    app.plugin(Left).unwrap();
    app.plugin(Updater).unwrap();

    assert_eq!(taken(), vec!["settings", "left", "db"]);
}

#[test]
fn a_dependency_cycle_names_the_path() {
    let mut app = builder();
    let err = format!("{:#}", app.plugin(CycleA).map(|_| ()).unwrap_err());

    assert!(err.contains("cycle"), "got {err}");
    assert!(err.contains("test.cycle.a"), "got {err}");
    assert!(err.contains("test.cycle.b"), "got {err}");
}

#[test]
fn two_plugins_sharing_an_id_is_an_error() {
    let mut app = builder();
    app.plugin(Settings).unwrap();

    let err = format!("{:#}", app.plugin(Colliding).map(|_| ()).unwrap_err());
    assert!(err.contains("ID collision"), "got {err}");
}

#[test]
fn a_missing_service_names_who_wanted_it() {
    let mut app = builder();
    let err = format!("{:#}", app.plugin(Orphan).map(|_| ()).unwrap_err());

    assert!(err.contains("test.orphan"), "got {err}");
    assert!(err.contains("Store"), "got {err}");
}

#[test]
fn a_feature_installed_twice_runs_once() {
    let mut app = builder();
    app.feature(Startup).unwrap();
    app.feature(Startup).unwrap();

    assert_eq!(taken(), vec!["startup", "settings"]);
}

#[test]
fn a_feature_may_pull_plugins_and_read_what_they_provide() {
    let mut app = builder();
    app.feature(Startup).unwrap();

    assert_eq!(app.require::<Store>().unwrap().0, "db");
}

#[test]
fn subscriptions_taken_during_install_are_dropped_on_shutdown() {
    use crate::feature::AppFeatureDeinitContext;
    use guinea_core::actor::event_bus::GlobalEventBus;
    use guinea_core::messages;

    messages! { Tick }

    let token = UiThreadToken::dangerously_create_token_unchecked();
    let lifecycle = AppLifecycle::new();
    let seen = Rc::new(RefCell::new(0usize));

    {
        let app = PluginBuilder::new(token.clone(), lifecycle.clone());
        let seen = seen.clone();
        app.subscribe_global::<Tick>(move |_| *seen.borrow_mut() += 1);
    }

    assert_eq!(GlobalEventBus::count_subscribers::<Tick>(), 1);

    let reactor = crate::timers::Reactor::new();
    let shared = guinea_core::SharedState::new();
    let mut ctx = AppFeatureDeinitContext {
        token: token.clone(),
        reactor: &reactor,
        shared: &shared,
    };
    lifecycle.shutdown(&token, &mut ctx);

    assert_eq!(GlobalEventBus::count_subscribers::<Tick>(), 0);
}

#[test]
fn route_hooks_see_the_previous_and_current_path() {
    let seen: Rc<RefCell<Vec<(Option<String>, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let token = UiThreadToken::dangerously_create_token_unchecked();

    let recorded = seen.clone();
    super::runtime::install_runtime(super::runtime::AppRuntime {
        token: token.clone(),
        builder: FeatureBuilder::new(token, AppLifecycle::new()),
        route_hooks: vec![Box::new(move |from, to| {
            recorded
                .borrow_mut()
                .push((from.map(str::to_string), to.to_string()));
        })],
        last_route: Default::default(),
    });

    super::runtime::route_changed("/a");
    super::runtime::route_changed("/b");

    let seen = seen.borrow();
    assert_eq!(seen[0], (None, "/a".to_string()));
    assert_eq!(seen[1], (Some("/a".to_string()), "/b".to_string()));
}

struct Greeting(&'static str);

struct GreetingPlugin;

impl Plugin for GreetingPlugin {
    const ID: &'static str = "test.greeting";

    fn build(self, app: &mut PluginBuilder) -> anyhow::Result<()> {
        app.provide(Greeting("hello"));
        Ok(())
    }
}

#[test]
fn a_feature_installs_without_a_router_and_reaches_the_services() {
    let token = UiThreadToken::dangerously_create_token_unchecked();

    let runtime = super::GuineaApp::new()
        .plugin(GreetingPlugin)
        .install(token.clone())
        .expect("install");
    crate::app::install_runtime(runtime);

    // No chain, no route, no backend: an application with a single window and
    // nothing to navigate between still gets a scope and its services.
    let host = crate::feature::FeatureHost::new(token);
    let scope = host
        .install(|ctx| {
            assert_eq!(ctx.require::<Greeting>()?.0, "hello");
            ctx.subscribe(|_: Ping| trace("pinged"));
            Ok(())
        })
        .expect("install the feature");

    host.event_bus().publish(Ping);
    assert_eq!(taken(), vec!["pinged"]);

    drop(scope);
    host.event_bus().publish(Ping);
    assert!(
        taken().is_empty(),
        "the subscription is owned by the scope and ends with it"
    );
}

#[derive(Clone)]
struct Ping;

impl guinea_core::actor::Message for Ping {}
