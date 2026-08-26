//! guinea on egui: the same router and features, drawn immediately.
//!
//! The closest relative among the backends is ratatui, not WinUI: egui is
//! immediate, so a view is not a tree that is kept and diffed - it is drawing
//! that happens inside one frame and leaves nothing behind. As there, a view
//! is a [`Node`]: drawing deferred until someone supplies the [`egui::Ui`] to
//! draw into, which is what lets a layout decide where its child goes.
//!
//! What differs from the terminal is who owns the loop. eframe owns it, the
//! way the reactor does under WinUI, so [`run`] hands it over and puts the
//! frame inside `eframe::App::update`. And unlike a terminal, egui sleeps when
//! nothing happens - so work finished on another thread has to wake it, which
//! is what the dispatcher's `request_repaint` is for.

mod dispatcher;
mod nav;
mod run;

pub use run::{MAIN, run};

use guinea_app::feature::FeatureInitContext;
use guinea_core::scope::Reducer;
use guinea_core::uri::AppUri;
use guinea_router::router::{
    NavigateHandle, RouteChain, SegmentEntry, SegmentProps, ToUri, Ui, single_entry_chain,
};

/// egui as a [`Ui`].
pub struct Egui;

impl Ui for Egui {
    type View = Node;
}

/// Drawing that has not happened yet.
///
/// `FnOnce` because a node is drawn exactly once per frame - the next frame
/// mounts fresh ones.
pub struct Node(Box<dyn FnOnce(&mut egui::Ui)>);

impl Node {
    pub fn new(draw: impl FnOnce(&mut egui::Ui) + 'static) -> Self {
        Self(Box::new(draw))
    }

    /// Draws into `ui`.
    pub fn draw(self, ui: &mut egui::Ui) {
        (self.0)(ui)
    }
}

/// A leaf of the route tree.
pub trait Page: 'static {
    /// When `true`, the router keeps this page's reducer states in memory
    /// while the page is not mounted.
    const CACHE_STATE_IN_MEMORY: bool = false;

    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    /// Draws the page. Runs again for every frame, so this is the drawing
    /// itself and not a description of it.
    fn render(cx: &mut PageCx<'_>);
}

/// A branch: draws its own chrome and decides where its child goes.
pub trait Layout: 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn render(cx: &mut LayoutCx<'_>);
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<Egui> {
    SegmentEntry::new(
        std::any::TypeId::of::<P>,
        P::install,
        mount_page::<P>,
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<Egui> {
    SegmentEntry::new(std::any::TypeId::of::<L>, L::install, mount_layout::<L>, false)
}

pub fn mount_page<P: Page>(props: SegmentProps<Egui>) -> Node {
    Node::new(move |ui| P::render(&mut PageCx { ui, props }))
}

pub fn mount_layout<L: Layout>(props: SegmentProps<Egui>) -> Node {
    Node::new(move |ui| L::render(&mut LayoutCx { ui, props }))
}

/// A one-segment chain, for a page drawn without a route tree.
pub fn page_chain<P: Page>() -> &'static [SegmentEntry<Egui>] {
    single_entry_chain(segment_entry::<P>())
}

/// What a page's drawing is handed.
pub struct PageCx<'a> {
    ui: &'a mut egui::Ui,
    props: SegmentProps<Egui>,
}

impl PageCx<'_> {
    pub fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// The reducer's state and actions.
    ///
    /// No subscription, as in the terminal: egui redraws the whole frame, so
    /// there is nothing to invalidate - the next pass reads the state again.
    /// What does need saying is when a frame should happen at all, and that is
    /// the dispatcher's job.
    pub fn read<R>(&self) -> (R::State, std::rc::Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone,
    {
        let binding = self.props.binding::<R>();
        (binding.get(), binding.actions())
    }

    /// A navigator over the route type the application runs.
    pub fn navigate<R>(&self) -> NavigateHandle<Egui, R>
    where
        R: RouteChain<Egui> + ToUri + Clone + PartialEq + 'static,
    {
        nav::current::<R>()
    }
}

/// What a layout's drawing is handed. Same as a page's, plus the child.
pub struct LayoutCx<'a> {
    ui: &'a mut egui::Ui,
    props: SegmentProps<Egui>,
}

impl LayoutCx<'_> {
    pub fn ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    pub fn read<R>(&self) -> (R::State, std::rc::Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone,
    {
        let binding = self.props.binding::<R>();
        (binding.get(), binding.actions())
    }

    /// A navigator over the route type the application runs.
    pub fn navigate<R>(&self) -> NavigateHandle<Egui, R>
    where
        R: RouteChain<Egui> + ToUri + Clone + PartialEq + 'static,
    {
        nav::current::<R>()
    }

    /// The next segment down the chain, for the layout to draw where it wants.
    ///
    /// Handed over rather than drawn here: a layout takes its `ui` from this
    /// same context, so a method that drew the child would need the context
    /// twice at once.
    pub fn outlet(&self) -> Node {
        self.props.outlet()
    }

    /// Whether the segment directly below is `P`.
    ///
    /// What a tab strip needs, and cheaper than it looks: the chain already
    /// says which page is mounted, so highlighting the current tab needs
    /// neither the route nor a copy of it in state.
    pub fn child_is<P: 'static>(&self) -> bool {
        self.props
            .chain
            .get(self.props.cursor + 1)
            .is_some_and(|entry| (entry.type_id)() == std::any::TypeId::of::<P>())
    }
}
