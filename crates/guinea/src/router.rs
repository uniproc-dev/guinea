use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::scope::{Reducer, Scope};

use crate::feature::FeatureInitContext;
use crate::uri::AppUri;

pub trait Page: Sized + 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut PageCx) -> windows_reactor::Element;
}

pub trait Layout: Sized + 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut LayoutCx) -> windows_reactor::Element;
}


pub struct SegmentEntry {
    pub type_id: fn() -> TypeId,
    pub install: fn(&FeatureInitContext, &AppUri) -> anyhow::Result<()>,
    view: fn(SegmentProps) -> windows_reactor::Element,
}

pub const fn segment_entry<P: Page>() -> SegmentEntry {
    SegmentEntry {
        type_id: TypeId::of::<P>,
        install: P::install,
        view: mount_page::<P>,
    }
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry {
    SegmentEntry {
        type_id: TypeId::of::<L>,
        install: L::install,
        view: mount_layout::<L>,
    }
}

#[derive(Clone)]
struct SegmentProps {
    chain: &'static [SegmentEntry],
    scopes: Rc<Vec<Rc<Scope>>>,
    cursor: usize,
}

impl PartialEq for SegmentProps {
    fn eq(&self, other: &Self) -> bool {
        // Not `std::ptr::eq(self.chain, other.chain)`: sibling leaves under the
        // same layout each get their own `const` chain array (see
        // `routes_dsl.rs`), so the two arrays' identity always differs even
        // when the segment at `cursor` (e.g. a shared layout) is unchanged.
        // Not `Rc::ptr_eq(&self.scopes, &other.scopes)` either: `Router::
        // install_from` wraps the whole scope stack in a fresh `Rc` on every
        // navigation, even when the individual `Rc<Scope>` at this cursor is
        // reused. Compare what actually determines "is this still the same
        // live segment instance": its type at `cursor`, and its own scope.
        self.cursor == other.cursor
            && (self.chain[self.cursor].type_id)() == (other.chain[other.cursor].type_id)()
            && Rc::ptr_eq(&self.scopes[self.cursor], &other.scopes[other.cursor])
    }
}

fn mount_page<P: Page>(props: SegmentProps) -> windows_reactor::Element {
    windows_reactor::component(render_page::<P>, props)
}

fn mount_layout<L: Layout>(props: SegmentProps) -> windows_reactor::Element {
    windows_reactor::component(render_layout::<L>, props)
}

fn render_page<P: Page>(props: &SegmentProps, cx: &mut windows_reactor::RenderCx) -> windows_reactor::Element {
    let mut segment_cx = SegmentCx {
        cx,
        chain: props.chain,
        scopes: props.scopes.clone(),
        cursor: props.cursor,
    };
    P::view(&mut PageCx(&mut segment_cx))
}

fn render_layout<L: Layout>(props: &SegmentProps, cx: &mut windows_reactor::RenderCx) -> windows_reactor::Element {
    let mut segment_cx = SegmentCx {
        cx,
        chain: props.chain,
        scopes: props.scopes.clone(),
        cursor: props.cursor,
    };
    L::view(&mut LayoutCx(&mut segment_cx))
}

fn single_entry_chain<P: Page>() -> &'static [SegmentEntry] {
    thread_local! {
        static CACHE: RefCell<std::collections::HashMap<TypeId, &'static [SegmentEntry]>> =
            RefCell::new(std::collections::HashMap::new());
    }
    CACHE.with(|cache| {
        *cache
            .borrow_mut()
            .entry(TypeId::of::<P>())
            .or_insert_with(|| Box::leak(Box::new([segment_entry::<P>()])) as &'static [SegmentEntry])
    })
}

pub trait RouteChain {
    fn chain(&self) -> &'static [SegmentEntry];
}

pub trait ToUri {
    fn to_uri(&self) -> AppUri;
}

pub struct NavigateHandle<R> {
    router: Rc<Router>,
    set_route: windows_reactor::SetState<R>,
}

impl<R> Clone for NavigateHandle<R> {
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
            set_route: self.set_route.clone(),
        }
    }
}

impl<R> PartialEq for NavigateHandle<R> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.router, &other.router)
    }
}

impl<R> NavigateHandle<R>
where
    R: RouteChain + ToUri + Clone + PartialEq + 'static,
{
    pub fn new(router: Rc<Router>, set_route: windows_reactor::SetState<R>) -> Self {
        Self { router, set_route }
    }

    pub fn to(&self, route: R) {
        let uri = route.to_uri();
        self.router.navigate(route.clone(), &uri).expect("navigate");
        self.set_route.call(route);
    }

    /// A parameterless handler that navigates to `route` - sugar for
    /// `move || nav.to(route.clone())`, matching `SetState::setter`'s own
    /// convention. Handy for `on_click`-style events wired to a fixed route,
    /// e.g. `button("Processes").on_click(nav.to_handler(Route::Processes { .. }))`.
    pub fn to_handler(&self, route: R) -> impl Fn() + Clone + 'static {
        let nav = self.clone();
        move || nav.to(route.clone())
    }
}

/// This route type's `Context` for `NavigateHandle<R>` - the same `Context`
/// value (same `ContextId`) on every call for a given `R`, so a provider and
/// any consumer agree on which slot they're reading/writing. Only the `id`
/// needs to be process-stable (kept in a `TypeId`-keyed cache, not a bare
/// `static` inside this generic fn, which would be a single symbol shared
/// across every `R` - the same footgun `single_entry_chain` works around);
/// `default` is rebuilt fresh each call, cheaply, since it's only a fallback
/// for "nothing provided yet". Private: `RouterRx::render` is the only
/// provider, `RenderCx::use_navigate` the only consumer - nothing else needs
/// the raw `Context`.
fn nav_context<R: 'static>() -> windows_reactor::Context<Option<NavigateHandle<R>>> {
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
    store: amethystate::DefaultStore,
) -> Rc<Router> {
    let slot = cx.use_ref(None::<Rc<Router>>);
    {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(Router::new(token, store)));
        }
    }
    slot.borrow().clone().expect("just initialized above")
}

pub struct RouterRx<R>(std::marker::PhantomData<R>);

impl<R> RouterRx<R>
where
    R: RouteChain + ToUri + Clone + PartialEq + 'static,
{
    pub fn render(cx: &mut windows_reactor::RenderCx, initial: R, store: amethystate::DefaultStore) -> windows_reactor::Element {
        use windows_reactor::ElementExt;

        // Genuinely on the UI thread here - `root()` render callbacks always
        // are - which is what the token attests to.
        let token = guinea_core::actor::UiThreadToken::dangerously_create_token_unchecked();

        let (route, set_route) = cx.use_state(initial);
        let router = scoped_router(cx, token, store);
        router.navigate(route.clone(), &route.to_uri()).expect("navigate");

        let nav = NavigateHandle::new(router.clone(), set_route);
        router
            .render()
            .provide(&nav_context::<R>(), Some(nav))
            .provide(&route_context::<R>(), Some(route))
    }
}

pub trait UseNavigate {
    fn use_navigate<R>(&self) -> NavigateHandle<R>
    where
        R: RouteChain + ToUri + Clone + PartialEq + 'static;
}

impl UseNavigate for windows_reactor::RenderCx {
    fn use_navigate<R>(&self) -> NavigateHandle<R>
    where
        R: RouteChain + ToUri + Clone + PartialEq + 'static,
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

struct SegmentCx<'a> {
    cx: &'a mut windows_reactor::RenderCx,
    chain: &'static [SegmentEntry],
    scopes: Rc<Vec<Rc<Scope>>>,
    cursor: usize,
}

impl<'a> SegmentCx<'a> {
    fn use_reducer<R>(&mut self) -> (R::State, Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone + PartialEq,
    {
        let owner = self.resolve_owner::<R>();
        let initial = owner.state::<R>().borrow().clone();
        let (current, set_current) = self.cx.use_state(initial);

        let owner_for_effect = owner.clone();
        self.cx.use_effect_with_cleanup((), move || {
            // Weak, not a clone of `owner_for_effect`: a strong `Rc<Scope>` here
            // would close a cycle back to the very `Scope` whose `cells` map
            // stores this closure (Scope -> cells -> listeners -> closure ->
            // Rc<Scope>), so the refcount would never reach zero and the scope
            // (and everything it owns - actors included) would leak forever,
            // regardless of what the router does on navigation. If the scope is
            // already gone by the time this fires, there's nothing to notify.
            let owner_for_callback = Rc::downgrade(&owner_for_effect);
            let set_current = set_current.clone();
            let subscription = owner_for_effect.subscribe::<R>(move || {
                let Some(owner) = owner_for_callback.upgrade() else {
                    return;
                };
                let latest = owner.state::<R>().borrow().clone();
                set_current.call(latest);
            });
            Some(move || drop(subscription))
        });

        let dispatch = owner.actions::<R>();
        (current, dispatch)
    }
    
    fn resolve_owner<R: Reducer>(&self) -> Rc<Scope> {
        self.scopes[..=self.cursor]
            .iter()
            .rev()
            .find(|scope| scope.peek::<R>().is_some())
            .unwrap_or(&self.scopes[self.cursor])
            .clone()
    }
    
    fn outlet(&mut self) -> windows_reactor::Element {
        let next = self.cursor + 1;
        assert!(
            next < self.chain.len(),
            "outlet() called on the last segment of the chain (no child to render)"
        );
        let props = SegmentProps {
            chain: self.chain,
            scopes: self.scopes.clone(),
            cursor: next,
        };
        (self.chain[next].view)(props)
    }
}

pub struct PageCx<'a, 'b>(&'b mut SegmentCx<'a>);

impl<'a, 'b> PageCx<'a, 'b> {
    pub fn use_reducer<R>(&mut self) -> (R::State, Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone + PartialEq,
    {
        self.0.use_reducer::<R>()
    }
}

impl<'a, 'b> std::ops::Deref for PageCx<'a, 'b> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.0.cx
    }
}

impl<'a, 'b> std::ops::DerefMut for PageCx<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.cx
    }
}

pub struct LayoutCx<'a, 'b>(&'b mut SegmentCx<'a>);

impl<'a, 'b> LayoutCx<'a, 'b> {
    pub fn use_reducer<R>(&mut self) -> (R::State, Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone + PartialEq,
    {
        self.0.use_reducer::<R>()
    }

    pub fn outlet(&mut self) -> windows_reactor::Element {
        self.0.outlet()
    }
}

impl<'a, 'b> std::ops::Deref for LayoutCx<'a, 'b> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.0.cx
    }
}

impl<'a, 'b> std::ops::DerefMut for LayoutCx<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.cx
    }
}

struct ActiveChain {
    entries: &'static [SegmentEntry],
    scopes: Rc<Vec<Rc<Scope>>>,
}

impl ActiveChain {
    fn root_view(&self) -> windows_reactor::Element {
        let props = SegmentProps {
            chain: self.entries,
            scopes: self.scopes.clone(),
            cursor: 0,
        };
        (self.entries[0].view)(props)
    }
}

fn common_prefix_len(prev: &[SegmentEntry], next: &[SegmentEntry]) -> usize {
    prev.iter()
        .zip(next.iter())
        .take_while(|(a, b)| (a.type_id)() == (b.type_id)())
        .count()
}

pub struct Router {
    active: RefCell<Option<ActiveChain>>,
    prev_route: RefCell<Option<Box<dyn Any>>>,
    token: guinea_core::actor::UiThreadToken,
    /// One `EventBus` per window - shared by every segment installed through
    /// this `Router`, so actors in different features of the same window can
    /// reach each other. See `FeatureInitContext::event_bus`.
    event_bus: Rc<guinea_core::actor::event_bus::EventBus>,
    store: amethystate::DefaultStore,
    debug_registry: Rc<guinea_core::actor::registry::DebugRegistry>,
}

impl Router {
    pub fn new(token: guinea_core::actor::UiThreadToken, store: amethystate::DefaultStore) -> Self {
        Self {
            active: RefCell::new(None),
            prev_route: RefCell::new(None),
            token,
            event_bus: Rc::new(guinea_core::actor::event_bus::EventBus::new()),
            store,
            debug_registry: Rc::new(guinea_core::actor::registry::DebugRegistry::new()),
        }
    }
    
    pub fn activate<P: Page>(&self, uri: &AppUri) -> anyhow::Result<Rc<Scope>> {
        *self.prev_route.borrow_mut() = None;
        self.install_chain(single_entry_chain::<P>(), uri)
    }

    pub fn navigate<R>(&self, route: R, uri: &AppUri) -> anyhow::Result<Rc<Scope>>
    where
        R: RouteChain + PartialEq + Clone + 'static,
    {
        let chain = route.chain();
        let mut shared_len = match &*self.active.borrow() {
            Some(active) => common_prefix_len(active.entries, chain),
            None => 0,
        };

        if shared_len == chain.len() {
            let prev_route = self.prev_route.borrow();
            if let Some(prev) = prev_route.as_ref().and_then(|p| p.downcast_ref::<R>()) {
                if *prev != route {
                    // Same shape, different captured params - the leaf's own
                    // data changed, so it (at least) must reinstall.
                    shared_len -= 1;
                }
            }
        }

        *self.prev_route.borrow_mut() = Some(Box::new(route));
        self.install_from(chain, shared_len, uri)
    }

    fn install_chain(&self, chain: &'static [SegmentEntry], uri: &AppUri) -> anyhow::Result<Rc<Scope>> {
        self.install_from(chain, 0, uri)
    }
    
    fn install_from(
        &self,
        chain: &'static [SegmentEntry],
        shared_len: usize,
        uri: &AppUri,
    ) -> anyhow::Result<Rc<Scope>> {
        
        let mut scopes: Vec<Rc<Scope>> = match self.active.borrow_mut().take() {
            Some(prev) => {
                let mut v = (*prev.scopes).clone();
                v.truncate(shared_len);
                v
            }
            None => Vec::new(),
        };

        for entry in &chain[shared_len..] {
            let scope = Rc::new(Scope::new());
            let ctx = FeatureInitContext {
                scope: scope.clone(),
                token: self.token.clone(),
                event_bus: self.event_bus.clone(),
                store: self.store.clone(),
                debug_registry: self.debug_registry.clone(),
            };
            (entry.install)(&ctx, uri)?;
            scopes.push(scope);
        }

        let leaf = scopes.last().expect("chain is non-empty").clone();
        *self.active.borrow_mut() = Some(ActiveChain {
            entries: chain,
            scopes: Rc::new(scopes),
        });
        Ok(leaf)
    }

    pub fn deactivate(&self) {
        *self.active.borrow_mut() = None;
        *self.prev_route.borrow_mut() = None;
    }
    
    pub fn active_scope(&self) -> Option<Rc<Scope>> {
        self.active.borrow().as_ref().and_then(|a| a.scopes.last().cloned())
    }
    
    pub fn render(&self) -> windows_reactor::Element {
        self.active
            .borrow()
            .as_ref()
            .expect("Router::render called with no active chain - call activate/navigate first")
            .root_view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        ctx.scope
            .push::<ProcessesReducer>(uri.segment(0).expect("test uri always has a segment").to_string());
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

        fn view(cx: &mut PageCx) -> windows_reactor::Element {
            let (state, _dispatch) = cx.use_reducer::<ProcessesReducer>();
            assert_eq!(state.seeded_from, "ubuntu");
            windows_reactor::Element::default()
        }
    }

    #[test]
    fn scoped_router_is_call_site_scoped_not_process_global() {
        let token = UiThreadToken::dangerously_create_token_unchecked();

        let mut cx1 = windows_reactor::RenderCx::new(Rc::new(|| {}));
        cx1.begin_render();
        let router_a = scoped_router(&mut cx1, token.clone(), amethystate::test_utils::unique_store("router-test"));

        // A second render pass of the *same* window (hook slots realign) -
        // must resolve the same Router, not a fresh one.
        cx1.begin_render();
        let router_a2 = scoped_router(&mut cx1, token.clone(), amethystate::test_utils::unique_store("router-test"));
        assert!(
            Rc::ptr_eq(&router_a, &router_a2),
            "re-rendering the same window must reuse its own Router"
        );

        // A different render root (e.g. a second window's own RenderCx) has
        // its own independent hook storage - must get its own Router, no
        // process-wide sharing.
        let mut cx2 = windows_reactor::RenderCx::new(Rc::new(|| {}));
        cx2.begin_render();
        let router_b = scoped_router(&mut cx2, token, amethystate::test_utils::unique_store("router-test"));
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
        let router = Router::new(token, amethystate::test_utils::unique_store("router-test"));
        let uri = AppUri::parse("/ubuntu/processes").unwrap();

        let scope = router.activate::<Processes>(&uri).expect("activate");
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
            chain: single_entry_chain::<Processes>(),
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
        let router = Router::new(token, amethystate::test_utils::unique_store("router-test"));
        let uri = AppUri::parse("/ubuntu/processes").unwrap();
        let scope = router.activate::<Processes>(&uri).expect("activate");

        let rerender_count = Rc::new(std::cell::Cell::new(0));
        let count_for_callback = rerender_count.clone();
        let mut cx = windows_reactor::RenderCx::new(Rc::new(move || {
            count_for_callback.set(count_for_callback.get() + 1);
        }));
        let props = SegmentProps {
            chain: single_entry_chain::<Processes>(),
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
        let router = Router::new(token, amethystate::test_utils::unique_store("router-test"));
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
        fn view(cx: &mut LayoutCx) -> windows_reactor::Element {
            cx.outlet()
        }
    }

    struct ServicesLeaf;
    impl Page for ServicesLeaf {
        fn view(cx: &mut PageCx) -> windows_reactor::Element {
            let _ = cx;
            windows_reactor::Element::default()
        }
    }

    #[derive(Clone, PartialEq)]
    enum TabRoute {
        Processes { context: String },
        Services { context: String },
    }

    const PROCESSES_CHAIN: [SegmentEntry; 2] =
        [layout_entry::<ProcsTab>(), segment_entry::<Processes>()];
    const SERVICES_CHAIN: [SegmentEntry; 2] =
        [layout_entry::<ProcsTab>(), segment_entry::<ServicesLeaf>()];

    impl RouteChain for TabRoute {
        fn chain(&self) -> &'static [SegmentEntry] {
            match self {
                TabRoute::Processes { .. } => &PROCESSES_CHAIN,
                TabRoute::Services { .. } => &SERVICES_CHAIN,
            }
        }
    }

    #[test]
    fn navigating_between_siblings_keeps_the_shared_ancestor_scope() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::new(token, amethystate::test_utils::unique_store("router-test"));
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
        let router = Router::new(token, amethystate::test_utils::unique_store("router-test"));

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
            fn view(cx: &mut PageCx) -> windows_reactor::Element {
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
            fn view(cx: &mut PageCx) -> windows_reactor::Element {
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
