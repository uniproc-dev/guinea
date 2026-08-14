//! A backend that draws nothing.
//!
//! Not a stub kept alive for tests: it is the proof that [`Ui`] demands
//! nothing of a backend. Its view type is `()`, it has no render context worth
//! the name, and the router works with it unchanged - so any requirement that
//! quietly crept into the agnostic half would fail to compile here first.

use guinea_core::scope::Reducer;

use guinea_app::feature::FeatureInitContext;
use crate::router::{SegmentEntry, SegmentProps, Ui, single_entry_chain};
use guinea_core::uri::AppUri;

pub struct Headless;

impl Ui for Headless {
    type View = ();
}

/// A leaf, in a backend that renders nothing. Note how little it has in common
/// with `winui::Page` beyond the name - which is the reason neither of them
/// belongs in the router.
pub trait Page: 'static {
    const CACHE_STATE_IN_MEMORY: bool = false;

    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut HeadlessCx);
}

pub trait Layout: 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut HeadlessCx);
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<Headless> {
    SegmentEntry::new(
        std::any::TypeId::of::<P>,
        P::install,
        mount_page::<P>,
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<Headless> {
    SegmentEntry::new(
        std::any::TypeId::of::<L>,
        L::install,
        mount_layout::<L>,
        false,
    )
}

/// What a headless view gets: the segment it belongs to, and nothing else.
pub struct HeadlessCx {
    props: SegmentProps<Headless>,
}

impl HeadlessCx {
    /// The reducer's current state and actions. No subscription: with nothing
    /// to re-render, a change is observed by reading again.
    pub fn read<R>(&self) -> (R::State, std::rc::Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone,
    {
        let binding = self.props.binding::<R>();
        (binding.get(), binding.actions())
    }

    pub fn outlet(&self) {
        self.props.outlet()
    }
}

pub fn mount_page<P: Page>(props: SegmentProps<Headless>) {
    P::view(&mut HeadlessCx { props })
}

pub fn mount_layout<L: Layout>(props: SegmentProps<Headless>) {
    L::view(&mut HeadlessCx { props })
}

pub fn page_chain<P: Page>() -> &'static [SegmentEntry<Headless>] {
    single_entry_chain(segment_entry::<P>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_app::feature::FeatureInitContext;
    use crate::router::Router;
    use guinea_core::uri::AppUri;
    use guinea_core::actor::UiThreadToken;
    use std::rc::Rc;
    use guinea_macros::reducer;

    #[derive(Clone, Default, PartialEq, Debug)]
    struct CounterState {
        installs: u32,
    }

    #[reducer]
    fn counter(state: &mut CounterState, msg: u32) {
        state.installs += msg;
    }

    struct Page1;

    impl Page for Page1 {
        fn install(ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
            ctx.port::<Counter>()(1);
            Ok(())
        }

        fn view(cx: &mut HeadlessCx) {
            let (state, _) = cx.read::<Counter>();
            assert_eq!(state.installs, 1);
        }
    }

    #[test]
    fn route_hooks_see_the_previous_and_current_path() {
        use std::cell::RefCell;

        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<Headless>::new(token);

        let seen: Rc<RefCell<Vec<(Option<String>, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let recorded = seen.clone();
        router.on_route_change(move |from, to| {
            recorded
                .borrow_mut()
                .push((from.map(str::to_string), to.to_string()));
        });

        router.route_changed("/a");
        router.route_changed("/b");

        let seen = seen.borrow();
        assert_eq!(seen[0], (None, "/a".to_string()));
        assert_eq!(seen[1], (Some("/a".to_string()), "/b".to_string()));
    }

    #[test]
    fn the_router_runs_with_a_backend_that_draws_nothing() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<Headless>::new(token);
        let uri = AppUri::parse("/anything").unwrap();

        let scope = router
            .activate(&uri, page_chain::<Page1>())
            .expect("activate");

        assert_eq!(scope.state::<Counter>().borrow().installs, 1);

        // Rendering is a plain call here - there is no reconciler to hand the
        // mounted view to, which is exactly what makes this a useful check.
        router.render();
    }
}
