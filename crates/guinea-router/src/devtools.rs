//! Every live router on this thread, readable without knowing its backend or
//! its route type.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use guinea_app::app::roots::{self, RootId};
use guinea_core::actor::registry::ActorSnapshot;
use guinea_core::devtools::Panel;
use guinea_core::actor::shape::Declared;
use guinea_core::scope::{DescribedState, Installed, Listener};

use crate::router::{Router, Ui};

/// A router as devtools see it.
pub struct RouterView {
    pub root: RootId,
    pub label: Option<String>,
    /// The backend's type name: `Egui`, `WinUi`.
    pub backend: &'static str,
    pub route: Option<String>,
    pub segments: Vec<SegmentView>,
    pub back: usize,
    pub forward: usize,
    pub pending: Option<String>,
    /// Actors spawned or driven by features under this router.
    pub actors: Vec<ActorSnapshot>,
    /// What the backend offered for this root.
    pub panels: Vec<Panel>,
    /// What is subscribed to this window's bus, by event.
    pub bus: Vec<(&'static str, usize)>,
}

pub struct SegmentView {
    pub name: &'static str,
    /// Matches `Owner::scope` on the actors this segment owns.
    pub scope: usize,
    /// Where `routes!` listed this page or layout.
    pub declared: Option<Declared>,
    /// Where the page or layout itself was written.
    pub written: Option<Declared>,
    pub features: Vec<Installed>,
    pub states: Vec<DescribedState>,
    pub listeners: Vec<Listener>,
}

trait Inspected {
    fn view(&self) -> RouterView;
}

thread_local! {
    static ROUTERS: RefCell<Vec<Weak<dyn Inspected>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn register<U: Ui>(router: &Rc<Router<U>>) {
    let weak = Rc::downgrade(&(router.clone() as Rc<dyn Inspected>));
    ROUTERS.with(|routers| routers.borrow_mut().push(weak));
}

/// Every router on this thread that is still alive, in the order they first
/// navigated.
pub fn routers() -> Vec<RouterView> {
    let alive: Vec<Rc<dyn Inspected>> = ROUTERS.with(|routers| {
        let mut routers = routers.borrow_mut();
        routers.retain(|router| router.strong_count() > 0);
        routers.iter().filter_map(Weak::upgrade).collect()
    });
    alive.iter().map(|router| router.view()).collect()
}

/// The name a segment goes by: its type, without the path or the generics.
pub fn short(name: &'static str) -> &'static str {
    let generic = name.find('<').unwrap_or(name.len());
    let start = name[..generic].rfind("::").map_or(0, |at| at + 2);
    &name[start..generic]
}

impl<U: Ui> Inspected for Router<U> {
    fn view(&self) -> RouterView {
        let root = self.root();
        let segments = match (self.active_chain(), self.active_scopes()) {
            (Some(chain), Some(scopes)) => chain
                .iter()
                .zip(scopes.iter())
                .map(|(entry, scope)| SegmentView {
                    name: short((entry.type_name)()),
                    scope: scope.key(),
                    declared: entry.declared,
                    written: entry.written,
                    features: scope.features(),
                    states: scope.describe_states(),
                    listeners: scope.listeners(),
                })
                .collect(),
            _ => Vec::new(),
        };

        let mut actors = self.host().debug_registry().snapshots();
        actors.sort_by_key(|actor| actor.id);

        RouterView {
            root,
            label: roots::label(root),
            backend: short(std::any::type_name::<U>()),
            route: self.described(),
            segments,
            back: self.history_len().0,
            forward: self.history_len().1,
            pending: self.pending().map(|ask| ask.text),
            actors,
            panels: guinea_core::devtools::panels(root.get()),
            bus: self.host().event_bus().subscriptions(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn names_lose_their_path_and_keep_their_generics_out() {
        assert_eq!(short("app::pages::Processes"), "Processes");
        assert_eq!(short("guinea_eframe::Egui"), "Egui");
        assert_eq!(short("a::List<b::Recent>"), "List");
        assert_eq!(short("Plain"), "Plain");
    }
}
