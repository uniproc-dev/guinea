use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use guinea_core::binding::ReducerBinding;
use guinea_core::scope::{Reducer, Scope};

use guinea_app::feature::{FeatureHost, FeatureInitContext};
use guinea_core::uri::AppUri;

/// A UI backend, described entirely by the types it works in.
///
/// Deliberately method-free: anything that looks like behaviour is carried as
/// data instead - mounting is a function pointer in [`SegmentEntry`],
/// navigation is a [`RouteSink`], identity is a [`SegmentIdentity`]. A trait
/// with methods here would have to name a common denominator of every
/// toolkit, and each backend would then be forced to fake the parts that do
/// not fit its model.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a guinea backend",
    note = "with more than one backend feature enabled there is no default one, so a route tree has to name it: `routes! {{ backend = guinea::winui::WinUi, .. }}` or `backend = guinea::ratatui::Tui`"
)]
pub trait Ui: Sized + 'static {
    /// What a view produces: an element tree for a reconciler, a deferred draw
    /// closure for immediate mode, `()` for a backend that pushes into
    /// retained properties.
    type View: 'static;
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
        // A segment is its own identity *and* everything it renders below,
        // because that is what it draws: a layout emits its child through
        // `outlet()`. Comparing only this segment would let a reconciler skip
        // a layout whose own identity is unchanged - and skipping the layout
        // skips the `outlet()` call, so navigating between two pages under a
        // shared layout would leave the old page on screen.
        //
        // Note what is deliberately not compared. Not
        // `std::ptr::eq(self.chain, other.chain)`: sibling leaves under the
        // same layout each get their own `const` chain array (see
        // `routes_dsl.rs`), so the arrays always differ even when the segment
        // is unchanged. Not `Rc::ptr_eq(&self.scopes, &other.scopes)` either:
        // `install_from` wraps the whole stack in a fresh `Rc` on every
        // navigation, even when the individual scopes are reused.
        self.cursor == other.cursor && self.tail_identity().eq(other.tail_identity())
    }
}

impl<U: Ui> SegmentProps<U> {
    /// This segment's identity, then every segment it renders inside itself.
    pub fn tail_identity(&self) -> impl Iterator<Item = SegmentIdentity> + '_ {
        let chain = self.chain;
        let scopes = &self.scopes;
        (self.cursor..chain.len().min(scopes.len())).map(move |cursor| SegmentIdentity {
            cursor,
            segment: (chain[cursor].type_id)(),
            scope: Rc::as_ptr(&scopes[cursor]) as usize,
        })
    }

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

    /// Mounts the next segment down the chain. Backends expose this to layouts
    /// only: a page is the end of the chain and has no child to render.
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

impl<U: Ui> SegmentEntry<U> {
    /// Built by the backend, which is where "page" and "layout" are defined -
    /// the router only needs something it can install and mount.
    pub const fn new(
        type_id: fn() -> TypeId,
        install: fn(&FeatureInitContext, &AppUri) -> anyhow::Result<()>,
        mount: fn(SegmentProps<U>) -> U::View,
        cache_state: bool,
    ) -> Self {
        Self {
            type_id,
            install,
            mount,
            cache_state,
        }
    }
}

/// A one-segment chain for `P`, leaked once per page type.
///
/// Leaked rather than owned because `SegmentEntry` chains are `&'static` -
/// `routes!` generates them as `const` arrays, and this is the path for a page
/// activated without a route tree (tests, and `Router::activate`).
pub fn single_entry_chain<U: Ui>(entry: SegmentEntry<U>) -> &'static [SegmentEntry<U>] {
    Box::leak(Box::new([entry])) as &'static [SegmentEntry<U>]
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
/// Keeps a route-change hook alive. Dropping it removes the hook.
pub struct RouteHookHandle {
    id: usize,
    router: std::rc::Weak<dyn AnyRouter>,
}

impl Drop for RouteHookHandle {
    fn drop(&mut self) {
        if let Some(router) = self.router.upgrade() {
            router.remove_route_hook(self.id);
        }
    }
}

/// The part of `Router` a hook handle needs, without its backend parameter -
/// a handle should not have to name `U` just to unregister itself.
pub trait AnyRouter {
    fn remove_route_hook(&self, id: usize);
}

impl<U: Ui> AnyRouter for Router<U> {
    fn remove_route_hook(&self, id: usize) {
        self.route_hooks.borrow_mut().retain(|(this, _)| *this != id);
    }
}

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
        if let Some(current) = self.current() {
            self.router.remember(Box::new(current.0), current.1);
        }
        self.go(route);
    }

    /// Back one step, if there is one. Reports whether it moved, so a key
    /// handler can fall through to something else - closing a dialog, or
    /// quitting - when there is nowhere to go.
    pub fn back(&self) -> bool {
        let leaving = self
            .current()
            .map(|(route, uri)| (Box::new(route) as Box<dyn Any>, uri));
        let Some(entry) = self.router.take_back(leaving) else {
            return false;
        };
        self.arrive(entry)
    }

    /// Forward one step, undoing a [`back`](Self::back).
    pub fn forward(&self) -> bool {
        let leaving = self
            .current()
            .map(|(route, uri)| (Box::new(route) as Box<dyn Any>, uri));
        let Some(entry) = self.router.take_forward(leaving) else {
            return false;
        };
        self.arrive(entry)
    }

    pub fn can_go_back(&self) -> bool {
        self.router.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.router.can_go_forward()
    }

    /// Where the router is now, as this handle's route type.
    fn current(&self) -> Option<(R, AppUri)> {
        let route = self.router.current_route::<R>()?;
        let uri = route.to_uri();
        Some((route, uri))
    }

    fn arrive(&self, entry: Visited) -> bool {
        let uri = entry.uri().clone();
        let Some(route) = entry.route::<R>() else {
            // Two route types on one router. Nothing in guinea builds that,
            // and silently doing nothing beats navigating somewhere wrong.
            tracing::warn!("history holds a route of another type; ignoring it");
            return false;
        };
        self.go_at(route, uri);
        true
    }

    fn go(&self, route: R) {
        let uri = route.to_uri();
        self.go_at(route, uri);
    }

    fn go_at(&self, route: R, uri: AppUri) {
        self.router.navigate(route.clone(), &uri).expect("navigate");
        self.router.route_changed(&uri.to_string());
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

/// A place the application has been: the route itself, erased, and the path
/// it produced.
pub struct Visited {
    route: Box<dyn Any>,
    uri: AppUri,
}

impl Visited {
    /// The route, if it is the type asked for. Anything else means two route
    /// types shared one router, which nothing in guinea sets up.
    pub fn route<R: 'static>(self) -> Option<R> {
        self.route.downcast::<R>().ok().map(|route| *route)
    }

    pub fn uri(&self) -> &AppUri {
        &self.uri
    }
}

pub struct Router<U: Ui> {
    pub(crate) active: RefCell<Option<ActiveChain<U>>>,
    prev_route: RefCell<Option<Box<dyn Any>>>,
    /// Where the application has been, and where it was pulled back from.
    ///
    /// Kept here rather than in the application because the browser is the
    /// only shell that supplies a history: WinUI and a terminal have none, and
    /// every one of them would otherwise grow the same stack next to the
    /// router. Routes are held erased - the router never learns `R` - and
    /// [`NavigateHandle`] downcasts them back on the way out.
    back: RefCell<Vec<Visited>>,
    forward: RefCell<Vec<Visited>>,
    /// Installing a feature is not a routing concern - the router asks the
    /// host for a context and does the part that is its own: which scopes to
    /// keep, which to tear down, and in what order.
    host: FeatureHost,
    state_cache: RefCell<StateCache>,
    /// Notified after a navigation is applied. Kept here rather than on the
    /// application, because a route change is something only a router has -
    /// an application without one has nothing to report.
    route_hooks: RefCell<Vec<(usize, Rc<dyn Fn(Option<&str>, &str)>)>>,
    next_hook_id: std::cell::Cell<usize>,
    last_route: RefCell<Option<String>>,
}

impl<U: Ui> Router<U> {
    pub fn new(token: guinea_core::actor::UiThreadToken) -> Self {
        Self::with_host(FeatureHost::new(token))
    }

    /// For a caller that already has a host - one window hosting more than a
    /// single router, say, where features must share an event bus.
    pub fn with_host(host: FeatureHost) -> Self {
        Self {
            active: RefCell::new(None),
            prev_route: RefCell::new(None),
            back: RefCell::new(Vec::new()),
            forward: RefCell::new(Vec::new()),
            host,
            state_cache: RefCell::new(StateCache::new()),
            route_hooks: RefCell::new(Vec::new()),
            next_hook_id: std::cell::Cell::new(0),
            last_route: RefCell::new(None),
        }
    }

    /// Runs `hook` after each navigation, with the previous path (`None` for
    /// the first) and the new one.
    ///
    /// The hook lasts as long as the returned handle. A caller that wants it
    /// for the life of the router can `std::mem::forget` it, but the usual
    /// caller is a view that must stop listening when it unmounts.
    #[must_use = "the hook is removed when the handle is dropped"]
    pub fn on_route_change(
        self: &Rc<Self>,
        hook: impl Fn(Option<&str>, &str) + 'static,
    ) -> RouteHookHandle {
        let id = self.next_hook_id.get();
        self.next_hook_id.set(id + 1);
        self.route_hooks.borrow_mut().push((id, Rc::new(hook)));
        RouteHookHandle {
            id,
            router: Rc::downgrade(&(self.clone() as Rc<dyn AnyRouter>)),
        }
    }

    /// Announces a navigation to the hooks. Called by `NavigateHandle::to`
    /// once the router has accepted the route.
    pub fn route_changed(&self, to: &str) {
        let from = self.last_route.borrow().clone();
        // Cloned out before calling: a hook is free to navigate, and would
        // otherwise re-enter this borrow.
        let hooks: Vec<_> = self
            .route_hooks
            .borrow()
            .iter()
            .map(|(_, hook)| hook.clone())
            .collect();
        for hook in hooks {
            hook(from.as_deref(), to);
        }
        *self.last_route.borrow_mut() = Some(to.to_string());
    }

    pub fn host(&self) -> &FeatureHost {
        &self.host
    }
    
    /// Installs a chain directly, outside any route tree - one segment, as a
    /// rule. The backend builds the chain, since only it knows how a segment
    /// is mounted.
    pub fn activate(
        &self,
        uri: &AppUri,
        chain: &'static [SegmentEntry<U>],
    ) -> anyhow::Result<Rc<Scope>> {
        *self.prev_route.borrow_mut() = None;
        self.install_chain(chain, uri)
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

            // The ancestors snapshot is everything built so far this loop -
            // root to this segment's immediate parent, never including
            // `scope` itself. `inherit()` walks it to find an ancestor that
            // already `install()`-ed the feature being asked for.
            let ctx = self
                .host
                .context(scope.clone(), Rc::from(scopes.clone()));
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
    
    /// The scope of the segment at `cursor` in the active chain: 0 is the
    /// outermost layout, the last one the leaf.
    pub fn scope_at(&self, cursor: usize) -> Option<Rc<Scope>> {
        self.active
            .borrow()
            .as_ref()
            .and_then(|active| active.scopes.get(cursor).cloned())
    }

    /// The route the router is on, if it is the type asked for.
    ///
    /// The router keeps it erased - it never learns `R` - so this is the way
    /// back to the typed route, and it is `None` before the first navigation.
    pub fn current_route<R: Clone + 'static>(&self) -> Option<R> {
        self.prev_route
            .borrow()
            .as_ref()
            .and_then(|route| route.downcast_ref::<R>())
            .cloned()
    }

    /// Records where the application is leaving from. Called by
    /// [`NavigateHandle::to`]; a forward stack only survives until the next
    /// ordinary navigation, exactly as a browser's does.
    pub fn remember(&self, route: Box<dyn Any>, uri: AppUri) {
        self.back.borrow_mut().push(Visited { route, uri });
        self.forward.borrow_mut().clear();
    }

    /// Takes the previous entry, putting `leaving` on the forward stack.
    pub fn take_back(&self, leaving: Option<(Box<dyn Any>, AppUri)>) -> Option<Visited> {
        let entry = self.back.borrow_mut().pop()?;
        if let Some((route, uri)) = leaving {
            self.forward.borrow_mut().push(Visited { route, uri });
        }
        Some(entry)
    }

    /// Takes the next entry, putting `leaving` back on the history.
    pub fn take_forward(&self, leaving: Option<(Box<dyn Any>, AppUri)>) -> Option<Visited> {
        let entry = self.forward.borrow_mut().pop()?;
        if let Some((route, uri)) = leaving {
            self.back.borrow_mut().push(Visited { route, uri });
        }
        Some(entry)
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.borrow().is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.borrow().is_empty()
    }

    /// The chain the active route installed, and the scopes it installed it
    /// into - what a backend needs to build props for a segment.
    pub fn active_chain(&self) -> Option<&'static [SegmentEntry<U>]> {
        self.active.borrow().as_ref().map(|active| active.entries)
    }

    pub fn active_scopes(&self) -> Option<Rc<Vec<Rc<Scope>>>> {
        self.active
            .borrow()
            .as_ref()
            .map(|active| active.scopes.clone())
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
