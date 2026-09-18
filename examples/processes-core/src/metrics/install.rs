use guinea::prelude::*;

use super::actor::{MetricsActor, Tick};
use super::contracts;

pub struct MetricsFeature {
    _samples: Bound<contracts::Metrics>,
}

#[installs]
impl Feature for MetricsFeature {
    type Exports = (contracts::Metrics,);

    fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
        let (samples, _) = cx.state::<contracts::Metrics>().driven_by(MetricsActor::new);
        samples.emit(Tick);
        Ok(Self { _samples: samples })
    }
}
