use guinea_core::actor::ManagedActor;
use guinea_macros::{actor_manifest, handler};

use super::contracts::{ProcessesMessages, ProcessesPort};

pub struct ProcessActor<P: ProcessesPort> {
    ui_port: P,
    items: Vec<String>,
}

impl<P: ProcessesPort> ProcessActor<P> {
    pub fn new(_context: String, ui_port: P) -> Self {
        Self {
            ui_port,
            items: Vec::new(),
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
}

#[handler]
fn kill<P: ProcessesPort + 'static>(this: &mut ProcessActor<P>, msg: Kill) {
    let needle = format!("(pid {})", msg.0);
    this.items.retain(|row| !row.ends_with(&needle));
    this.publish();
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
