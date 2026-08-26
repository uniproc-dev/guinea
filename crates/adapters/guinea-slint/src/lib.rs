//! guinea on Slint: the same router and features, in a tree Slint compiled.
//!
//! The other two backends hand the router a view: WinUI an element tree,
//! ratatui a piece of deferred drawing. Slint hands it nothing. Its tree is
//! declared in `.slint` and compiled, and a component written in Rust cannot
//! be embedded into another one at runtime - `ComponentContainer` exists, but
//! the code generator answers `todo!("Components written in Rust can not get
//! embedded yet")`, so a factory's product ends up in the window without ever
//! being joined to the tree above it. It looks like it works until a second
//! page is mounted and the item indices start to overlap.
//!
//! So here a segment renders nothing: `View = ()`. The whole tree exists from
//! the start, with the pages in it, and the route decides which branch is
//! alive - a `.slint` conditional, which Slint creates and destroys on its
//! own. What the router still does is everything it does elsewhere: install
//! the features a route needs, own their scopes, and tear them down on the way
//! out.
//!
//! State reaches the tree through globals, one per page, declared beside the
//! component that reads them. A global is a singleton, so a binding made when
//! the page was installed survives the branch being destroyed and rebuilt -
//! which is exactly what happens every time the route changes.

mod convert;
mod dispatcher;
mod model;
mod nav;
mod root;
mod run;
mod windows;

pub use convert::ToSlint;
pub use run::{MAIN, run};

use std::rc::Rc;

use guinea_app::feature::FeatureInitContext;
use guinea_core::binding::ReducerBinding;
use guinea_core::scope::{DropGuard, Reducer};
use guinea_core::uri::AppUri;
use guinea_router::router::{
    NavigateHandle, RouteChain, SegmentEntry, SegmentProps, ToUri, Ui, single_entry_chain,
};

/// Installing a root without [`run`], for tests and for an application that
/// drives Slint's loop itself.
pub mod testing {
    /// Parks a root the way [`crate::run`] does, so that pages can be
    /// installed and bound with no window loop running.
    pub fn set_root<W: slint::ComponentHandle + 'static>(root: W) {
        crate::root::install(root)
    }
}

/// Slint as a [`Ui`].
pub struct Slint;

impl Ui for Slint {
    type View = ();
}

/// A leaf of the route tree.
///
/// There is no `view`, and the name would lie if there were: the view is the
/// `.slint` file. What Rust adds is the wiring.
pub trait Page: 'static {
    /// When `true`, the router keeps this page's reducer states in memory
    /// while the page is not mounted.
    const CACHE_STATE_IN_MEMORY: bool = false;

    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    /// Ties this page's globals to state and actions.
    ///
    /// Runs when the router installs the page, which is before anything is
    /// drawn, and again on every later install. The globals outlive the
    /// branch that reads them, so nothing here has to be redone when Slint
    /// rebuilds it.
    fn bind(cx: PageCx);
}

/// A branch: the component the pages below it sit inside.
pub trait Layout: 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn bind(cx: LayoutCx);
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<Slint> {
    SegmentEntry::new(
        std::any::TypeId::of::<P>,
        install_page::<P>,
        nothing_to_render,
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<Slint> {
    SegmentEntry::new(
        std::any::TypeId::of::<L>,
        install_layout::<L>,
        nothing_to_render,
        false,
    )
}

/// Wiring happens when a segment is installed, not when it is mounted.
///
/// Mounting is per-render, and the router re-mounts a whole chain every time.
/// Everywhere else that is what drawing means; here it would re-run the
/// bindings - handing a global a second listener, and re-setting a callback
/// from inside the very callback that navigated, which Slint refuses with
/// "Callback Handler set while called". Installing happens once per segment
/// per route, which is exactly the life the bindings should have: the scope
/// they hang on is created and dropped with it.
fn install_page<P: Page>(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
    P::install(ctx, uri)?;
    P::bind(PageCx { at: Where::of(ctx) });
    Ok(())
}

fn install_layout<L: Layout>(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
    L::install(ctx, uri)?;
    L::bind(LayoutCx { at: Where::of(ctx) });
    Ok(())
}

fn nothing_to_render(_: SegmentProps<Slint>) {}

/// A one-segment chain, for a page shown without a route tree.
pub fn page_chain<P: Page>() -> &'static [SegmentEntry<Slint>] {
    single_entry_chain(segment_entry::<P>())
}

/// The scope a segment was installed in, and the ones above it.
///
/// The same lookup `SegmentProps` does, from what `install` is given: a
/// reducer belongs to the nearest scope whose `install` claimed it.
#[derive(Clone)]
struct Where {
    scope: Rc<guinea_core::scope::Scope>,
    ancestors: Rc<[Rc<guinea_core::scope::Scope>]>,
}

impl Where {
    fn of(ctx: &FeatureInitContext) -> Self {
        Self {
            scope: ctx.scope.clone(),
            ancestors: ctx.ancestors.clone(),
        }
    }

    fn binding<R: Reducer>(&self) -> ReducerBinding<R> {
        std::iter::once(&self.scope)
            .chain(self.ancestors.iter().rev())
            .find(|scope| scope.has_feature::<R>())
            .unwrap_or_else(|| {
                panic!(
                    "bind::<{}>() found no scope - this one or an ancestor - whose install() \
                     claimed it with ctx.port/ctx.actions/ctx.seed_reducer",
                    std::any::type_name::<R>()
                )
            })
            .binding::<R>()
    }

    fn own<T: 'static>(&self, resource: T) {
        self.scope.own(DropGuard(resource))
    }
}

/// What a page's wiring is handed.
#[derive(Clone)]
pub struct PageCx {
    at: Where,
}

impl PageCx {
    /// The application's root component, for the globals hanging off it.
    ///
    /// ```ignore
    /// let model = cx.root::<AppWindow>().global::<ProcessesModel>();
    /// ```
    ///
    /// Panics if called before [`run`] has a window, or with a different type
    /// than the one it was given.
    pub fn root<W: slint::ComponentHandle + 'static>(&self) -> W {
        root::current::<W>()
    }

    /// The reducer's binding: state now, and a place to subscribe.
    pub fn binding<R: Reducer>(&self) -> ReducerBinding<R> {
        self.at.binding::<R>()
    }

    /// A snapshot of the reducer's state.
    pub fn read<R>(&self) -> R::State
    where
        R: Reducer,
        R::State: Clone,
    {
        self.binding::<R>().get()
    }

    pub fn actions<R: Reducer>(&self) -> Rc<R::Actions> {
        self.binding::<R>().actions()
    }

    /// Applies the state now, and again after every change, for as long as
    /// this page's scope lives.
    pub fn bind<R: Reducer>(&self, apply: impl Fn(&R::State) + 'static) {
        bind_to_scope(&self.binding::<R>(), apply)
    }

    /// [`Self::bind`], with the root handed back on every call.
    ///
    /// Which is what a binding needs, since the globals it writes to hang off
    /// the root and borrow it - so nothing can capture one. Holding the root
    /// instead is safe: it is a handle, and the window outlives every page.
    pub fn bind_to<R, W>(&self, root: &W, apply: impl Fn(&W, &R::State) + 'static)
    where
        R: Reducer,
        W: slint::ComponentHandle + 'static,
    {
        let root = root.clone_strong();
        self.bind::<R>(move |state| apply(&root, state))
    }

    /// A list property that reads the state instead of copying it.
    ///
    /// Set once, not on every change: the model answers from the reducer's
    /// own state and converts a row when Slint asks for that row - so a list
    /// of ten thousand costs a screenful, and adding one row costs one.
    ///
    /// ```ignore
    /// model.set_items(cx.rows::<ProcessesReducer, _>(|state| &state.items));
    /// ```
    pub fn rows<R, T>(&self, select: fn(&R::State) -> &[T]) -> slint::ModelRc<T::Slint>
    where
        R: Reducer,
        T: ToSlint + 'static,
        T::Slint: Clone + 'static,
    {
        model::Rows::new(self.binding::<R>(), select)
    }

    /// A navigator over the route type the application runs.
    pub fn navigate<R>(&self) -> NavigateHandle<Slint, R>
    where
        R: RouteChain<Slint> + ToUri + Clone + PartialEq + 'static,
    {
        nav::current::<R>()
    }

    /// Drops `resource` when the router drops this page.
    pub fn own<T: 'static>(&self, resource: T) {
        self.at.own(resource)
    }
}

/// What a layout's wiring is handed. The same as a page's: with the tree
/// declared up front, a layout has nothing to place.
#[derive(Clone)]
pub struct LayoutCx {
    at: Where,
}

impl LayoutCx {
    pub fn root<W: slint::ComponentHandle + 'static>(&self) -> W {
        root::current::<W>()
    }

    pub fn binding<R: Reducer>(&self) -> ReducerBinding<R> {
        self.at.binding::<R>()
    }

    pub fn read<R>(&self) -> R::State
    where
        R: Reducer,
        R::State: Clone,
    {
        self.binding::<R>().get()
    }

    pub fn actions<R: Reducer>(&self) -> Rc<R::Actions> {
        self.binding::<R>().actions()
    }

    pub fn bind<R: Reducer>(&self, apply: impl Fn(&R::State) + 'static) {
        bind_to_scope(&self.binding::<R>(), apply)
    }

    /// [`Self::bind`], with the root handed back on every call.
    pub fn bind_to<R, W>(&self, root: &W, apply: impl Fn(&W, &R::State) + 'static)
    where
        R: Reducer,
        W: slint::ComponentHandle + 'static,
    {
        let root = root.clone_strong();
        self.bind::<R>(move |state| apply(&root, state))
    }

    /// A list property that reads the state instead of copying it.
    pub fn rows<R, T>(&self, select: fn(&R::State) -> &[T]) -> slint::ModelRc<T::Slint>
    where
        R: Reducer,
        T: ToSlint + 'static,
        T::Slint: Clone + 'static,
    {
        model::Rows::new(self.binding::<R>(), select)
    }

    pub fn navigate<R>(&self) -> NavigateHandle<Slint, R>
    where
        R: RouteChain<Slint> + ToUri + Clone + PartialEq + 'static,
    {
        nav::current::<R>()
    }

    pub fn own<T: 'static>(&self, resource: T) {
        self.at.own(resource)
    }
}

fn bind_to_scope<R: Reducer>(binding: &ReducerBinding<R>, apply: impl Fn(&R::State) + 'static) {
    apply(&binding.peek());
    binding.on_change_owned(apply);
}
