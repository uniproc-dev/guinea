use crate::actor::addr::Addr;
use crate::actor::shape::Shape;
use crate::actor::short_type_name;
use crate::actor::traits::ManagedActor;
use parking_lot::RwLock;
use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Default)]
pub struct ActorRegistry {
    actors: RwLock<HashMap<TypeId, Box<dyn Any>>>,
}

// HACK: this registry for test only
unsafe impl Send for ActorRegistry {}
unsafe impl Sync for ActorRegistry {}

impl ActorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<A: 'static>(&self, addr: Addr<A>) {
        let mut actors = self.actors.write();
        actors.insert(TypeId::of::<A>(), Box::new(addr));
    }

    pub fn get<A: 'static>(&self) -> Option<Addr<A>> {
        let actors = self.actors.read();
        actors
            .get(&TypeId::of::<A>())?
            .downcast_ref::<Addr<A>>()
            .cloned()
    }
}

pub struct ActorSnapshot {
    pub id: usize,
    pub type_name: &'static str,
    pub state: String,
    pub shape: Shape,
    pub owner: Owner,
}

/// Where an actor lives, for grouping it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Owner {
    /// The scope that owns it, as `Scope::key` names it.
    pub scope: Option<usize>,
    /// The feature that created it.
    pub feature: Option<&'static str>,
    /// The reducer it drives, when it was created with `driven_by`.
    pub drives: Option<&'static str>,
}

struct DebugEntry {
    type_name: &'static str,
    shape: Shape,
    owner: Owner,
    snapshot: Box<dyn Fn() -> String>,
}

#[derive(Default)]
pub struct DebugRegistry {
    entries: RwLock<HashMap<usize, DebugEntry>>,
}

impl DebugRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<A: std::fmt::Debug + ManagedActor>(&self, addr: &Addr<A>) {
        self.register_owned(addr, Owner::default());
    }

    pub fn register_owned<A: std::fmt::Debug + ManagedActor>(&self, addr: &Addr<A>, owner: Owner) {
        let addr = addr.clone();
        self.entries.write().insert(
            addr.id(),
            DebugEntry {
                type_name: short_type_name::<A>(),
                shape: A::SHAPE,
                owner,
                snapshot: Box::new(move || addr.debug_snapshot()),
            },
        );
    }

    pub fn unregister(&self, id: usize) {
        self.entries.write().remove(&id);
    }

    pub fn clear(&self) {
        self.entries.write().clear();
    }

    pub fn snapshots(&self) -> Vec<ActorSnapshot> {
        self.entries
            .read()
            .iter()
            .map(|(&id, entry)| ActorSnapshot {
                id,
                type_name: entry.type_name,
                shape: entry.shape,
                owner: entry.owner,
                state: (entry.snapshot)(),
            })
            .collect()
    }
}
