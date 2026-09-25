use guinea::prelude::*;

use super::actor::{MetricsActor, Tick};
use super::contracts;

feature! {
    pub MetricsFeature {
        exports { contracts::Metrics }
    }
}

#[installs]
fn metrics(cx: &FeatureInitContext) -> anyhow::Result<MetricsFeature> {
    let (samples, _) = cx.state::<contracts::Metrics>().driven_by(MetricsActor::new);
    samples.emit(Tick);
    Ok(MetricsFeature(samples))
}
