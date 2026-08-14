//! The windows-reactor backend: what a view is, how a segment is mounted, and
//! the hooks a view reads state through.

use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::scope::Reducer;

use crate::feature::FeatureInitContext;
use crate::router::{
    NavigateHandle, RouteChain, RouteSink, Router, SegmentEntry, SegmentProps, ToUri, Ui,
    single_entry_chain,
};
use crate::uri::AppUri;

/// windows-reactor as a [`Ui`].
pub struct WinUi;

impl Ui for WinUi {
    type View = windows_reactor::Element;
}

/// A leaf of the route tree.
///
/// Declared by the backend, not the router: what a view is handed and what it
/// returns differ per toolkit, and there is nothing useful left once you take
/// the difference out.
pub trait Page: 'static {
    /// When `true`, the router keeps this page's reducer states in memory
    /// while the page is not mounted. The page's scope (and therefore its
    /// actors) is still torn down, but when the user returns the UI will see
    /// the last cached state immediately instead of starting from defaults.
    const CACHE_STATE_IN_MEMORY: bool = false;

    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut PageCx) -> windows_reactor::Element;
}

/// A branch of the route tree: renders its own chrome and its child through
/// [`LayoutCx::outlet`].
pub trait Layout: 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut LayoutCx) -> windows_reactor::Element;
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<WinUi> {
    SegmentEntry::new(
        TypeId::of::<P>,
        P::install,
        mount_page::<P>,
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<WinUi> {
    SegmentEntry::new(TypeId::of::<L>, L::install, mount_layout::<L>, false)
}

pub fn mount_page<P: Page>(props: SegmentProps<WinUi>) -> windows_reactor::Element {
    windows_reactor::component(render_page::<P>, props)
}

pub fn mount_layout<L: Layout>(props: SegmentProps<WinUi>) -> windows_reactor::Element {
    windows_reactor::component(render_layout::<L>, props)
}

fn render_page<P: Page>(
    props: &SegmentProps<WinUi>,
    cx: &mut windows_reactor::RenderCx,
) -> windows_reactor::Element {
    P::view(&mut PageCx {
        props: props.clone(),
        cx,
    })
}

fn render_layout<L: Layout>(
    props: &SegmentProps<WinUi>,
    cx: &mut windows_reactor::RenderCx,
) -> windows_reactor::Element {
    L::view(&mut LayoutCx {
        props: props.clone(),
        cx,
    })
}

/// A one-segment chain for a page mounted without a route tree.
pub fn page_chain<P: Page>() -> &'static [SegmentEntry<WinUi>] {
    single_entry_chain(segment_entry::<P>())
}

fn nav_context<R: 'static>() -> windows_reactor::Context<Option<NavigateHandle<WinUi, R>>> {
    thread_local! {
        static IDS: RefCell<std::collections::HashMap<TypeId, windows_reactor::ContextId>> =
            RefCell::new(std::collections::HashMap::new());
    }
    let id = IDS.with(|ids| {
        *ids.borrow_mut()
            .entry(TypeId::of::<R>())
            .or_insert_with(windows_reactor::ContextId::new)
    });
    windows_reactor::Context { default: None, id }
}

fn route_context<R: 'static>() -> windows_reactor::Context<Option<R>> {
    thread_local! {
        static IDS: RefCell<std::collections::HashMap<TypeId, windows_reactor::ContextId>> =
            RefCell::new(std::collections::HashMap::new());
    }
    let id = IDS.with(|ids| {
        *ids.borrow_mut()
            .entry(TypeId::of::<R>())
            .or_insert_with(windows_reactor::ContextId::new)
    });
    windows_reactor::Context { default: None, id }
}

fn scoped_router(
    cx: &mut windows_reactor::RenderCx,
    token: guinea_core::actor::UiThreadToken,
) -> Rc<Router<WinUi>> {
    let slot = cx.use_ref(None::<Rc<Router<WinUi>>>);
    {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(Router::new(token)));
        }
    }
    slot.borrow().clone().expect("just initialized above")
}

pub struct RouterRx<R>(std::marker::PhantomData<R>);

impl<R> RouterRx<R>
where
    R: RouteChain<WinUi> + ToUri + Clone + PartialEq + 'static,
{
    pub fn render(cx: &mut windows_reactor::RenderCx, initial: R) -> windows_reactor::Element {
        use windows_reactor::ElementExt;

        // Genuinely on the UI thread here - `root()` render callbacks always
        // are - which is what the token attests to.
        let token = guinea_core::actor::UiThreadToken::dangerously_create_token_unchecked();

        let (route, set_route) = cx.use_state(initial);
        let router = scoped_router(cx, token);
        router.navigate(route.clone(), &route.to_uri()).expect("navigate");

        let nav = NavigateHandle::new(
            router.clone(),
            RouteSink::new(move |route| set_route.call(route)),
        );
        router
            .render()
            .provide(&nav_context::<R>(), Some(nav))
            .provide(&route_context::<R>(), Some(route))
    }
}

/// The router as a root component, for [`crate::app::App::run`].
///
/// An application whose UI is not route-based passes its own component
/// instead - `run` does not know about routing.
pub struct RouterRoot<R> {
    initial: R,
}

impl<R> RouterRoot<R> {
    pub fn at(initial: R) -> Self {
        Self { initial }
    }
}

impl<R> windows_reactor::Component for RouterRoot<R>
where
    R: RouteChain<WinUi> + ToUri + Clone + PartialEq + 'static,
{
    fn render(
        &self,
        _props: &(),
        cx: &mut windows_reactor::RenderCx,
    ) -> windows_reactor::Element {
        RouterRx::<R>::render(cx, self.initial.clone())
    }
}

pub trait UseNavigate {
    fn use_navigate<R>(&self) -> NavigateHandle<WinUi, R>
    where
        R: RouteChain<WinUi> + ToUri + Clone + PartialEq + 'static;
}

impl UseNavigate for windows_reactor::RenderCx {
    fn use_navigate<R>(&self) -> NavigateHandle<WinUi, R>
    where
        R: RouteChain<WinUi> + ToUri + Clone + PartialEq + 'static,
    {
        self.use_context(&nav_context::<R>()).unwrap_or_else(|| {
            panic!(
                "use_navigate::<{}>() called with no NavigateHandle provided - \
                 render the tree through RouterRx::<{0}>::render",
                std::any::type_name::<R>()
            )
        })
    }
}

pub trait UseRoute {
    fn use_route<R>(&self) -> R
    where
        R: Clone + PartialEq + 'static;
}

impl UseRoute for windows_reactor::RenderCx {
    fn use_route<R>(&self) -> R
    where
        R: Clone + PartialEq + 'static,
    {
        self.use_context(&route_context::<R>()).unwrap_or_else(|| {
            panic!(
                "use_route::<{}>() called with no route provided - \
                 render the tree through RouterRx::<{0}>::render",
                std::any::type_name::<R>()
            )
        })
    }
}


/// Reads a reducer's state and re-renders this component when it changes.
fn use_reducer<R>(
    props: &SegmentProps<WinUi>,
    cx: &mut windows_reactor::RenderCx,
) -> (R::State, Rc<R::Actions>)
where
    R: Reducer,
    R::State: Clone + PartialEq,
{
    let binding = props.binding::<R>();
    let (current, set_current) = cx.use_state(binding.get());

    // The subscription ends with this component instance, not with the scope:
    // an unmounted component that kept listening would call `set_current` on a
    // state slot nobody renders.
    let binding_for_effect = binding.clone();
    cx.use_effect_with_cleanup((), move || {
        let set_current = set_current.clone();
        let subscription =
            binding_for_effect.on_change(move |latest| set_current.call(latest.clone()));
        Some(move || drop(subscription))
    });

    (current, binding.actions())
}

pub struct PageCx<'a> {
    props: SegmentProps<WinUi>,
    cx: &'a mut windows_reactor::RenderCx,
}

impl PageCx<'_> {
    pub fn use_reducer<R>(&mut self) -> (R::State, Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone + PartialEq,
    {
        use_reducer::<R>(&self.props, self.cx)
    }
}

impl std::ops::Deref for PageCx<'_> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

impl std::ops::DerefMut for PageCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cx
    }
}

pub struct LayoutCx<'a> {
    props: SegmentProps<WinUi>,
    cx: &'a mut windows_reactor::RenderCx,
}

impl LayoutCx<'_> {
    pub fn use_reducer<R>(&mut self) -> (R::State, Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone + PartialEq,
    {
        use_reducer::<R>(&self.props, self.cx)
    }

    pub fn outlet(&mut self) -> windows_reactor::Element {
        self.props.outlet()
    }
}

impl std::ops::Deref for LayoutCx<'_> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

impl std::ops::DerefMut for LayoutCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::FeatureInitContext;
    use crate::router::*;
    use crate::uri::AppUri;
    use guinea_core::actor::UiThreadToken;
    use guinea_macros::{ReducerState, reducer, routes};

    #[derive(ReducerState)]
    struct ProcessesViewState {
        seeded_from: String,
    }

    #[reducer]
    fn processes_reducer(state: &mut ProcessesViewState, msg: String) {
        state.seeded_from = msg;
    }

    fn install_processes(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        ctx.seed_reducer::<ProcessesReducer>(ProcessesViewState {
            seeded_from: uri.segment(0).expect("test uri always has a segment").to_string(),
        });
        Ok(())
    }

    struct Processes;

    routes! {
        AppRoute {
            page(Processes, "/:context/processes") { context: String }
        }
    }

    impl Page for Processes {
        fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
            install_processes(ctx, uri)
        }

        fn view(cx: &mut PageCx<'_>) -> windows_reactor::Element {
            let (state, _dispatch) = cx.use_reducer::<ProcessesReducer>();
            assert_eq!(state.seeded_from, "ubuntu");
            windows_reactor::Element::default()
        }
    }

    struct Greeting(&'static str);

    struct GreetingPlugin;

    impl guinea_app::app::Plugin for GreetingPlugin {
        const ID: &'static str = "test.greeting";

        fn build(self, app: &mut guinea_app::app::PluginBuilder) -> anyhow::Result<()> {
            app.provide(Greeting("hello"));
            Ok(())
        }
    }

    struct NeedsAService;

    impl Page for NeedsAService {
        fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
            let greeting = ctx.require::<Greeting>()?;
            assert_eq!(greeting.0, "hello");
            Ok(())
        }

        fn view(_cx: &mut PageCx<'_>) -> windows_reactor::Element {
            windows_reactor::Element::default()
        }
    }

    #[test]
    fn a_page_can_read_what_an_application_plugin_provided() {
        let token = UiThreadToken::dangerously_create_token_unchecked();

        let runtime = guinea_app::app::App::new()
            .plugin(GreetingPlugin)
            .install(token.clone())
            .expect("install");
        guinea_app::app::install_runtime(runtime);

        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();

        router
            .activate(&uri, page_chain::<NeedsAService>())
            .expect("the page's install must find the service the plugin provided");
    }

    #[test]
    fn a_page_without_an_application_gets_a_plain_error_not_a_panic() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();

        let err = router
            .activate(&uri, page_chain::<NeedsAService>())
            .map(|_| ())
            .expect_err("nothing provided the service");

        assert!(
            format!("{err:#}").contains("Greeting"),
            "the error should name the missing service, got: {err:#}"
        );
    }

    #[test]
    fn scoped_router_is_call_site_scoped_not_process_global() {
        let token = UiThreadToken::dangerously_create_token_unchecked();

        let mut cx1 = windows_reactor::RenderCx::new(Rc::new(|| {}));
        cx1.begin_render();
        let router_a = scoped_router(&mut cx1, token.clone());

        // A second render pass of the *same* window (hook slots realign) -
        // must resolve the same Router, not a fresh one.
        cx1.begin_render();
        let router_a2 = scoped_router(&mut cx1, token.clone());
        assert!(
            Rc::ptr_eq(&router_a, &router_a2),
            "re-rendering the same window must reuse its own Router"
        );

        // A different render root (e.g. a second window's own RenderCx) has
        // its own independent hook storage - must get its own Router, no
        // process-wide sharing.
        let mut cx2 = windows_reactor::RenderCx::new(Rc::new(|| {}));
        cx2.begin_render();
        let router_b = scoped_router(&mut cx2, token);
        assert!(
            !Rc::ptr_eq(&router_a, &router_b),
            "a different render root must get its own Router, not the first one's"
        );
    }

    #[test]
    fn route_roundtrips_through_path() {
        let route = AppRoute::Processes {
            context: "ubuntu".to_string(),
        };
        assert_eq!(route.path(), "/ubuntu/processes");
        assert_eq!(
            AppRoute::parse("/ubuntu/processes"),
            Some(AppRoute::Processes {
                context: "ubuntu".to_string()
            })
        );
    }

    #[test]
    fn activating_a_page_runs_install_and_view_can_use_it() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();

        let scope = router.activate(&uri, page_chain::<Processes>()).expect("activate");
        assert!(router.active_scope().is_some());

        // install already ran (synchronously, inside activate) and seeded state.
        let state = scope.state::<ProcessesReducer>();
        assert_eq!(state.borrow().seeded_from, "ubuntu");

        // `Router::render()` returns a mounted `Element::Component`, which
        // only a real backend's `Reconciler` can invoke - this test instead
        // calls our own `render_page` directly (same-module access), the
        // same fn `mount_page::<Processes>` wraps, to exercise the
        // `SegmentCx`/`PageCx`/`use_reducer` plumbing on its own.
        let props = SegmentProps {
            chain: page_chain::<Processes>(),
            scopes: Rc::new(vec![scope.clone()]),
            cursor: 0,
        };
        let mut cx = windows_reactor::RenderCx::new(Rc::new(|| {}));
        render_page::<Processes>(&props, &mut cx);

        router.deactivate();
        assert!(router.active_scope().is_none());
    }

    #[test]
    fn push_after_first_render_requests_a_rerender() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();
        let scope = router.activate(&uri, page_chain::<Processes>()).expect("activate");

        let rerender_count = Rc::new(std::cell::Cell::new(0));
        let count_for_callback = rerender_count.clone();
        let mut cx = windows_reactor::RenderCx::new(Rc::new(move || {
            count_for_callback.set(count_for_callback.get() + 1);
        }));
        let props = SegmentProps {
            chain: page_chain::<Processes>(),
            scopes: Rc::new(vec![scope.clone()]),
            cursor: 0,
        };
        render_page::<Processes>(&props, &mut cx); // first render registers the effect...
        cx.flush_effects(); // ...which a real reconciler runs after each render; here
        // there's none, so call it manually - this is what runs Scope::subscribe.

        // An actor updating data well after the component last rendered.
        scope.push::<ProcessesReducer>("debian".to_string());

        assert!(
            rerender_count.get() > 0,
            "a push after the first render should have requested a re-render, \
             not just updated a cell nothing is watching"
        );
    }

    #[test]
    fn navigating_to_the_same_route_twice_in_a_row_keeps_the_same_scope_identity() {
        // Mirrors what `RouterRx::render` actually does: it calls
        // `Router::navigate` on *every* render, not just when the route
        // value changes (the caller doesn't know whether it changed).
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();
        let route = AppRoute::Processes { context: "ubuntu".to_string() };

        let scope_a = router.navigate(route.clone(), &uri).expect("first navigate");
        let scope_b = router.navigate(route, &uri).expect("second navigate, same route value");

        assert!(
            Rc::ptr_eq(&scope_a, &scope_b),
            "re-navigating to an unchanged route must reuse the exact same Scope, \
             not silently reinstall it - anything accumulating state in that Scope \
             (e.g. a live-updating chart) would otherwise reset on every single render"
        );
    }

    // --- nested layout persistence (RouteChain / navigate) ---

    #[derive(ReducerState)]
    struct TabsViewState {
        install_count: i32,
    }

    #[reducer]
    fn tabs_reducer(state: &mut TabsViewState, msg: i32) {
        state.install_count = msg;
    }

    struct ProcsTab;
    impl Layout for ProcsTab {
        fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
            let count = ctx.scope.peek::<TabsReducer>().map_or(0, |s| s.borrow().install_count);
            ctx.scope.push::<TabsReducer>(count + 1);
            Ok(())
        }
        fn view(cx: &mut LayoutCx<'_>) -> windows_reactor::Element {
            cx.outlet()
        }
    }

    struct ServicesLeaf;
    impl Page for ServicesLeaf {
        fn view(cx: &mut PageCx<'_>) -> windows_reactor::Element {
            let _ = cx;
            windows_reactor::Element::default()
        }
    }

    #[derive(Clone, PartialEq)]
    enum TabRoute {
        Processes { context: String },
        Services { context: String },
    }

    const PROCESSES_CHAIN: [SegmentEntry<WinUi>; 2] = [
        layout_entry::<ProcsTab>(),
        segment_entry::<Processes>(),
    ];
    const SERVICES_CHAIN: [SegmentEntry<WinUi>; 2] = [
        layout_entry::<ProcsTab>(),
        segment_entry::<ServicesLeaf>(),
    ];

    impl RouteChain<WinUi> for TabRoute {
        fn chain(&self) -> &'static [SegmentEntry<WinUi>] {
            match self {
                TabRoute::Processes { .. } => &PROCESSES_CHAIN,
                TabRoute::Services { .. } => &SERVICES_CHAIN,
            }
        }
    }

    #[test]
    fn navigating_between_siblings_keeps_the_shared_ancestor_scope() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);
        let uri = AppUri::parse("/ubuntu/processes").unwrap();

        router
            .navigate(
                TabRoute::Processes {
                    context: "ubuntu".to_string(),
                },
                &uri,
            )
            .expect("navigate to processes");
        let tabs_installs_after_first = router
            .active
            .borrow()
            .as_ref()
            .unwrap()
            .scopes[0]
            .state::<TabsReducer>()
            .borrow()
            .install_count;
        assert_eq!(tabs_installs_after_first, 1);

        router
            .navigate(
                TabRoute::Services {
                    context: "ubuntu".to_string(),
                },
                &uri,
            )
            .expect("navigate to services");
        let tabs_installs_after_second = router
            .active
            .borrow()
            .as_ref()
            .unwrap()
            .scopes[0]
            .state::<TabsReducer>()
            .borrow()
            .install_count;
        assert_eq!(
            tabs_installs_after_second, 1,
            "the shared ProcsTab ancestor must not reinstall when only the leaf changes"
        );
    }

    #[test]
    fn navigating_to_the_same_leaf_type_with_different_params_reinstalls_only_the_leaf() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);

        router
            .navigate(
                TabRoute::Processes {
                    context: "ubuntu".to_string(),
                },
                &AppUri::parse("/ubuntu/processes").unwrap(),
            )
            .expect("navigate to processes/ubuntu");
        let leaf_scope_1 = router.active_scope().unwrap();

        router
            .navigate(
                TabRoute::Processes {
                    context: "fedora".to_string(),
                },
                &AppUri::parse("/fedora/processes").unwrap(),
            )
            .expect("navigate to processes/fedora");
        let leaf_scope_2 = router.active_scope().unwrap();

        assert!(
            !Rc::ptr_eq(&leaf_scope_1, &leaf_scope_2),
            "same leaf type but different captured params must reinstall the leaf"
        );
        assert_eq!(
            router
                .active
                .borrow()
                .as_ref()
                .unwrap()
                .scopes[0]
                .state::<TabsReducer>()
                .borrow()
                .install_count,
            1,
            "the ancestor tab, unaffected by the leaf's own param change, stays put"
        );
    }

    #[test]
    fn routes_macro_nesting_produces_the_right_chain() {
        use std::any::TypeId;

        struct Services;
        impl Page for Services {
            fn view(cx: &mut PageCx<'_>) -> windows_reactor::Element {
                let _ = cx;
                windows_reactor::Element::default()
            }
        }

        routes! {
            DerivedRoute {
                layout(ProcsTab) {
                    page(Processes, "/tab/processes/:context") { context: String }
                    page(Services, "/tab/services/:context") { context: String }
                }
            }
        }

        let processes_chain = DerivedRoute::Processes {
            context: "ubuntu".to_string(),
        }
        .chain();
        assert_eq!(processes_chain.len(), 2);
        assert_eq!((processes_chain[0].type_id)(), TypeId::of::<ProcsTab>());
        assert_eq!((processes_chain[1].type_id)(), TypeId::of::<Processes>());

        let services_chain = DerivedRoute::Services {
            context: "ubuntu".to_string(),
        }
        .chain();
        assert_eq!(services_chain.len(), 2);
        assert_eq!((services_chain[0].type_id)(), TypeId::of::<ProcsTab>());
        assert_eq!((services_chain[1].type_id)(), TypeId::of::<Services>());

        // Same ancestor position across siblings - what lets `Router::navigate`
        // detect a shared prefix and keep that Scope alive.
        assert_eq!(
            (processes_chain[0].type_id)(),
            (services_chain[0].type_id)()
        );

        // #[layout]/#[end_layout] don't touch path/parse - purely a chain()
        // concern.
        assert_eq!(
            DerivedRoute::Processes {
                context: "ubuntu".to_string()
            }
            .path(),
            "/tab/processes/ubuntu"
        );
    }

    #[test]
    fn routes_macro_supports_a_zero_field_page() {
        // Regression test: `page(Home, "/")` with no trailing `{ field: Type }`
        // block used to generate an enum whose variant was *declared*
        // struct-style (`Home {}`) but *matched/constructed* unit-style
        // (`Home`) in `path`/`parse`/`chain` - a declaration/usage mismatch
        // that fails to compile with E0533 ("expected unit struct, unit
        // variant or constant, found struct variant"). Every call site must
        // consistently use the same (struct-style) shape.
        struct Home;
        impl Page for Home {
            fn view(cx: &mut PageCx<'_>) -> windows_reactor::Element {
                let _ = cx;
                windows_reactor::Element::default()
            }
        }

        routes! {
            ZeroFieldRoute {
                page(Home, "/")
            }
        }

        let route = ZeroFieldRoute::Home {};
        assert_eq!(route.path(), "/");
        assert_eq!(ZeroFieldRoute::parse("/"), Some(ZeroFieldRoute::Home {}));
    }

    #[test]
    fn navigating_away_from_page_disposes_actor_subscribed_to_global_bus() {
        use guinea_core::actor::Context;
        use guinea_core::actor::event_bus::GlobalEventBus;
        use guinea_core::actor::Message;
        use guinea_macros::{actor, handler};
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone, Debug)]
        struct ProbeEvent(u32);
        impl Message for ProbeEvent {}

        struct ProbeActor {
            seen: Rc<RefCell<Vec<u32>>>,
            /// Simulates a real actor holding a UI port closure produced by
            /// `ctx.port::<Reducer>()`. The port must not keep the page scope
            /// alive via a strong Rc, otherwise Scope -> Addr -> Actor -> Port
            /// -> Scope forms an Rc cycle and the actor never drops on navigation.
            _port: Box<dyn Fn(()) + 'static>,
        }

        impl std::fmt::Debug for ProbeActor {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("ProbeActor").finish()
            }
        }

        impl Drop for ProbeActor {
            fn drop(&mut self) {
                PROBE_DROPPED.with(|d| *d.borrow_mut() = true);
            }
        }

        actor! {
            ProbeActor {
                handlers { ProbeEvent }
            }
        }

        #[handler]
        fn on_probe(this: &mut ProbeActor, ctx: Context<ProbeActor, ProbeEvent>) {
            this.seen.borrow_mut().push(ctx.msg.0);
        }

        thread_local! {
            static PROBE_DROPPED: RefCell<bool> = RefCell::new(false);
        }

        #[derive(ReducerState)]
        struct ProbeState {
            value: u32,
        }

        #[reducer]
        fn probe_reducer(state: &mut ProbeState, _msg: ()) {
            state.value += 1;
        }

        struct ProbePage;
        impl Page for ProbePage {
            fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
                PROBE_DROPPED.with(|d| *d.borrow_mut() = false);
                let addr = ctx.spawn_actor(ProbeActor {
                    seen: Rc::new(RefCell::new(Vec::new())),
                    // Use the real port helper; it must not keep the page scope
                    // alive via a strong Rc, otherwise Scope -> Addr -> Actor
                    // -> Port -> Scope forms a cycle.
                    _port: Box::new(ctx.port::<ProbeReducer>()),
                });
                ctx.subscribe_on_global_bus::<ProbeActor, ProbeEvent>(addr.clone());
                Ok(())
            }

            fn view(_cx: &mut PageCx<'_>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct OtherPage;
        impl Page for OtherPage {
            fn view(_cx: &mut PageCx<'_>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct ProbeLayout;
        impl Layout for ProbeLayout {
            fn view(cx: &mut LayoutCx<'_>) -> windows_reactor::Element {
                cx.outlet()
            }
        }

        routes! {
            ProbeRoute {
                layout(ProbeLayout) {
                    page(ProbePage, "/probe")
                    page(OtherPage, "/other")
                }
            }
        }

        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<WinUi>::new(token);

        router
            .navigate(ProbeRoute::ProbePage {}, &AppUri::parse("/probe").unwrap())
            .expect("navigate to probe");
        assert!(
            GlobalEventBus::has_subscribers::<ProbeEvent>(),
            "actor must be subscribed to the global bus while the page is active"
        );

        router
            .navigate(ProbeRoute::OtherPage {}, &AppUri::parse("/other").unwrap())
            .expect("navigate to other");

        assert!(
            PROBE_DROPPED.with(|d| *d.borrow()),
            "ProbeActor state must be dropped when the page scope is torn down"
        );

        assert!(
            !GlobalEventBus::has_subscribers::<ProbeEvent>(),
            "global bus subscription must be removed when the page scope drops"
        );
    }
}

#[cfg(test)]
mod reducer_macro_tests {
    use guinea_core::scope::{NoopActions, Reducer};
    use guinea_macros::{ReducerState, reducer};

    // Proves `#[reducer]` turns the reduce fn into a marker (fn `widget_reducer`
    // -> type `WidgetReducer`, snake_case name UpperCamelCased) with a full
    // `impl Reducer`, inferring `State`/`Push` from the signature and
    // defaulting `Actions` to `NoopActions` when there's no `#[dispatch]`.
    #[derive(ReducerState)]
    struct WidgetState {
        value: i32,
    }

    #[reducer]
    fn widget_reducer(state: &mut WidgetState, msg: i32) {
        state.value = msg;
    }

    #[test]
    fn reducer_macro_infers_state_push_and_defaults_actions() {
        let _ = WidgetReducer;

        fn assert_shape<R>()
        where
            R: Reducer<State = WidgetState, Push = i32, Actions = NoopActions>,
        {
        }
        assert_shape::<WidgetReducer>();

        let mut state = WidgetState { value: 0 };
        WidgetReducer::reduce(&mut state, 42);
        assert_eq!(state.value, 42);
    }
}
