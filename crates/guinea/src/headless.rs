//! A backend that draws nothing.
//!
//! Not a stub kept alive for tests: it is the proof that [`Ui`] demands
//! nothing of a backend. Its view type is `()`, it has no render context worth
//! the name, and the router works with it unchanged - so any requirement that
//! quietly crept into the agnostic half would fail to compile here first.

use guinea_core::scope::Reducer;

use crate::router::{Layout, Page, SegmentEntry, SegmentProps, Ui, single_entry_chain};

pub struct Headless;

impl Ui for Headless {
    type View = ();
    type PageCx<'a> = HeadlessCx;
    type LayoutCx<'a> = HeadlessCx;
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

pub fn mount_page<P: Page<Headless>>(props: SegmentProps<Headless>) {
    P::view(&mut HeadlessCx { props })
}

pub fn mount_layout<L: Layout<Headless>>(props: SegmentProps<Headless>) {
    L::view(&mut HeadlessCx { props })
}

pub fn page_chain<P: Page<Headless>>() -> &'static [SegmentEntry<Headless>] {
    single_entry_chain::<Headless, P>(mount_page::<P>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::FeatureInitContext;
    use crate::router::Router;
    use crate::uri::AppUri;
    use guinea_core::actor::UiThreadToken;
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

    impl Page<Headless> for Page1 {
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
    fn the_router_runs_with_a_backend_that_draws_nothing() {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<Headless>::new(token);
        let uri = AppUri::parse("/anything").unwrap();

        let scope = router
            .activate::<Page1>(&uri, mount_page::<Page1>)
            .expect("activate");

        assert_eq!(scope.state::<Counter>().borrow().installs, 1);

        // Rendering is a plain call here - there is no reconciler to hand the
        // mounted view to, which is exactly what makes this a useful check.
        router.render();
    }
}
