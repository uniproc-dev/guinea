//! The windows-reactor backend: what a view is, how a segment is mounted, and
//! the hooks a view reads state through.

use std::any::TypeId;
use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::scope::Reducer;

use guinea_app::feature::FeatureInitContext;
use guinea_router::router::{
    NavigateHandle, RouteChain, RouteSink, Router, SegmentEntry, SegmentProps, ToUri, Ui,
    single_entry_chain,
};
use guinea_core::uri::AppUri;

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
