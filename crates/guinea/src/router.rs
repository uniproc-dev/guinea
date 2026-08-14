use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use guinea_core::scope::{Reducer, Scope};

use crate::feature::FeatureInitContext;
use crate::uri::AppUri;

pub trait Page: Sized + 'static {

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
    pub cache_state: bool,
}

pub const fn segment_entry<P: Page>() -> SegmentEntry {
    SegmentEntry {
        type_id: TypeId::of::<P>,
        install: P::install,
        view: mount_page::<P>,
        cache_state: P::CACHE_STATE_IN_MEMORY,
    }
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry {
    SegmentEntry {
        type_id: TypeId::of::<L>,
        install: L::install,
        view: mount_layout::<L>,
        cache_state: false,
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
        crate::app::route_changed(&uri.to_string());
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
) -> Rc<Router> {
    let slot = cx.use_ref(None::<Rc<Router>>);
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
    R: RouteChain + ToUri + Clone + PartialEq + 'static,
{
    pub fn render(cx: &mut windows_reactor::RenderCx, initial: R) -> windows_reactor::Element {
        use windows_reactor::ElementExt;

        // Genuinely on the UI thread here - `root()` render callbacks always
        // are - which is what the token attests to.
        let token = guinea_core::actor::UiThreadToken::dangerously_create_token_unchecked();

        let (route, set_route) = cx.use_state(initial);
        let router = scoped_router(cx, token);
        router.navigate(route.clone(), &route.to_uri()).expect("navigate");

        let nav = NavigateHandle::new(router.clone(), set_route);
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
    R: RouteChain + ToUri + Clone + PartialEq + 'static,
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
        let binding = self.resolve_owner::<R>().binding::<R>();
        let (current, set_current) = self.cx.use_state(binding.get());

        // The subscription ends with this component instance, not with the
        // scope: an unmounted component that kept listening would call
        // `set_current` on a state slot nobody renders.
        let binding_for_effect = binding.clone();
        self.cx.use_effect_with_cleanup((), move || {
            let set_current = set_current.clone();
            let subscription =
                binding_for_effect.on_change(move |latest| set_current.call(latest.clone()));
            Some(move || drop(subscription))
        });

        (current, binding.actions())
    }
    
    fn resolve_owner<R: Reducer>(&self) -> Rc<Scope> {
        // Ownership is decided by `note_reducer_owner::<R>()` - set
        // synchronously inside `ctx.port`/`ctx.actions`/`ctx.seed_reducer`
        // during `install()`, always before the first render for this
        // segment. That makes this a reliable signal (unlike checking
        // whether `R`'s state cell already exists, which depends on
        // render/actor-response timing, not on ownership), so a miss here
        // is a genuine setup bug, not a normal race - panic instead of
        // silently treating the current scope as the owner.
        let owner = self.scopes[..=self.cursor]
            .iter()
            .rev()
            .find(|scope| scope.has_feature::<R>());
        owner
            .unwrap_or_else(|| {
                panic!(
                    "use_reducer::<{}>() found no scope (this one or any ancestor) whose \
                     install() called ctx.port/ctx.actions/ctx.seed_reducer for it. Either \
                     this route never installs the feature that owns it, or that feature \
                     installs in the wrong branch of the route tree.",
                    std::any::type_name::<R>()
                )
            })
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct StateCacheKey {
    segment_index: usize,
    type_id: TypeId,
}

const MAX_CACHED_STATES: usize = 10;

struct StateCache {
    /// Cached reducer states, keyed by segment position and type.
    entries: HashMap<StateCacheKey, HashMap<TypeId, Rc<dyn Any>>>,
    /// Order of insertion for LRU eviction.
    order: VecDeque<StateCacheKey>,
}

impl StateCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn insert(&mut self, key: StateCacheKey, states: HashMap<TypeId, Rc<dyn Any>>) {
        if self.entries.insert(key, states).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MAX_CACHED_STATES {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    fn take(&mut self, key: StateCacheKey) -> Option<HashMap<TypeId, Rc<dyn Any>>> {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| *k != key);
        }
        self.entries.remove(&key)
    }
}

pub struct Router {
    active: RefCell<Option<ActiveChain>>,
    prev_route: RefCell<Option<Box<dyn Any>>>,
    token: guinea_core::actor::UiThreadToken,
    /// One `EventBus` per window - shared by every segment installed through
    /// this `Router`, so actors in different features of the same window can
    /// reach each other. See `FeatureInitContext::event_bus`.
    event_bus: Rc<guinea_core::actor::event_bus::EventBus>,
    debug_registry: Rc<guinea_core::actor::registry::DebugRegistry>,
    state_cache: RefCell<StateCache>,
}

impl Router {
    pub fn new(token: guinea_core::actor::UiThreadToken) -> Self {
        Self {
            active: RefCell::new(None),
            prev_route: RefCell::new(None),
            token,
            event_bus: Rc::new(guinea_core::actor::event_bus::EventBus::new()),
            debug_registry: Rc::new(guinea_core::actor::registry::DebugRegistry::new()),
            state_cache: RefCell::new(StateCache::new()),
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
        // Capture the previous chain/scopes before dropping them, so we can
        // snapshot reducer states for cache-eligible segments.
        let prev = self.active.borrow_mut().take();

        if let Some(prev) = &prev {
            let mut cache = self.state_cache.borrow_mut();
            for (index, (entry, scope)) in prev
                .entries
                .iter()
                .zip(prev.scopes.iter())
                .enumerate()
                .skip(shared_len)
            {
                if entry.cache_state {
                    let key = StateCacheKey {
                        segment_index: index,
                        type_id: (entry.type_id)(),
                    };
                    cache.insert(key, scope.snapshot_states());
                }
            }
        }

        let mut scopes: Vec<Rc<Scope>> = match prev {
            Some(prev) => {
                let mut v = (*prev.scopes).clone();
                v.truncate(shared_len);
                v
            }
            None => Vec::new(),
        };

        for (index, entry) in chain.iter().enumerate().skip(shared_len) {
            let scope = Rc::new(Scope::new());

            if entry.cache_state {
                let key = StateCacheKey {
                    segment_index: index,
                    type_id: (entry.type_id)(),
                };
                if let Some(states) = self.state_cache.borrow_mut().take(key) {
                    scope.restore_states(states);
                }
            }

            let ctx = FeatureInitContext {
                scope: scope.clone(),
                // Snapshot of everything built so far this loop - root to
                // this segment's immediate parent, never including `scope`
                // itself. `inherit()` walks this to find an ancestor that
                // already `install()`-ed the feature being asked for.
                ancestors: Rc::from(scopes.clone()),
                token: self.token.clone(),
                event_bus: self.event_bus.clone(),
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
        let router = Router::new(token);
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
        let router = Router::new(token);
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
        let router = Router::new(token);
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
        let router = Router::new(token);
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
        let router = Router::new(token);

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

            fn view(_cx: &mut PageCx) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct OtherPage;
        impl Page for OtherPage {
            fn view(_cx: &mut PageCx) -> windows_reactor::Element {
                windows_reactor::Element::default()
            }
        }

        struct ProbeLayout;
        impl Layout for ProbeLayout {
            fn view(cx: &mut LayoutCx) -> windows_reactor::Element {
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
        let router = Router::new(token);

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
