use std::time::{Duration, Instant};

use guinea_core::actor::{Context, ManagedActor};
use guinea_macros::{actor_manifest, handler};

use super::contracts::{MetricsMsg, MetricsPort};

pub struct MetricsActor<P: MetricsPort> {
    ui_port: P,
    tick: u64,
    start: Instant,
}

impl<P: MetricsPort> MetricsActor<P> {
    pub fn new(ui_port: P) -> Self {
        Self { ui_port, tick: 0, start: Instant::now() }
    }
}

#[actor_manifest]
impl<P: MetricsPort + 'static> ManagedActor for MetricsActor<P> {
    type Handlers = handlers!(Tick);
}

#[handler]
fn tick<P: MetricsPort + 'static>(this: &mut MetricsActor<P>, _: Tick, ctx: &Context<MetricsActor<P>>) {
    this.tick += 1;
    let t = this.tick as f32;

    // Synthetic, deterministic waveforms - just enough to prove the chart
    // (overlay, per-series interpolation/fill, live update, hover) actually
    // works end to end.
    let cpu = (50.0 + 35.0 * (t / 6.0).sin()).clamp(0.0, 100.0);
    let memory = (40.0 + 20.0 * (t / 10.0).cos()).clamp(0.0, 100.0);
    let at = this.start.elapsed().as_millis() as u64;

    this.ui_port.send(MetricsMsg::Sample { at, cpu, memory });

    ctx.spawn_bg::<Tick, _>(async {
        tokio::time::sleep(Duration::from_millis(800)).await;
        Tick
    });
}
