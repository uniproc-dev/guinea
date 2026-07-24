use std::rc::Rc;

use guinea_core::actor::{Context, ManagedActor};
use guinea_core::actor::event_bus::EventBus;
use guinea_macros::{actor_manifest, handler};

use crate::events::ProcessKilled;

use super::contracts::{ProcessesMessages, ProcessesPort};

pub struct ProcessActor<P: ProcessesPort> {
    ui_port: P,
    items: Vec<String>,
    event_bus: Rc<EventBus>,
}

impl<P: ProcessesPort> ProcessActor<P> {
    pub fn new(_context: String, ui_port: P, event_bus: Rc<EventBus>) -> Self {
        Self {
            ui_port,
            items: Vec::new(),
            event_bus,
        }
    }

    fn publish(&self) {
        self.ui_port
            .send(ProcessesMessages::SetItems(self.items.clone()));
    }
}

#[actor_manifest]
impl<P: ProcessesPort + 'static> ManagedActor for ProcessActor<P> {
    type Handlers = handlers!(
        bind {
            Kill(pub u32)
        },
        Refresh
    );
    type Signals = bus!(ProcessKilled);
}

#[handler]
fn kill<P: ProcessesPort + 'static>(this: &mut ProcessActor<P>, msg: Kill, ctx: &Context<ProcessActor<P>>) {
    let needle = format!("(pid {})", msg.0);
    let killed = this.items.iter().find(|row| row.ends_with(&needle)).cloned();
    this.items.retain(|row| !row.ends_with(&needle));
    this.publish();

    if let Some(name) = killed {
        // Window-local: only this window's `TabsLayout` hears it.
        ctx.publish_local(&this.event_bus, ProcessKilled { name: name.clone() });
        // Process-wide: every window's `TabsLayout` hears it, including
        // ones opened after this event fires.
        ctx.publish(ProcessKilled { name });
    }
}

#[handler]
fn refresh<P: ProcessesPort + 'static>(this: &mut ProcessActor<P>, _: Refresh) {
    this.items = vec![
        "systemd (pid 1)".to_string(),
        "sshd (pid 42)".to_string(),
        "bash (pid 512)".to_string(),
        "cargo (pid 900)".to_string(),
    ];
    this.publish();
}
