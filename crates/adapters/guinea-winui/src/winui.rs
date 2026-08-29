//! The windows-reactor backend: what a view is, how a segment is mounted, and
//! the hooks a view reads state through.

use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::scope::Reducer;

use guinea_app::feature::{FeatureInitContext, Reaches, Segment};
use guinea_router::router::{
    Mount, NavigateHandle, RouteChain, RouteSink, Router, SegmentEntry, SegmentProps, Ui,
    single_entry_chain,
};

/// windows-reactor as a [`Ui`].
pub struct WinUi;

impl Ui for WinUi {
    type View<'a> = windows_reactor::Element;
    /// Nothing: the reconciler owns the tree, and an element it is handed owns
    /// everything it shows.
    type Nodes = ();
}

/// A leaf of the route tree.
///
/// Declared by the backend, not the router: what a view is handed and what it
/// returns differ per toolkit, and there is nothing useful left once you take
/// the difference out.
pub trait Page: Sized + 'static {
    /// When `true`, the router keeps this page's reducer states in memory
    /// while the page is not mounted. The page's scope (and therefore its
    /// actors) is still torn down, but when the user returns the UI will see
    /// the last cached state immediately instead of starting from defaults.
    const CACHE_STATE_IN_MEMORY: bool = false;

    /// What this page captured from the route, named by `routes!`. `()` for a
    /// page that captures nothing.
    ///
    /// `PartialEq` because the router's one question about a capture is
    /// whether it is still the same one - which decides what reinstalls and
    /// which cached state may come back.
    type Params: PartialEq + 'static;

    /// What this segment installs, and `()` when it installs nothing.
    ///
    /// The list is not written beside the body - it *is* the body's
    /// obligation: `install` returns it, so a feature that stops being
    /// installed stops type-checking. Which is also why `install` has no
    /// default any more.
    ///
    /// What is returned is owned by the segment's scope, which is what gives a
    /// feature its own lifetime.
    type Installs: 'static;

    fn install(ctx: &FeatureInitContext, params: &Self::Params) -> anyhow::Result<Self::Installs>;

    fn view(cx: &mut PageCx<Self>) -> windows_reactor::Element;
}

/// A branch of the route tree: renders its own chrome and its child through
/// [`LayoutCx::outlet`].
pub trait Layout: Sized + 'static {
    /// What every page under this layout carries, derived by `routes!` as the
    /// intersection of their parameters. A layout declares nothing; it is
    /// handed what all of its children were reached with.
    type Params: PartialEq + 'static;

    /// What this segment installs, and `()` when it installs nothing.
    ///
    /// The list is not written beside the body - it *is* the body's
    /// obligation: `install` returns it, so a feature that stops being
    /// installed stops type-checking. Which is also why `install` has no
    /// default any more.
    ///
    /// What is returned is owned by the segment's scope, which is what gives a
    /// feature its own lifetime.
    type Installs: 'static;

    fn install(ctx: &FeatureInitContext, params: &Self::Params) -> anyhow::Result<Self::Installs>;

    fn view(cx: &mut LayoutCx<Self>) -> windows_reactor::Element;
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<WinUi> {
    SegmentEntry::new(
        TypeId::of::<P>,
        install_page::<P>,
        guinea_router::router::same_params::<P::Params>,
        &const { MountPage::<P>(std::marker::PhantomData) },
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<WinUi> {
    SegmentEntry::new(
        TypeId::of::<L>,
        install_layout::<L>,
        guinea_router::router::same_params::<L::Params>,
        &const { MountLayout::<L>(std::marker::PhantomData) },
        false,
    )
}

fn install_page<P: Page>(
    ctx: &FeatureInitContext,
    params: &dyn std::any::Any,
) -> anyhow::Result<()> {
    own(ctx, P::install(ctx, guinea_router::router::narrow::<P::Params, P>(params)?)?);
    Ok(())
}

/// Hands what a segment installed to its scope - a feature's lifetime is the
/// segment's, and dropping this here would end it at the end of `install`.
fn own<T: 'static>(ctx: &FeatureInitContext, installed: T) {
    ctx.scope.own(guinea_core::scope::DropGuard(installed));
}

fn install_layout<L: Layout>(
    ctx: &FeatureInitContext,
    params: &dyn std::any::Any,
) -> anyhow::Result<()> {
    own(ctx, L::install(ctx, guinea_router::router::narrow::<L::Params, L>(params)?)?);
    Ok(())
}

/// A zero-sized marker per segment type: what a `const` entry points at to get
/// its `&'static dyn Mount`.
pub struct MountPage<P>(pub std::marker::PhantomData<P>);
pub struct MountLayout<L>(pub std::marker::PhantomData<L>);

impl<P: Page> Mount<WinUi> for MountPage<P> {
    fn view<'a>(
        &self,
        props: SegmentProps<WinUi>,
        _nodes: &'a (),
    ) -> windows_reactor::Element {
        windows_reactor::component(render_page::<P>, props)
    }
}

impl<L: Layout> Mount<WinUi> for MountLayout<L> {
    fn view<'a>(
        &self,
        props: SegmentProps<WinUi>,
        _nodes: &'a (),
    ) -> windows_reactor::Element {
        windows_reactor::component(render_layout::<L>, props)
    }
}

/// Renders a page without a reconciler - for tests that exercise the hooks
/// directly. `mount_page` is the real entry point.
#[doc(hidden)]
pub fn render_page<P: Page>(
    props: &SegmentProps<WinUi>,
    cx: &mut windows_reactor::RenderCx,
) -> windows_reactor::Element {
    P::view(&mut PageCx {
        props: props.clone(),
        cx,
        page: std::marker::PhantomData,
    })
}

#[doc(hidden)]
pub fn render_layout<L: Layout>(
    props: &SegmentProps<WinUi>,
    cx: &mut windows_reactor::RenderCx,
) -> windows_reactor::Element {
    L::view(&mut LayoutCx {
        props: props.clone(),
        cx,
        layout: std::marker::PhantomData,
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

/// A router in a context slot. Compared by identity - a `Router` has no
/// meaningful equality, and two handles mean the same router or a different
/// one.
#[derive(Clone)]
pub struct RouterHandle(Rc<Router<WinUi>>);

impl PartialEq for RouterHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The router itself, for hooks that configure it. One per backend rather
/// than per route type: `Router<WinUi>` does not mention `R`.
fn router_context() -> windows_reactor::Context<Option<RouterHandle>> {
    thread_local! {
        static ID: windows_reactor::ContextId = windows_reactor::ContextId::new();
    }
    windows_reactor::Context {
        default: None,
        id: ID.with(|id| *id),
    }
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

/// The router bound to this call site: one per component instance, not one
/// per process.
pub fn scoped_router(
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
    R: RouteChain<WinUi> + Clone + PartialEq + 'static,
{
    pub fn render(cx: &mut windows_reactor::RenderCx, initial: R) -> windows_reactor::Element {
        use windows_reactor::ProvideExt;

        // Genuinely on the UI thread here - `root()` render callbacks always
        // are - which is what the token attests to.
        let token = guinea_core::actor::UiThreadToken::dangerously_create_token_unchecked();

        let (route, set_route) = cx.use_state(initial);
        let router = scoped_router(cx, token);
        router.navigate(route.clone()).expect("navigate");

        let nav = NavigateHandle::new(
            router.clone(),
            RouteSink::new(move |route| set_route.call(route)),
        );
        router
            .render(&())
            .provide(&nav_context::<R>(), Some(nav))
            .provide(&route_context::<R>(), Some(route))
            .provide(&router_context(), Some(RouterHandle(router.clone())))
    }
}

/// The router as a root component, for [`crate::app::App::run`].
///
/// An application whose UI is not route-based passes its own component
/// instead - `run` does not know about routing.
/// Subscribing to route changes from a view, the way any other effect is
/// written: registered once, undone when the view unmounts.
pub trait UseRouteChange {
    /// Runs `hook` after each navigation, for as long as this view is
    /// mounted. Panics if there is no router above - which is a wiring
    /// mistake, not a state a running application can reach.
    fn use_route_change(&mut self, hook: impl Fn(Option<&str>, &str) + 'static);
}

impl UseRouteChange for windows_reactor::RenderCx {
    fn use_route_change(&mut self, hook: impl Fn(Option<&str>, &str) + 'static) {
        let router = self.use_context(&router_context()).unwrap_or_else(|| {
            panic!(
                "use_route_change() found no router above this view - it has to be                  called from inside the tree a RouterRx renders"
            )
        });

        self.use_effect_with_cleanup((), move || {
            let handle = router.0.on_route_change(hook);
            Some(move || drop(handle))
        });
    }
}

pub trait UseNavigate {
    fn use_navigate<R>(&self) -> NavigateHandle<WinUi, R>
    where
        R: RouteChain<WinUi> + Clone + PartialEq + 'static;
}

impl UseNavigate for windows_reactor::RenderCx {
    fn use_navigate<R>(&self) -> NavigateHandle<WinUi, R>
    where
        R: RouteChain<WinUi> + Clone + PartialEq + 'static,
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
) -> (R, guinea_core::feature::Dispatch)
where
    R: Reducer + Clone + PartialEq,
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

    (current, binding.dispatch())
}

/// What a page's render is handed.
///
/// Carries the page type, not because rendering needs it, but because reading
/// does: what a segment may read is a fact about where it sits, and this is
/// where that fact enters the signature.
pub struct PageCx<'a, P> {
    props: SegmentProps<WinUi>,
    cx: &'a mut windows_reactor::RenderCx,
    page: std::marker::PhantomData<fn() -> P>,
}

impl<P: Segment> PageCx<'_, P> {
    /// Reads a reducer's state and re-renders this component when it changes.
    ///
    /// Which feature answers is settled at build time: this page installed it,
    /// or a segment above listed it in `Exports`. The `_` is [`Reaches`]'s
    /// index, which says which of several impls applied - Rust has no partial
    /// turbofish, so it has to be written.
    pub fn use_reducer<R, I>(&mut self) -> (R, guinea_core::feature::Dispatch)
    where
        R: Reducer + Clone + PartialEq,
        P: Reaches<R, I>,
    {
        use_reducer::<R>(&self.props, self.cx)
    }
}

impl<P> std::ops::Deref for PageCx<'_, P> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

impl<P> std::ops::DerefMut for PageCx<'_, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cx
    }
}

pub struct LayoutCx<'a, L> {
    props: SegmentProps<WinUi>,
    cx: &'a mut windows_reactor::RenderCx,
    layout: std::marker::PhantomData<fn() -> L>,
}

impl<L: Segment> LayoutCx<'_, L> {
    /// See [`PageCx::use_reducer`].
    pub fn use_reducer<R, I>(&mut self) -> (R, guinea_core::feature::Dispatch)
    where
        R: Reducer + Clone + PartialEq,
        L: Reaches<R, I>,
    {
        use_reducer::<R>(&self.props, self.cx)
    }
}

impl<L> LayoutCx<'_, L> {
    pub fn outlet(&mut self) -> windows_reactor::Element {
        self.props.outlet(&())
    }
}

impl<L> std::ops::Deref for LayoutCx<'_, L> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

impl<L> std::ops::DerefMut for LayoutCx<'_, L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cx
    }
}
