//! A page with no window.
//!
//! Mounted on reactor's recording runtime: the native tree it would build is
//! read back as data, and its messages and clicks are delivered the way the
//! window would deliver them. It lives in a segment of guinea-app's
//! `Harness`, so everything the page sets off runs in the seed's order and on
//! the test's clock.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use guinea_app::app::{Harness, Segment};
use guinea_core::mark::Mark;
use guinea_core::scope::Scope;
use guinea_router::router::{SegmentEntry, SegmentProps};
use windows_reactor::View;
use windows_reactor::test::{
    EventId, EventPayload, NodeId, PropertyId, PropertyValue, Pump, QueuedEvent, RealizedContainer,
    RecordingRuntime,
};

use crate::winui::{Page, PageNode, Signal, WinUi, install_page, segment_entry};

/// How many component turns one pass may run before it looks again.
const TURNS: usize = 64;

type Sender<M> = Rc<dyn Fn(Signal<M>) -> bool>;

thread_local! {
    static SENDERS: RefCell<HashMap<TypeId, Box<dyn Any>>> = RefCell::new(HashMap::new());
    static CHAINS: RefCell<HashMap<(TypeId, usize), &'static [SegmentEntry<WinUi>]>> =
        RefCell::new(HashMap::new());
}

/// What a mounted page's component answers to, kept as it is created.
pub(crate) fn remember<P: Page>(send: impl Fn(Signal<P::Message>) -> bool + 'static) {
    let send: Sender<P::Message> = Rc::new(send);
    SENDERS.with(|senders| {
        senders.borrow_mut().insert(TypeId::of::<P>(), Box::new(send));
    });
}

fn sender<P: Page>() -> Sender<P::Message> {
    SENDERS.with(|senders| {
        senders
            .borrow()
            .get(&TypeId::of::<P>())
            .and_then(|send| send.downcast_ref::<Sender<P::Message>>())
            .cloned()
            .unwrap_or_else(|| panic!("{} is not mounted", std::any::type_name::<P>()))
    })
}

/// A chain `depth` long whose every entry is this page's own: a page below
/// `depth - 1` segments names itself by its place in the chain, and the
/// segments above it have no entries of their own here. Made once per page
/// and depth.
fn chain<P: Page>(depth: usize) -> &'static [SegmentEntry<WinUi>] {
    CHAINS.with(|chains| {
        *chains
            .borrow_mut()
            .entry((TypeId::of::<P>(), depth))
            .or_insert_with(|| {
                let entries: Vec<SegmentEntry<WinUi>> =
                    (0..depth).map(|_| segment_entry::<P>()).collect();
                Box::leak(entries.into_boxed_slice())
            })
    })
}

/// One element of what a page drew, as a snapshot keeps it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Node {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Its `AutomationId` - the mark every page and layout carries on its
    /// border, and any the page set itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

/// A page mounted with no window, in a segment of a [`Harness`].
pub struct Mounted<'h, P: Page> {
    harness: &'h Harness,
    pump: Pump<RecordingRuntime>,
    /// The items brought into view so far, by list and index.
    realized: HashMap<(NodeId, usize), NodeId>,
    page: PhantomData<P>,
}

impl<'h, P: Page> Mounted<'h, P> {
    /// Installs `P` into `segment` - what it `Installs`, and the node it
    /// starts as - and mounts it, the way a navigation to it would.
    pub fn mount(segment: &Segment<'h>, params: P::Params) -> anyhow::Result<Self> {
        let cx = segment.context();
        install_page::<P>(cx, &params)?;

        let scopes: Vec<Rc<Scope>> = cx
            .ancestors
            .iter()
            .cloned()
            .chain([cx.scope.clone()])
            .collect();
        let depth = scopes.len();

        let props = SegmentProps {
            chain: chain::<P>(depth),
            scopes: Rc::new(scopes),
            cursor: depth - 1,
        };

        let mut pump = Pump::new(RecordingRuntime::default());
        pump.mount_view(View::component::<PageNode<P>>(props))
            .map_err(|refused| anyhow::anyhow!("mounting {}: {refused:?}", std::any::type_name::<P>()))?;

        let mut mounted = Self {
            harness: segment.harness(),
            pump,
            realized: HashMap::new(),
            page: PhantomData,
        };
        mounted.settle();

        Ok(mounted)
    }

    /// Hands the page one of its own messages, as a widget's callback would.
    pub fn send(&mut self, message: P::Message) {
        let delivered = sender::<P>()(Signal::Node(message));
        assert!(delivered, "{} no longer takes messages", std::any::type_name::<P>());

        self.turn();
    }

    /// Clicks what carries `mark` the way a pointer would: on the nearest
    /// element at or above it that listens for a click.
    pub fn click(&mut self, mark: impl Mark) {
        let root = self.page_root();
        let found = self.marked(root, &mark);
        self.click_at(found, mark.name());
    }

    /// [`click`](Self::click) for what shows `text` and carries no mark.
    pub fn click_text(&mut self, text: &str) {
        let root = self.page_root();
        let found = self.showing(root, text);
        self.click_at(found, text);
    }

    /// The first element, depth first, that carries `mark`.
    pub fn find(&self, mark: impl Mark) -> Option<NodeId> {
        self.first(self.root()?, PropertyId::AutomationId, mark.name())
    }

    /// The first element, depth first, that shows `text`.
    pub fn find_text(&self, text: &str) -> Option<NodeId> {
        self.first(self.root()?, PropertyId::TextBlockText, text)
    }

    /// The part of the page that carries `mark`, to find and click in.
    pub fn within(&mut self, mark: impl Mark) -> Within<'_, 'h, P> {
        let root = self.page_root();
        let found = self.marked(root, &mark);

        Within {
            mounted: self,
            root: found,
        }
    }

    /// Item `index` of the first list on the page, brought into view the way
    /// scrolling to it would - a list builds only the items on screen, and
    /// with no screen, none until asked.
    pub fn item(&mut self, index: usize) -> Within<'_, 'h, P> {
        let root = self.page_root();
        let found = self.realize(root, index);

        Within {
            mounted: self,
            root: found,
        }
    }

    fn marked(&self, under: NodeId, mark: &impl Mark) -> NodeId {
        let name = mark.name();
        self.first(under, PropertyId::AutomationId, name)
            .unwrap_or_else(|| panic!("nothing here is marked {name:?}:\n{:#?}", self.node(under)))
    }

    fn showing(&self, under: NodeId, text: &str) -> NodeId {
        self.first(under, PropertyId::TextBlockText, text)
            .unwrap_or_else(|| panic!("nothing here shows {text:?}:\n{:#?}", self.node(under)))
    }

    /// The list at or under `under`, with item `index` realized in it.
    fn realize(&mut self, under: NodeId, index: usize) -> NodeId {
        let list = self
            .list(under)
            .unwrap_or_else(|| panic!("there is no list here:\n{:#?}", self.node(under)));

        let children = |mounted: &Self| -> Vec<NodeId> {
            mounted
                .pump
                .runtime()
                .node(list)
                .map(|recorded| recorded.children().to_vec())
                .unwrap_or_default()
        };

        if let Some(item) = self.realized.get(&(list, index))
            && children(self).contains(item)
        {
            return *item;
        }

        let before = children(self);
        self.pump
            .runtime_mut()
            .queue_realize(list, RealizedContainer(index as u64 + 1), index);
        self.pump
            .process_realizations()
            .expect("bringing an item into view");
        self.settle();

        let item = children(self)
            .into_iter()
            .find(|child| !before.contains(child))
            .unwrap_or_else(|| panic!("the list has no item {index}"));
        self.realized.insert((list, index), item);

        item
    }

    fn list(&self, under: NodeId) -> Option<NodeId> {
        let mut unseen = vec![under];

        while let Some(node) = unseen.pop() {
            if self.pump.runtime().source_revision(node).is_some() {
                return Some(node);
            }

            let recorded = self.pump.runtime().node(node)?;
            unseen.extend(recorded.children().iter().rev().copied());
        }

        None
    }

    fn click_at(&mut self, found: NodeId, label: &str) {
        let parents = self.parents();
        let mut at = Some(found);

        while let Some(node) = at {
            if self.press(node) {
                self.turn();
                return;
            }
            at = parents.get(&node).copied();
        }

        panic!("{label:?} is on the page, but nothing at or above it listens for a click");
    }

    /// Runs everything until nothing is left: the harness's work, then what
    /// reached the page, then the page drawing again - as often as one sets
    /// off another.
    pub fn settle(&mut self) {
        loop {
            let ran = self.harness.settled() + self.turn();
            if ran == 0 {
                return;
            }
        }
    }

    /// The outermost element the page drew: what the window holds.
    fn root(&self) -> Option<NodeId> {
        let window = self.pump.window()?;
        self.pump.runtime().node(window)?.children().first().copied()
    }

    fn page_root(&self) -> NodeId {
        self.root().expect("a mounted page has drawn something")
    }

    /// What the page drew, from its outermost element down.
    pub fn tree(&self) -> Node {
        self.node(self.page_root())
    }

    fn first(&self, under: NodeId, property: PropertyId, value: &str) -> Option<NodeId> {
        let mut unseen = vec![under];

        while let Some(node) = unseen.pop() {
            let recorded = self.pump.runtime().node(node)?;

            if text_of(recorded.property(property)).as_deref() == Some(value) {
                return Some(node);
            }

            unseen.extend(recorded.children().iter().rev().copied());
        }

        None
    }

    fn node(&self, id: NodeId) -> Node {
        let Some(recorded) = self.pump.runtime().node(id) else {
            return Node {
                kind: "?".to_string(),
                text: None,
                id: None,
                children: Vec::new(),
            };
        };

        Node {
            kind: recorded
                .kind()
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "?".to_string()),
            text: text_of(recorded.property(PropertyId::TextBlockText)),
            id: text_of(recorded.property(PropertyId::AutomationId)),
            children: recorded.children().iter().map(|child| self.node(*child)).collect(),
        }
    }

    fn parents(&self) -> HashMap<NodeId, NodeId> {
        let mut parents = HashMap::new();
        let mut unseen: Vec<NodeId> = self.root().into_iter().collect();

        while let Some(node) = unseen.pop() {
            if let Some(recorded) = self.pump.runtime().node(node) {
                for child in recorded.children() {
                    parents.insert(*child, node);
                    unseen.push(*child);
                }
            }
        }

        parents
    }

    /// Delivers a click to `node` if it listens for one.
    fn press(&mut self, node: NodeId) -> bool {
        if let Some(revision) = self.pump.event_revision(node, EventId::ButtonClick) {
            self.pump.queue_event(QueuedEvent::new(
                node,
                EventId::ButtonClick,
                revision,
                EventPayload::Unit,
            ));
            return true;
        }

        let Some(released) = self.pump.event_revision(node, EventId::BorderPointerReleased) else {
            return false;
        };

        if let Some(pressed) = self.pump.event_revision(node, EventId::BorderPointerPressed) {
            self.pump.queue_event(QueuedEvent::new(
                node,
                EventId::BorderPointerPressed,
                pressed,
                EventPayload::PointerEventInfo(Default::default()),
            ));
        }
        self.pump.queue_event(QueuedEvent::new(
            node,
            EventId::BorderPointerReleased,
            released,
            EventPayload::PointerEventInfo(Default::default()),
        ));

        true
    }

    /// One pass of what the window would do between frames: deliver the
    /// events waiting for the page, then let its components update and draw.
    fn turn(&mut self) -> usize {
        let events = self.pump.dispatch_events().expect("delivering events to the page");
        let turns = self
            .pump
            .dispatch_components(TURNS)
            .expect("running the page's components");

        events + turns
    }
}

/// Part of a mounted page: what is found and clicked through it is found
/// under it, so the same mark in every row of a list names one thing again.
pub struct Within<'m, 'h, P: Page> {
    mounted: &'m mut Mounted<'h, P>,
    root: NodeId,
}

impl<P: Page> Within<'_, '_, P> {
    /// The part of this part that carries `mark`.
    pub fn within(self, mark: impl Mark) -> Self {
        let root = self.mounted.marked(self.root, &mark);
        Self { root, ..self }
    }

    /// Item `index` of the first list in this part - see [`Mounted::item`].
    pub fn item(self, index: usize) -> Self {
        let root = self.mounted.realize(self.root, index);
        Self { root, ..self }
    }

    pub fn find(&self, mark: impl Mark) -> Option<NodeId> {
        self.mounted.first(self.root, PropertyId::AutomationId, mark.name())
    }

    pub fn find_text(&self, text: &str) -> Option<NodeId> {
        self.mounted.first(self.root, PropertyId::TextBlockText, text)
    }

    /// Clicks what carries `mark` in this part - see [`Mounted::click`].
    pub fn click(self, mark: impl Mark) {
        let found = self.mounted.marked(self.root, &mark);
        self.mounted.click_at(found, mark.name());
    }

    pub fn click_text(self, text: &str) {
        let found = self.mounted.showing(self.root, text);
        self.mounted.click_at(found, text);
    }

    /// Clicks this part itself: a row, to select it.
    pub fn click_here(self) {
        self.mounted.click_at(self.root, "this part");
    }

    /// What this part drew, from its outermost element down.
    pub fn tree(&self) -> Node {
        self.mounted.node(self.root)
    }
}

fn text_of(value: Option<&PropertyValue>) -> Option<String> {
    match value? {
        PropertyValue::Str(text) => Some(text.to_string()),
        _ => None,
    }
}
