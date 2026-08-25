use guinea_core::actor::Context;
use guinea_core::messages;
use guinea_macros::{actor, handler};

use super::contracts::{ServicesMessages, ServicesPort};

pub struct ServiceActor<P: ServicesPort> {
    ui_port: P,
}

impl<P: ServicesPort> ServiceActor<P> {
    pub fn new(ui_port: P) -> Self {
        Self { ui_port }
    }
}

messages! { Refresh }

actor! {
    ServiceActor<P: ServicesPort + 'static> {
        handlers { Refresh }
    }
}

#[handler]
fn refresh<P: ServicesPort + 'static>(this: &mut ServiceActor<P>, _ctx: Context<ServiceActor<P>, Refresh>) {
    this.ui_port.send(ServicesMessages::SetItems(vec![
        "sshd.service".to_string(),
        "docker.service".to_string(),
        "cron.service".to_string(),
    ]));
}
