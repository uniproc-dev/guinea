use std::time::{Duration, Instant};

use guinea_core::actor::Context;
use guinea_core::feature::Push;
use guinea_core::messages;
use guinea_macros::{actor, handler};

use super::contracts::{Metrics, Sampled};

pub struct MetricsActor {
    push: Push<Metrics>,
    tick: u64,
    start: Instant,
}

impl MetricsActor {
    pub fn new(push: Push<Metrics>) -> Self {
        Self {
            push,
            tick: 0,
            start: Instant::now(),
        }
    }
}

messages! { Tick }

actor! {
    MetricsActor {
        handlers {
            Tick => { bg Tick loop }
        }
    }
}

#[handler]
fn tick(this: &mut MetricsActor, ctx: Context<MetricsActor, Tick>) {
    this.tick += 1;
    let t = this.tick as f32;

    // Synthetic, deterministic waveforms - just enough to prove the chart
    // (overlay, per-series interpolation/fill, live update, hover) actually
    // works end to end.
    let cpu = (50.0 + 35.0 * (t / 6.0).sin()).clamp(0.0, 100.0);
    let memory = (40.0 + 20.0 * (t / 10.0).cos()).clamp(0.0, 100.0);
    let at = this.start.elapsed().as_millis() as u64;

    this.push.send(Sampled::At { at, cpu, memory });

    ctx.spawn_bg::<Tick, _>(async {
        tokio::time::sleep(Duration::from_millis(800)).await;
        Tick
    });
}
