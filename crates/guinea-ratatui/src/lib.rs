//! guinea on ratatui: the same router and features, drawn in a terminal.
//!
//! The interesting difference from the WinUI backend is what a view *is*.
//! WinUI is retained: a view builds an element tree and hands it over.
//! ratatui is immediate: a view draws into a frame that only exists during
//! one pass, and nothing survives it. So here a view is neither of those - it
//! is a [`Node`], a piece of drawing deferred until someone supplies a frame
//! and an area.
//!
//! That indirection is what lets a layout place its child. `mount` receives
//! only [`SegmentProps`], never a frame, so a node cannot draw when it is
//! built; by the time the frame exists the layout has decided the geometry and
//! passes down whatever rectangle it wants the page to occupy.

mod dispatcher;
mod keys;
mod run;

pub use keys::pressed;
pub use run::{Flow, run};

use guinea_app::feature::FeatureInitContext;
use guinea_core::scope::Reducer;
use guinea_core::uri::AppUri;
use guinea_router::router::{SegmentEntry, SegmentProps, Ui, single_entry_chain};
use ratatui::Frame;
use ratatui::layout::Rect;

/// ratatui as a [`Ui`].
pub struct Tui;

impl Ui for Tui {
    type View = Node;
}

/// Drawing that has not happened yet.
///
/// Boxed because a segment's drawing captures its own scope and props, and
/// `FnOnce` because a node is drawn exactly once per frame - the next frame
/// mounts fresh ones.
pub struct Node(Box<dyn FnOnce(&mut Frame, Rect)>);

impl Node {
    pub fn new(draw: impl FnOnce(&mut Frame, Rect) + 'static) -> Self {
        Self(Box::new(draw))
    }

    /// Draws into `area` of `frame`.
    pub fn draw(self, frame: &mut Frame, area: Rect) {
        (self.0)(frame, area)
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

    fn view(cx: &mut PageCx<'_, '_>);
}

/// A branch: draws its own chrome and decides where its child goes.
pub trait Layout: 'static {
    fn install(_ctx: &FeatureInitContext, _uri: &AppUri) -> anyhow::Result<()> {
        Ok(())
    }

    fn view(cx: &mut LayoutCx<'_, '_>);
}

pub const fn segment_entry<P: Page>() -> SegmentEntry<Tui> {
    SegmentEntry::new(
        std::any::TypeId::of::<P>,
        P::install,
        mount_page::<P>,
        P::CACHE_STATE_IN_MEMORY,
    )
}

pub const fn layout_entry<L: Layout>() -> SegmentEntry<Tui> {
    SegmentEntry::new(std::any::TypeId::of::<L>, L::install, mount_layout::<L>, false)
}

pub fn mount_page<P: Page>(props: SegmentProps<Tui>) -> Node {
    Node::new(move |frame, area| {
        P::view(&mut PageCx {
            frame,
            area,
            props,
        })
    })
}

pub fn mount_layout<L: Layout>(props: SegmentProps<Tui>) -> Node {
    Node::new(move |frame, area| {
        L::view(&mut LayoutCx {
            frame,
            area,
            props,
        })
    })
}

/// A one-segment chain, for a page drawn without a route tree.
pub fn page_chain<P: Page>() -> &'static [SegmentEntry<Tui>] {
    single_entry_chain(segment_entry::<P>())
}

/// What a page's view is handed: the frame it draws into, the rectangle it was
/// given, and a way to read the state its feature owns.
pub struct PageCx<'a, 'b> {
    frame: &'a mut Frame<'b>,
    area: Rect,
    props: SegmentProps<Tui>,
}

impl<'b> PageCx<'_, 'b> {
    pub fn frame(&mut self) -> &mut Frame<'b> {
        self.frame
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    /// The reducer's state and actions.
    ///
    /// No subscription, unlike the reactor's `use_reducer`: a terminal redraws
    /// the whole frame on its own schedule, so there is nothing to invalidate -
    /// the next pass reads the state again.
    pub fn read<R>(&self) -> (R::State, std::rc::Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone,
    {
        let binding = self.props.binding::<R>();
        (binding.get(), binding.actions())
    }
}

/// What a layout's view is handed. Same as a page's, plus the child.
pub struct LayoutCx<'a, 'b> {
    frame: &'a mut Frame<'b>,
    area: Rect,
    props: SegmentProps<Tui>,
}

impl<'b> LayoutCx<'_, 'b> {
    pub fn frame(&mut self) -> &mut Frame<'b> {
        self.frame
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn read<R>(&self) -> (R::State, std::rc::Rc<R::Actions>)
    where
        R: Reducer,
        R::State: Clone,
    {
        let binding = self.props.binding::<R>();
        (binding.get(), binding.actions())
    }

    /// Draws the next segment down the chain into `area`.
    ///
    /// The whole reason a view is deferred: the child was mounted before any
    /// frame existed, and only here is it known where it belongs.
    pub fn outlet(&mut self, area: Rect) {
        self.props.outlet().draw(self.frame, area);
    }

    /// Whether the segment directly below is `P`.
    ///
    /// What a tab strip needs, and cheaper than it looks: the chain already
    /// says which page is mounted, so highlighting the current tab needs
    /// neither the route nor a copy of it in state - and a copy is a second
    /// thing to keep in step.
    pub fn child_is<P: 'static>(&self) -> bool {
        self.props
            .chain
            .get(self.props.cursor + 1)
            .is_some_and(|entry| (entry.type_id)() == std::any::TypeId::of::<P>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::actor::UiThreadToken;
    use guinea_router::router::Router;
    use ratatui::layout::{Constraint, Direction, Layout as RLayout};
    use ratatui::widgets::Paragraph;
    use ratatui::{Terminal, backend::TestBackend};

    struct Shell;

    impl Layout for Shell {
        fn view(cx: &mut LayoutCx<'_, '_>) {
            let chunks = RLayout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(cx.area());

            let area = cx.area();
            cx.frame().render_widget(Paragraph::new("tabs"), chunks[0]);
            let _ = area;
            cx.outlet(chunks[1]);
        }
    }

    struct Processes;

    impl Page for Processes {
        fn view(cx: &mut PageCx<'_, '_>) {
            let area = cx.area();
            cx.frame().render_widget(Paragraph::new("processes"), area);
        }
    }

    const CHAIN: [SegmentEntry<Tui>; 2] = [layout_entry::<Shell>(), segment_entry::<Processes>()];

    fn rendered() -> String {
        let token = UiThreadToken::dangerously_create_token_unchecked();
        let router = Router::<Tui>::new(token);
        let uri = AppUri::parse("/processes").unwrap();
        router.activate(&uri, &CHAIN).expect("activate");

        let mut terminal = Terminal::new(TestBackend::new(12, 3)).expect("terminal");
        terminal
            .draw(|frame| router.render().draw(frame, frame.area()))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_layout_places_the_page_it_wraps() {
        // The same router, the same chain, drawn by a backend with no widget
        // tree at all - the layout put its chrome on the first row and handed
        // the rest to the page.
        assert_eq!(rendered().trim_end(), "tabs\nprocesses");
    }
}
