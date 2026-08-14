use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use guinea_core::binding::ReducerBinding;
use guinea_core::scope::{Reducer, Scope};

use crate::feature::FeatureInitContext;
use crate::uri::AppUri;

/// A UI backend, described entirely by the types it works in.
///
/// Deliberately method-free: anything that looks like behaviour is carried as
/// data instead - mounting is a function pointer in [`SegmentEntry`],
/// navigation is a [`RouteSink`], identity is a [`SegmentIdentity`]. A trait
/// with methods here would have to name a common denominator of every
/// toolkit, and each backend would then be forced to fake the parts that do
/// not fit its model.
pub trait Ui: Sized + 'static {
    /// What a view produces: an element tree for a reconciler, a deferred
    /// draw closure for immediate mode, `()` for a backend that pushes into
    /// retained properties.
    type View: 'static;

    /// What a page's `view` is handed. Separate from `LayoutCx` so a page
    /// cannot call `outlet()` - it has no child to render.
    type PageCx<'a>;

    /// What a layout's `view` is handed.
    type LayoutCx<'a>;
}

pub trait Page<U: Ui>: Sized + 'static {

    /// When `true`, the router keeps this page's reducer states in memory
    /// while the page is not mounted. The page's scope (and therefore its
    /// actors) is still torn down, but when the user returns the UI will see
    /// the last cached state immediately instead of starting from defaults.
    const CACHE_STATE_IN_MEMORY: bool = false;

    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut U::PageCx<'_>) -> U::View;
}

pub trait Layout<U: Ui>: Sized + 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut U::LayoutCx<'_>) -> U::View;
}

pub struct SegmentEntry<U: Ui> {
    pub type_id: fn() -> TypeId,
    pub install: fn(&FeatureInitContext, &AppUri) -> anyhow::Result<()>,
    /// Built by the backend: the agnostic half only calls it.
    pub mount: fn(SegmentProps<U>) -> U::View,
    pub cache_state: bool,
}

/// Where a segment sits while it renders: the chain it belongs to, the scope
/// stack that chain installed, and which of the two this segment is.
pub struct SegmentProps<U: Ui> {
    pub chain: &'static [SegmentEntry<U>],
    pub scopes: Rc<Vec<Rc<Scope>>>,
    pub cursor: usize,
}

impl<U: Ui> Clone for SegmentProps<U> {
    fn clone(&self) -> Self {
        Self {
            chain: self.chain,
            scopes: self.scopes.clone(),
            cursor: self.cursor,
        }
    }
}

/// What makes a mounted segment *this* segment: where it sits in the chain,
/// what it is, and which scope it runs in.
///
/// Data rather than a trait method, because backends need it for different
/// things - a reconciler compares it to decide whether to re-mount, an
/// immediate-mode backend ignores it entirely. `derive`d rather than
/// hand-written so a field added later is compared automatically instead of
/// being silently forgotten.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SegmentIdentity {
    pub cursor: usize,
    pub segment: TypeId,
    pub scope: usize,
}

impl<U: Ui> PartialEq for SegmentProps<U> {
    fn eq(&self, other: &Self) -> bool {
        // Compared through `identity`, not field by field. Not
        // `std::ptr::eq(self.chain, other.chain)`: sibling leaves under the
        // same layout each get their own `const` chain array (see
        // `routes_dsl.rs`), so the two arrays' identity always differs even
        // when the segment at `cursor` (e.g. a shared layout) is unchanged.
        // Not `Rc::ptr_eq(&self.scopes, &other.scopes)` either:
        // `install_from` wraps the whole scope stack in a fresh `Rc` on every
        // navigation, even when the individual `Rc<Scope>` at this cursor is
        // reused.
        self.identity() == other.identity()
    }
}

impl<U: Ui> SegmentProps<U> {
    pub fn identity(&self) -> SegmentIdentity {
        SegmentIdentity {
            cursor: self.cursor,
            segment: (self.chain[self.cursor].type_id)(),
            scope: Rc::as_ptr(&self.scopes[self.cursor]) as usize,
        }
    }

    /// The reducer's owning scope, and a binding to its state.
    ///
    /// Ownership is decided by `note_reducer_owner::<R>()` - set
    /// synchronously inside `ctx.port`/`ctx.actions`/`ctx.seed_reducer`
    /// during `install()`, always before the first render for this segment.
    /// That makes this a reliable signal (unlike checking whether `R`'s state
    /// cell already exists, which depends on render/actor-response timing,
    /// not on ownership), so a miss here is a genuine setup bug, not a normal
    /// race - it panics instead of silently treating the current scope as the
    /// owner.
    pub fn binding<R: Reducer>(&self) -> ReducerBinding<R> {
        let owner = self.scopes[..=self.cursor]
            .iter()
            .rev()
            .find(|scope| scope.has_feature::<R>())
            .unwrap_or_else(|| {
                panic!(
                    "use_reducer::<{}>() found no scope (this one or any ancestor) whose                      install() called ctx.port/ctx.actions/ctx.seed_reducer for it. Either                      this route never installs the feature that owns it, or that feature                      installs in the wrong branch of the route tree.",
                    std::any::type_name::<R>()
                )
            });
        owner.binding::<R>()
    }

    /// Mounts the next segment down the chain. Layouts only - a page has no
    /// child, which is why `Ui::PageCx` is a separate type.
    pub fn outlet(&self) -> U::View {
        let next = self.cursor + 1;
        assert!(
            next < self.chain.len(),
            "outlet() called on the last segment of the chain (no child to render)"
        );
        (self.chain[next].mount)(SegmentProps {
            chain: self.chain,
            scopes: self.scopes.clone(),
            cursor: next,
        })
    }
}

pub const fn segment_entry<U: Ui, P: Page<U>>(mount: fn(SegmentProps<U>) -> U::View) -> SegmentEntry<U> {
    SegmentEntry {
        type_id: TypeId::of::<P>,
        install: P::install,
        mount,
        cache_state: P::CACHE_STATE_IN_MEMORY,
    }
}

pub const fn layout_entry<U: Ui, L: Layout<U>>(mount: fn(SegmentProps<U>) -> U::View) -> SegmentEntry<U> {
    SegmentEntry {
        type_id: TypeId::of::<L>,
        install: L::install,
        mount,
        cache_state: false,
    }
}

/// A one-segment chain for `P`, leaked once per page type.
///
/// Leaked rather than owned because `SegmentEntry` chains are `&'static` -
/// `routes!` generates them as `const` arrays, and this is the path for a page
/// activated without a route tree (tests, and `Router::activate`).
pub(crate) fn single_entry_chain<U: Ui, P: Page<U>>(
    mount: fn(SegmentProps<U>) -> U::View,
) -> &'static [SegmentEntry<U>] {
    Box::leak(Box::new([segment_entry::<U, P>(mount)])) as &'static [SegmentEntry<U>]
}

pub trait RouteChain<U: Ui> {
    fn chain(&self) -> &'static [SegmentEntry<U>];
}

pub trait ToUri {
    fn to_uri(&self) -> AppUri;
}

/// Where a navigation goes once the router has accepted it - a reconciler's
/// state setter, a field in a TUI's application struct, a Slint property.
///
/// Data, not a trait: the agnostic half publishes the new route, the backend
/// decides what publishing means.
pub struct RouteSink<R> {
    publish: Rc<dyn Fn(R)>,
}

impl<R> RouteSink<R> {
    pub fn new(publish: impl Fn(R) + 'static) -> Self {
        Self {
            publish: Rc::new(publish),
        }
    }

    pub fn publish(&self, route: R) {
        (self.publish)(route)
    }
}

impl<R> Clone for RouteSink<R> {
    fn clone(&self) -> Self {
        Self {
            publish: self.publish.clone(),
        }
    }
}

pub struct NavigateHandle<U: Ui, R> {
    router: Rc<Router<U>>,
    sink: RouteSink<R>,
}

impl<U: Ui, R> Clone for NavigateHandle<U, R> {
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
            sink: self.sink.clone(),
        }
    }
}

impl<U: Ui, R> PartialEq for NavigateHandle<U, R> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.router, &other.router)
    }
}

impl<U: Ui, R> NavigateHandle<U, R>
where
    R: RouteChain<U> + ToUri + Clone + PartialEq + 'static,
{
    pub fn new(router: Rc<Router<U>>, sink: RouteSink<R>) -> Self {
        Self { router, sink }
    }

    pub fn to(&self, route: R) {
        let uri = route.to_uri();
        self.router.navigate(route.clone(), &uri).expect("navigate");
        crate::app::route_changed(&uri.to_string());
        self.sink.publish(route);
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
pub(crate) struct ActiveChain<U: Ui> {
    pub(crate) entries: &'static [SegmentEntry<U>],
    pub(crate) scopes: Rc<Vec<Rc<Scope>>>,
}

impl<U: Ui> ActiveChain<U> {
    fn root_view(&self) -> U::View {
        let props = SegmentProps {
            chain: self.entries,
            scopes: self.scopes.clone(),
            cursor: 0,
        };
        (self.entries[0].mount)(props)
    }
}

fn common_prefix_len<U: Ui>(prev: &[SegmentEntry<U>], next: &[SegmentEntry<U>]) -> usize {
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

pub struct Router<U: Ui> {
    pub(crate) active: RefCell<Option<ActiveChain<U>>>,
    prev_route: RefCell<Option<Box<dyn Any>>>,
    token: guinea_core::actor::UiThreadToken,
    /// One `EventBus` per window - shared by every segment installed through
    /// this `Router`, so actors in different features of the same window can
    /// reach each other. See `FeatureInitContext::event_bus`.
    event_bus: Rc<guinea_core::actor::event_bus::EventBus>,
    debug_registry: Rc<guinea_core::actor::registry::DebugRegistry>,
    state_cache: RefCell<StateCache>,
    /// Services provided by application-level plugins, handed to every feature
    /// this router installs. Empty when there is no installed application.
    services: guinea_core::SharedState,
}

impl<U: Ui> Router<U> {
    pub fn new(token: guinea_core::actor::UiThreadToken) -> Self {
        Self {
            active: RefCell::new(None),
            prev_route: RefCell::new(None),
            token,
            event_bus: Rc::new(guinea_core::actor::event_bus::EventBus::new()),
            debug_registry: Rc::new(guinea_core::actor::registry::DebugRegistry::new()),
            state_cache: RefCell::new(StateCache::new()),
            services: guinea_app::app::app_services(),
        }
    }
    
    pub fn activate<P: Page<U>>(&self, uri: &AppUri, mount: fn(SegmentProps<U>) -> U::View) -> anyhow::Result<Rc<Scope>> {
        *self.prev_route.borrow_mut() = None;
        self.install_chain(single_entry_chain::<U, P>(mount), uri)
    }

    pub fn navigate<R>(&self, route: R, uri: &AppUri) -> anyhow::Result<Rc<Scope>>
    where
        R: RouteChain<U> + PartialEq + Clone + 'static,
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

    fn install_chain(&self, chain: &'static [SegmentEntry<U>], uri: &AppUri) -> anyhow::Result<Rc<Scope>> {
        self.install_from(chain, 0, uri)
    }
    
    fn install_from(
        &self,
        chain: &'static [SegmentEntry<U>],
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
                services: self.services.clone(),
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
    
    pub fn render(&self) -> U::View {
        self.active
            .borrow()
            .as_ref()
            .expect("Router::render called with no active chain - call activate/navigate first")
            .root_view()
    }
}
