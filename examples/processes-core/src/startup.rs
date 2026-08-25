use std::sync::Arc;
use std::time::Instant;

use guinea::app::{AppFeature, FeatureBuilder};
use guinea::feature::{ContextActorExt, ContextReactorExt};
use guinea_core::actor::Context;
use guinea_core::messages;
use guinea_macros::{actor, handler};
use guinea_plugin_store::Store;

use crate::events::ProcessKilled;

pub struct StartedAt(pub Instant);

messages! { Sweep }

#[derive(Default)]
pub struct Housekeeping {
    sweeps: u64,
}

actor! {
    Housekeeping {
        handlers { Sweep }
    }
}

#[handler]
fn sweep(this: &mut Housekeeping, _ctx: Context<Housekeeping, Sweep>) {
    this.sweeps += 1;
    tracing::debug!(sweeps = this.sweeps, "housekeeping sweep");
}

pub struct Startup;

impl AppFeature for Startup {
    fn install(self, app: &mut FeatureBuilder) -> anyhow::Result<()> {
        app.provide(StartedAt(Instant::now()));
        let started: Arc<StartedAt> = app.require()?;

        let store = app.require::<Store>()?;
        let launches = store.get::<u64>("app.launches")?.unwrap_or_default() + 1;
        store.set("app.launches", &launches)?;
        tracing::info!(launches, "started");

        let addr = app.spawn(Housekeeping::default());
        app.spawn_heartbeat(&addr, || 5_000, || Sweep);

        app.subscribe_global::<ProcessKilled>(|event| {
            tracing::info!(process = %event.name, "process killed");
        });

        app.on_cleanup(move |_| {
            let uptime = started.0.elapsed();
            tracing::info!(?uptime, "shutdown ran");
            Ok(())
        });

        Ok(())
    }
}
