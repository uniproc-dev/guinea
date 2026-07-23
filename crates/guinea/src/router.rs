//! The router: owns the `Store` for whichever route/context is active,
//! ties `Page`s to their declared `Features` at the type level, and (via
//! `#[derive(Routable)]`) maps a URI path to a typed route value instead of
//! a string. UI-agnostic ambitions were dropped deliberately (see the
//! design discussion) - `Page::view` returns a real `windows_reactor::Element`
//! directly, not something behind a hook-crate seam.
//!
//! This first pass is intentionally flat: one active `Store` at a time, no
//! nested segments/outlet, no history stack, no dynamic route registration.
//! Those layer on top of this same `Store`-ownership mechanism without
//! changing it - what's here is the part that needed proving first: that a
//! `Page`'s declared `Features` are the only things it can `use_feature`,
//! checked at compile time, and that activating/deactivating a route
//! actually mounts/tears down real `Store` state.

use std::cell::RefCell;
use std::rc::Rc;

use guinea_core::store::{FeatureBindings, FeatureState, Store};

use crate::feature::{FeatureInitContext, RouteFeature};
use crate::uri::AppUri;

/// Per-position markers disambiguating which tuple slot satisfies
/// `Contains<F, _>` when checking whether `F` is one of a `Page`'s declared
/// `Features` - needed because, unlike a cons-list (frunk's `HList`), a
/// plain Rust tuple can't recurse structurally. Without a distinct `Idx` per
/// position, two impls that both fix a different tuple slot to `F` would be
/// flagged as overlapping by the coherence checker (both apply when a
/// `Page` happens to repeat the same feature type); the `Idx` type
/// parameter makes them distinct trait instantiations instead.
// Not `pub`: nothing outside this module ever needs to name these. A
// caller writes `cx.use_feature::<F, _>()` and inference finds whichever
// one applies - spelling out `Idx0` by hand was never actually required,
// it just hadn't been tried.
struct Idx0;
struct Idx1;
struct Idx2;
struct Idx3;

/// `F` is one of the types in `Self` (a tuple of feature marker types) -
/// what makes `use_feature::<F>()` a compile error for any `F` a `Page`
/// didn't declare in its `Features`, rather than a runtime "not mounted"
/// panic.
pub trait Contains<F, Idx> {}

impl<F> Contains<F, Idx0> for (F,) {}

impl<F, T2> Contains<F, Idx0> for (F, T2) {}
impl<T1, F> Contains<F, Idx1> for (T1, F) {}

impl<F, T2, T3> Contains<F, Idx0> for (F, T2, T3) {}
impl<T1, F, T3> Contains<F, Idx1> for (T1, F, T3) {}
impl<T1, T2, F> Contains<F, Idx2> for (T1, T2, F) {}

impl<F, T2, T3, T4> Contains<F, Idx0> for (F, T2, T3, T4) {}
impl<T1, F, T3, T4> Contains<F, Idx1> for (T1, F, T3, T4) {}
impl<T1, T2, F, T4> Contains<F, Idx2> for (T1, T2, F, T4) {}
impl<T1, T2, T3, F> Contains<F, Idx3> for (T1, T2, T3, F) {}

// Tried erasing `Idx` entirely (a blanket `impl<F, Features, Idx>
// InFeatures<Features> for F where Features: Contains<F, Idx> {}`) so
// `use_feature` would take a single type parameter. Doesn't compile:
// `Idx` appears only in the where-clause, not the impl header, which is
// E0207 ("unconstrained type parameter") - a hard Rust rule, not a bug in
// the attempt. A private wrapper method hiding the bound behind
// `use_feature<F>` doesn't work either (E0277): the bound has to be
// provable for arbitrary `F` at `use_feature`'s own signature, it can't be
// deferred to a call inside the body. `use_feature::<F, _>()` - `Idx`
// always `_`, never named by hand - is the floor Rust's type system
// actually allows here; frunk's own Selector-style APIs land on the same
// two-parameter shape for the same reason.

/// A tuple of feature marker types - what a `Page::Features` associated
/// type is. `install` runs every member's `RouteFeature::install` once, in
/// declaration order; the router calls this exactly once per route/context
/// activation.
pub trait FeatureSet {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()>;
}

impl FeatureSet for () {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<F1: Default + RouteFeature> FeatureSet for (F1,) {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        F1::default().install(ctx.clone(), uri)
    }
}

impl<F1: Default + RouteFeature, F2: Default + RouteFeature> FeatureSet for (F1, F2) {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        F1::default().install(ctx.clone(), uri)?;
        F2::default().install(ctx.clone(), uri)
    }
}

impl<F1: Default + RouteFeature, F2: Default + RouteFeature, F3: Default + RouteFeature> FeatureSet
    for (F1, F2, F3)
{
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        F1::default().install(ctx.clone(), uri)?;
        F2::default().install(ctx.clone(), uri)?;
        F3::default().install(ctx.clone(), uri)
    }
}

impl<
    F1: Default + RouteFeature,
    F2: Default + RouteFeature,
    F3: Default + RouteFeature,
    F4: Default + RouteFeature,
> FeatureSet for (F1, F2, F3, F4)
{
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        F1::default().install(ctx.clone(), uri)?;
        F2::default().install(ctx.clone(), uri)?;
        F3::default().install(ctx.clone(), uri)?;
        F4::default().install(ctx.clone(), uri)
    }
}

/// A page: what a route resolves to. `Features` is the compile-time-checked
/// declaration of what `view`/`pending`/`error` may `use_feature` - the
/// router installs exactly these, and only these, when the page's route
/// activates.
pub trait Page: Sized + 'static {
    type Features: FeatureSet;

    fn view(cx: &mut PageCx<Self>) -> windows_reactor::Element;

    /// `None` (the default) means: nothing to show for this page
    /// specifically: fall back to a parent segment's `pending`, then to
    /// guinea's own built-in default. See the design notes on nearest-
    /// boundary resolution (Next.js App Router-style) - not yet wired into
    /// this flat first pass (there's only one segment), but the trait shape
    /// is already the one nesting will use.
    fn pending(_cx: &mut windows_reactor::RenderCx) -> Option<windows_reactor::Element> {
        None
    }

    fn error(
        _cx: &mut windows_reactor::RenderCx,
        _err: &anyhow::Error,
    ) -> Option<windows_reactor::Element> {
        None
    }
}

/// What a `Page::view` (and `pending`/`error`, once those take one too)
/// renders with: a `RenderCx` plus the route's `Store`, `Contains`-checked
/// so `use_feature` only accepts a feature the page actually declared.
pub struct PageCx<'a, P: Page> {
    pub cx: &'a mut windows_reactor::RenderCx,
    store: Rc<Store>,
    _marker: std::marker::PhantomData<P>,
}

impl<'a, P: Page> PageCx<'a, P> {
    pub fn new(cx: &'a mut windows_reactor::RenderCx, store: Rc<Store>) -> Self {
        Self {
            cx,
            store,
            _marker: std::marker::PhantomData,
        }
    }

    /// The feature's current snapshot - resolved through the route's
    /// `Store`, same as the actor's own install code resolves it. Call as
    /// `use_feature::<F, _>()` - `Idx` is mechanical (always `_`, inference
    /// finds the one real impl), never something to work out by hand; see
    /// the note above `Contains`'s impls for why it can't be erased from
    /// the signature entirely. `F` not being one of `P`'s declared features
    /// is a compile error here, not a "feature not mounted" panic at
    /// runtime.
    ///
    /// Returns `StateHandle`, not the bare `Rc<RefCell<F::State>>>`
    /// `Store::state` itself holds - view code only ever gets `.borrow()`
    /// (a `Ref`, read-only), never `borrow_mut`. `reduce` is the only
    /// mutator, and it runs inside the cell, not here.
    ///
    /// Only needs `F: FeatureState` - reading a feature's state has nothing
    /// to do with whether it has bindings at all. Use `use_dispatch` (below)
    /// separately for features that do.
    pub fn use_feature<F, Idx>(&self) -> guinea_core::store::StateHandle<F::State>
    where
        F: FeatureState,
        P::Features: Contains<F, Idx>,
    {
        self.store.state::<F>().into()
    }

    /// The feature's dispatch handle - resolved the same way `use_feature`
    /// resolves state, but only for features that actually implement
    /// `FeatureBindings`. Kept as a separate method, not folded into
    /// `use_feature`'s return tuple: dispatch is an unrelated flow (view ->
    /// domain, not domain -> view), and a purely read-only feature has none
    /// to offer - forcing every `use_feature` call to also produce a
    /// bindings handle, real or not, would leak that flow into features
    /// that don't have it.
    pub fn use_dispatch<F, Idx>(&self) -> Rc<F::Bindings>
    where
        F: FeatureState + FeatureBindings,
        P::Features: Contains<F, Idx>,
    {
        self.store.bindings::<F>()
    }
}

impl<'a, P: Page> std::ops::Deref for PageCx<'a, P> {
    type Target = windows_reactor::RenderCx;
    fn deref(&self) -> &Self::Target {
        self.cx
    }
}

impl<'a, P: Page> std::ops::DerefMut for PageCx<'a, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cx
    }
}

/// Owns the currently active route's `Store`. Flat by design for this
/// pass: `activate` replaces whatever was active (dropping its `Store`,
/// cascading through every cell's actor/tasks - see `guinea_core::store`),
/// `deactivate` clears it without replacing. Nested segments, history, and
/// dynamic route registration build on top of this same activate/drop
/// mechanism later; none of them change what "activate" or "deactivate"
/// means for a single segment's `Store`.
pub struct Router {
    active: RefCell<Option<Rc<Store>>>,
    token: guinea_core::actor::UiThreadToken,
}

impl Router {
    pub fn new(token: guinea_core::actor::UiThreadToken) -> Self {
        Self {
            active: RefCell::new(None),
            token,
        }
    }

    /// Activates `P` for `uri`: a fresh `Store`, `P::Features` installed
    /// into it, then swapped in as the active route - dropping (and so
    /// tearing down) whatever was active before.
    pub fn activate<P: Page>(&self, uri: &AppUri) -> anyhow::Result<Rc<Store>> {
        let store = Rc::new(Store::new());
        let ctx = FeatureInitContext {
            store: store.clone(),
            token: self.token.clone(),
        };
        P::Features::install(&ctx, uri)?;
        *self.active.borrow_mut() = Some(store.clone());
        Ok(store)
    }

    /// Tears down the active route's `Store` without activating a new one.
    pub fn deactivate(&self) {
        *self.active.borrow_mut() = None;
    }

    pub fn active_store(&self) -> Option<Rc<Store>> {
        self.active.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::actor::UiThreadToken;
    use guinea_macros::{Routable, route_feature};

    // Deliberately trivial fixture: no Port/Bindings contracts, no actor,
    // no FeatureBindings at all - this feature is pure display, fed only by
    // a direct push, and has nothing to dispatch. Store<->Port/Bindings
    // bridging is already covered by guinea-core's own store.rs tests and
    // store_adapter_gen's snapshot test - what's actually new here (and what
    // this test proves) is the router: does activating a Page really
    // construct a fresh Store, run its Features' install exactly once, let
    // a Contains-checked use_feature read what install wrote, and tear the
    // Store down on deactivate. Re-simulating codegen ceremony on top of
    // that would test the same ground twice for no added confidence.

    #[derive(Default, Clone, PartialEq, Debug)]
    struct ProcessesViewState {
        seeded_from: String,
    }

    #[derive(Default)]
    struct ProcessesFeature;

    impl FeatureState for ProcessesFeature {
        type State = ProcessesViewState;
        type Push = String;

        fn reduce(state: &mut Self::State, msg: Self::Push) {
            state.seeded_from = msg;
        }
    }

    #[route_feature(ProcessesFeature)]
    fn install_processes(ctx: FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        // proves `uri` reaches the loader, and that install runs before the
        // first `view()` - nothing else.
        ctx.store
            .push::<ProcessesFeature>(uri.context_name.to_string());
        Ok(())
    }

    #[derive(Routable, Debug, PartialEq)]
    enum AppRoute {
        #[route("/:context/processes")]
        Processes { context: String },
    }

    struct ProcessesPage;

    impl Page for ProcessesPage {
        type Features = (ProcessesFeature,);

        fn view(cx: &mut PageCx<Self>) -> windows_reactor::Element {
            // Compile-time proof `Contains` works: this line simply wouldn't
            // compile if `ProcessesFeature` weren't in `Self::Features`.
            let state = cx.use_feature::<ProcessesFeature, _>();
            assert_eq!(state.borrow().seeded_from, "ubuntu");
            windows_reactor::Element::default()
        }
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
    fn activating_a_page_installs_its_features_and_view_can_use_them() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::new(token);
        let uri = AppUri::new("ubuntu", std::borrow::Cow::Borrowed("processes"), vec![]);

        let store = router.activate::<ProcessesPage>(&uri).expect("activate");
        assert!(router.active_store().is_some());

        // Loader already ran (synchronously, inside activate) and seeded state.
        let state = store.state::<ProcessesFeature>();
        assert_eq!(state.borrow().seeded_from, "ubuntu");

        let mut cx = windows_reactor::RenderCx::new(Rc::new(|| {}));
        let mut page_cx = PageCx::<ProcessesPage>::new(&mut cx, store.clone());
        ProcessesPage::view(&mut page_cx); // exercises use_feature for real

        router.deactivate();
        assert!(router.active_store().is_none());
    }
}

#[cfg(test)]
mod feature_macro_tests {
    use guinea_core::store::{FeatureBindings, FeatureState};
    use guinea_macros::feature_bindings;

    // Proves #[feature_bindings] fills in `type Bindings` correctly from
    // the marker's own name (`WidgetFeature` -> `WidgetBindings`) - the
    // whole point being that nothing has to spell out the codegen-generated
    // struct name itself. Written as a separate impl from FeatureState
    // (below) on purpose - a feature with no dispatch simply wouldn't write
    // this block at all.
    #[derive(Default)]
    struct WidgetBindings;

    struct WidgetFeature;

    impl FeatureState for WidgetFeature {
        type State = i32;
        type Push = i32;

        fn reduce(state: &mut i32, msg: i32) {
            *state = msg;
        }
    }

    #[feature_bindings]
    impl FeatureBindings for WidgetFeature {}

    #[test]
    fn feature_macro_fills_in_bindings_type() {
        fn assert_bindings_is<F: FeatureBindings<Bindings = WidgetBindings>>() {}
        assert_bindings_is::<WidgetFeature>();
    }
}
