#![cfg(feature = "winui")]

//! The router driven through the facade, the way an application sees it.
//!
//! Integration rather than unit tests on purpose: `routes!` resolves paths
//! through `proc_macro_crate`, which answers `Itself` inside the crate and
//! `Name` outside it - only the second case is what an application gets.

mod routing {
    use guinea::winui::*;
    use guinea::feature::FeatureInitContext;
    use std::rc::Rc;
    use guinea::router::*;
    use guinea_core::actor::UiThreadToken;
    use guinea_core::feature::Bound;
    use guinea_core::scope::Reducer;
    use guinea_macros::{routes, segment};

    /// The reducer is the state; `Processes` below is the page that shows it.
    #[derive(Default, Clone, PartialEq, Debug)]
    struct Listing {
        seeded_from: String,
    }

    impl Reducer for Listing {
        type Update = String;

        fn reduce(&mut self, from: String) {
            self.seeded_from = from;
        }
    }

    fn install_processes(
        ctx: &FeatureInitContext,
        params: &ProcessesParams,
    ) -> anyhow::Result<Bound<Listing>> {
        Ok(ctx
            .state::<Listing>()
            .seed(Listing {
                seeded_from: params.context.clone(),
            })
            .plain())
    }

    struct Processes;

    routes! {
        AppRoute {
            page(Processes) link("/:context/processes") { context: String }
        }
    }

    #[segment]
    impl Page for Processes {
        type Params = ProcessesParams;
        /// The claim itself, which is what makes `Listing` readable here.
        type Installs = Bound<Listing>;

        fn install(
            ctx: &FeatureInitContext,
            params: &ProcessesParams,
        ) -> anyhow::Result<Bound<Listing>> {
            install_processes(ctx, params)
        }

        fn view(cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
            let (state, _dispatch) = cx.use_reducer::<Listing, _>();
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

    #[segment]
    impl Page for NeedsAService {
        type Params = ();

        fn install(ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<()> {
            let greeting = ctx.require::<Greeting>()?;
            assert_eq!(greeting.0, "hello");
            Ok(())
        }

        fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
            windows_reactor::Element::default()
        }
    }

    #[test]
    fn a_page_can_read_what_an_application_plugin_provided() {
        let token = UiThreadToken::dangerously_create_token_unchecked();

        let runtime = guinea::app::GuineaApp::new()
            .plugin(GreetingPlugin)
            .install(token.clone())
            .expect("install");
        guinea_app::app::install_runtime(runtime);

        let router = Rc::new(Router::<WinUi>::new(token));

        router
            .activate(page_chain::<NeedsAService>(), vec![Box::new(())])
            .expect("the page's install must find the service the plugin provided");
    }

    #[test]
    fn a_page_without_an_application_gets_a_plain_error_not_a_panic() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Rc::new(Router::<WinUi>::new(token));

        let err = router
            .activate(page_chain::<NeedsAService>(), vec![Box::new(())])
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
        assert_eq!(route.link().as_deref(), Some("/ubuntu/processes"));
        assert_eq!(
            AppRoute::parse("/ubuntu/processes"),
            Some(AppRoute::Processes {
                context: "ubuntu".to_string()
            })
        );
    }

    #[test]
    fn a_capture_that_looks_like_a_path_stays_one_segment() {
        // `Display` used to write this straight into the link, so the address
        // read as three segments and parsed back as a different route - or as
        // nothing. The escaping lives in `LinkValue`; this checks it reaches
        // the generated `link`/`parse` rather than only the trait's own tests.
        let route = AppRoute::Processes {
            context: "a/b c".to_string(),
        };

        let link = route.link().expect("an addressable route");
        assert_eq!(link, "/a%2Fb%20c/processes");
        assert_eq!(link.matches('/').count(), 2, "still two separators");
        assert_eq!(AppRoute::parse(&link), Some(route));
    }

    #[test]
    fn activating_a_page_runs_install_and_view_can_use_it() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Rc::new(Router::<WinUi>::new(token));

        let scope = router
            .activate(
                page_chain::<Processes>(),
                vec![Box::new(ProcessesParams {
                    context: "ubuntu".to_string(),
                })],
            )
            .expect("activate");
        assert!(router.active_scope().is_some());

        // install already ran (synchronously, inside activate) and seeded state.
        let state = scope.state::<Listing>();
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
        let router = Rc::new(Router::<WinUi>::new(token));
        let scope = router
            .activate(
                page_chain::<Processes>(),
                vec![Box::new(ProcessesParams {
                    context: "ubuntu".to_string(),
                })],
            )
            .expect("activate");

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
        scope.push::<Listing>("debian".to_string());

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
        let router = Rc::new(Router::<WinUi>::new(token));
        let route = AppRoute::Processes { context: "ubuntu".to_string() };

        let scope_a = router
            .navigate(route.clone())
            .expect("first navigate")
            .scope()
            .expect("nothing guards this route");
        let scope_b = router
            .navigate(route)
            .expect("second navigate, same route value")
            .scope()
            .expect("nothing guards this route");

        assert!(
            Rc::ptr_eq(&scope_a, &scope_b),
            "re-navigating to an unchanged route must reuse the exact same Scope, \
             not silently reinstall it - anything accumulating state in that Scope \
             (e.g. a live-updating chart) would otherwise reset on every single render"
        );
    }

    // --- nested layout persistence (RouteChain / navigate) ---

    #[derive(Default, Clone, PartialEq, Debug)]
    struct Tabs {
        install_count: i32,
    }

    impl Reducer for Tabs {
        type Update = i32;

        fn reduce(&mut self, count: i32) {
            self.install_count = count;
        }
    }

    struct ProcsTab;
    #[segment]
    impl Layout for ProcsTab {
        // `TabRoute` below is written by hand and declares that this layout
        // derives nothing from its pages - so a leaf's own parameter changing
        // is none of its business, which is what the tests here check.
        type Params = ();

        fn install(ctx: &FeatureInitContext, _params: &Self::Params) -> anyhow::Result<()> {
            let count = ctx.scope.peek::<Tabs>().map_or(0, |s| s.borrow().install_count);
            ctx.scope.push::<Tabs>(count + 1);
            Ok(())
        }
        fn view(cx: &mut LayoutCx<'_, Self>) -> windows_reactor::Element {
            cx.outlet()
        }
    }

    struct ServicesLeaf;
    #[segment]
    impl Page for ServicesLeaf {
        type Params = ();

        fn view(cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
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

        fn params(&self) -> Vec<Box<dyn std::any::Any>> {
            match self {
                TabRoute::Processes { context } => vec![
                    Box::new(()),
                    Box::new(ProcessesParams {
                        context: context.clone(),
                    }),
                ],
                TabRoute::Services { .. } => vec![Box::new(()), Box::new(())],
            }
        }

        fn name(&self) -> &'static str {
            match self {
                TabRoute::Processes { .. } => "Processes",
                TabRoute::Services { .. } => "Services",
            }
        }
    }

    #[test]
    fn a_layout_is_not_the_same_props_when_the_page_under_it_changes() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Rc::new(Router::<WinUi>::new(token));

        let props_at_layout = |route: TabRoute| {
            router.navigate(route).expect("navigate");
            SegmentProps::<WinUi> {
                chain: router.active_chain().expect("a chain is active"),
                scopes: router.active_scopes().expect("scopes are installed"),
                cursor: 0,
            }
        };

        let showing_processes = props_at_layout(TabRoute::Processes {
            context: "ubuntu".to_string(),
        });
        let showing_services = props_at_layout(TabRoute::Services {
            context: "ubuntu".to_string(),
        });

        // The layout itself is unchanged - same type, same scope, same
        // cursor - and comparing only that is what used to make a reconciler
        // skip it. Skipping the layout skips its `outlet()`, so the old page
        // stayed on screen until something unrelated forced a re-render.
        assert_eq!(showing_processes.identity(), showing_services.identity());
        assert!(
            showing_processes != showing_services,
            "a layout showing a different page is not the same props"
        );
    }

    #[test]
    fn navigating_between_siblings_keeps_the_shared_ancestor_scope() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Rc::new(Router::<WinUi>::new(token));

        router
            .navigate(TabRoute::Processes {
                context: "ubuntu".to_string(),
            })
            .expect("navigate to processes");
        let tabs_installs_after_first = router
            .scope_at(0)
            .unwrap()
            .state::<Tabs>()
            .borrow()
            .install_count;
        assert_eq!(tabs_installs_after_first, 1);

        router
            .navigate(TabRoute::Services {
                context: "ubuntu".to_string(),
            })
            .expect("navigate to services");
        let tabs_installs_after_second = router
            .scope_at(0)
            .unwrap()
            .state::<Tabs>()
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
        let router = Rc::new(Router::<WinUi>::new(token));

        router
            .navigate(TabRoute::Processes {
                context: "ubuntu".to_string(),
            })
            .expect("navigate to processes/ubuntu");
        let leaf_scope_1 = router.active_scope().unwrap();

        router
            .navigate(TabRoute::Processes {
                context: "fedora".to_string(),
            })
            .expect("navigate to processes/fedora");
        let leaf_scope_2 = router.active_scope().unwrap();

        assert!(
            !Rc::ptr_eq(&leaf_scope_1, &leaf_scope_2),
            "same leaf type but different captured params must reinstall the leaf"
        );
        assert_eq!(
            router
                .scope_at(0)
                .unwrap()
                .state::<Tabs>()
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
        #[segment]
        impl Page for Services {
            type Params = ServicesParams;

            fn view(cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                let _ = cx;
                windows_reactor::Element::default()
            }
        }

        /// A page of its own rather than the module's `Processes`, for the same
        /// reason as `DerivedTab` below: a page belongs to one tree, since its
        /// ancestry is part of what the compiler reads off it.
        struct DerivedProcesses;
        #[segment]
        impl Page for DerivedProcesses {
            type Params = DerivedProcessesParams;

            fn view(cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                let _ = cx;
                windows_reactor::Element::default()
            }
        }

        /// A layout of its own rather than the module's `ProcsTab`: this tree
        /// derives parameters for it, and a layout has one `Params` for every
        /// tree it appears in.
        struct DerivedTab;
        #[segment]
        impl Layout for DerivedTab {
            type Params = DerivedTabParams;

            fn view(cx: &mut LayoutCx<'_, Self>) -> windows_reactor::Element {
                cx.outlet()
            }
        }

        routes! {
            DerivedRoute {
                layout(DerivedTab) {
                    page(DerivedProcesses) link("/tab/processes/:context") { context: String }
                    page(Services) link("/tab/services/:context") { context: String }
                }
            }
        }

        let processes_chain = DerivedRoute::DerivedProcesses {
            context: "ubuntu".to_string(),
        }
        .chain();
        assert_eq!(processes_chain.len(), 2);
        assert_eq!((processes_chain[0].type_id)(), TypeId::of::<DerivedTab>());
        assert_eq!(
            (processes_chain[1].type_id)(),
            TypeId::of::<DerivedProcesses>()
        );

        let services_chain = DerivedRoute::Services {
            context: "ubuntu".to_string(),
        }
        .chain();
        assert_eq!(services_chain.len(), 2);
        assert_eq!((services_chain[0].type_id)(), TypeId::of::<DerivedTab>());
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
            DerivedRoute::DerivedProcesses {
                context: "ubuntu".to_string()
            }
            .link()
            .as_deref(),
            Some("/tab/processes/ubuntu")
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
        #[segment]
        impl Page for Home {
            type Params = HomeParams;

            fn view(cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                let _ = cx;
                windows_reactor::Element::default()
            }
        }

        routes! {
            ZeroFieldRoute {
                page(Home) link("/")
            }
        }

        let route = ZeroFieldRoute::Home {};
        assert_eq!(route.link().as_deref(), Some("/"));
        assert_eq!(ZeroFieldRoute::parse("/"), Some(ZeroFieldRoute::Home {}));
    }

    #[test]
    fn a_page_without_a_link_has_no_address_at_all() {
        struct Inner;
        #[segment]
        impl Page for Inner {
            type Params = InnerParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct Shared;
        #[segment]
        impl Page for Shared {
            type Params = SharedParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        routes! {
            MixedRoute {
                page(Inner) { token: String }
                page(Shared) link("/shared/:id") { id: String }
            }
        }

        let inner = MixedRoute::Inner {
            token: "secret".to_string(),
        };
        assert_eq!(inner.link(), None, "nothing outside can name it");
        assert_eq!(inner.name(), "Inner", "but a log still can");

        let surface = MixedRoute::deep_links();
        assert_eq!(
            surface.iter().map(|link| link.path).collect::<Vec<_>>(),
            ["/shared/:id"],
            "the external surface is exactly what agreed to be on it"
        );
        assert_eq!(surface[0].tree, "MixedRoute");
        assert_eq!(surface[0].route, "Shared");
        assert_eq!(
            surface[0].captures,
            [guinea::link::Capture {
                name: "id",
                ty: "String"
            }],
            "the capture carries what would silently change the address"
        );

        assert_eq!(
            MixedRoute::parse("/secret"),
            None,
            "an unaddressed page is unreachable from a path, whatever the path"
        );
        assert_eq!(
            MixedRoute::parse("/shared/42"),
            Some(MixedRoute::Shared {
                id: "42".to_string()
            })
        );
    }

    #[test]
    fn a_literal_wins_over_a_capture_whatever_the_declaration_order() {
        struct Settings;
        #[segment]
        impl Page for Settings {
            type Params = SettingsParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct Host;
        #[segment]
        impl Page for Host {
            type Params = HostParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        // The capture is declared *first*, and matches anything of this
        // length. Trying arms in declaration order gave it "/settings/pods"
        // and the literal page was unreachable.
        routes! {
            ShadowRoute {
                page(Host) link("/:name/pods") { name: String }
                page(Settings) link("/settings/pods")
            }
        }

        assert_eq!(
            ShadowRoute::parse("/settings/pods"),
            Some(ShadowRoute::Settings {}),
            "the literal is more specific, so it wins wherever it was declared"
        );
        assert_eq!(
            ShadowRoute::parse("/kube-01/pods"),
            Some(ShadowRoute::Host {
                name: "kube-01".to_string()
            })
        );
    }

    #[test]
    fn a_capture_that_does_not_parse_falls_through_to_the_next_branch() {
        struct ById;
        #[segment]
        impl Page for ById {
            type Params = ByIdParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct ByName;
        #[segment]
        impl Page for ByName {
            type Params = ByNameParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        // Two branches of the same shape, told apart by whether the segment
        // parses as a number. The old emission used `?` on the parse, which
        // returned from `parse` itself - so a non-numeric path matched
        // nothing at all instead of falling through.
        routes! {
            EitherRoute {
                page(ById) link("/job/:id") { id: u32 }
                page(ByName) link("/name/:who") { who: String }
            }
        }

        assert_eq!(
            EitherRoute::parse("/job/42"),
            Some(EitherRoute::ById { id: 42 })
        );
        assert_eq!(
            EitherRoute::parse("/name/ada"),
            Some(EitherRoute::ByName {
                who: "ada".to_string()
            }),
            "a branch whose capture failed to parse must not end the search"
        );
        assert_eq!(EitherRoute::parse("/job/ada"), None);
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
            _push: Box<dyn Fn(()) + 'static>,
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

        #[derive(Default)]
        struct Probe {
            value: u32,
        }

        impl Reducer for Probe {
            type Update = ();
    
            fn reduce(&mut self, _: ()) {
                self.value += 1;
            }
        }

        struct ProbePage;
        #[segment]
        impl Page for ProbePage {
            type Params = ProbePageParams;

            fn install(ctx: &FeatureInitContext, _params: &ProbePageParams) -> anyhow::Result<()> {
                PROBE_DROPPED.with(|d| *d.borrow_mut() = false);
                // The real way in; it must not keep the page scope alive via a
                // strong Rc, otherwise Scope -> Addr -> Actor -> Push -> Scope
                // forms a cycle.
                let push = ctx.state::<Probe>().plain().port();
                let addr = ctx.spawn_actor(ProbeActor {
                    seen: Rc::new(RefCell::new(Vec::new())),
                    _push: Box::new(move |()| push.send(())),
                });
                ctx.subscribe_on_global_bus::<ProbeActor, ProbeEvent>(addr.clone());
                Ok(())
            }

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct OtherPage;
        #[segment]
        impl Page for OtherPage {
            type Params = OtherPageParams;

            fn view(_cx: &mut PageCx<'_, Self>) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct ProbeLayout;
        #[segment]
        impl Layout for ProbeLayout {
            type Params = ProbeLayoutParams;

            fn view(cx: &mut LayoutCx<'_, Self>) -> windows_reactor::Element {
                cx.outlet()
            }
        }

        routes! {
            ProbeRoute {
                layout(ProbeLayout) {
                    page(ProbePage)
                    page(OtherPage)
                }
            }
        }

        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Rc::new(Router::<WinUi>::new(token));

        router
            .navigate(ProbeRoute::ProbePage {})
            .expect("navigate to probe");
        assert!(
            GlobalEventBus::has_subscribers::<ProbeEvent>(),
            "actor must be subscribed to the global bus while the page is active"
        );

        router
            .navigate(ProbeRoute::OtherPage {})
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

mod a_reducer_is_plain_rust {
    use guinea_core::scope::{Reducer, Scope};
    use std::rc::Rc;

    /// What `#[reducer]`, `#[derive(ReducerState)]`, `#[dispatch]`, `#[port]`
    /// and half of `messages!` used to produce between them. The whole
    /// declaration is here, and the central type of the feature - the one
    /// every other file names - is written rather than derived from a
    /// function's name by changing its case.
    #[derive(Default)]
    struct Widget {
        value: i32,
    }

    impl Reducer for Widget {
        type Update = i32;

        fn reduce(&mut self, to: i32) {
            self.value = to;
        }
    }

    #[test]
    fn the_state_is_the_reducer() {
        let scope = Rc::new(Scope::new());
        scope.push::<Widget>(42);

        assert_eq!(scope.state::<Widget>().borrow().value, 42);
    }
}
