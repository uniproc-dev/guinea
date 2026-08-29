use guinea::feature::{Feature, FeatureInitContext};
use guinea_core::feature::Bound;
use guinea_macros::installs;

use super::actor::{MetricsActor, Tick};
use super::contracts;

pub struct MetricsFeature {
    _samples: Bound<contracts::Metrics>,
}

#[installs]
impl Feature for MetricsFeature {
    type Exports = (contracts::Metrics,);

    fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
        let samples = cx.state::<contracts::Metrics>().driven_by(MetricsActor::new);
        samples.emit(Tick);
        Ok(Self { _samples: samples })
    }
}
