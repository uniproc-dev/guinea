use crate::actor::addr::Addr;
use crate::actor::short_type_name;
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
}

struct DebugEntry {
    type_name: &'static str,
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

    pub fn register<A: std::fmt::Debug + 'static>(&self, addr: &Addr<A>) {
        let addr = addr.clone();
        self.entries.write().insert(
            addr.id(),
            DebugEntry {
                type_name: short_type_name::<A>(),
                snapshot: Box::new(move || addr.debug_snapshot()),
            },
        );
    }

    pub fn unregister(&self, id: usize) {
        self.entries.write().remove(&id);
    }

    pub fn snapshots(&self) -> Vec<ActorSnapshot> {
        self.entries
            .read()
            .iter()
            .map(|(&id, entry)| ActorSnapshot {
                id,
                type_name: entry.type_name,
                state: (entry.snapshot)(),
            })
            .collect()
    }
}
