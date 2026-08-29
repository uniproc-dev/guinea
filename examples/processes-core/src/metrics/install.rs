use guinea::feature::{Feature, FeatureInitContext};
use guinea_core::feature::Bound;

use super::actor::{MetricsActor, Tick};
use super::contracts;

pub struct MetricsFeature {
    _samples: Bound<contracts::Metrics>,
}

impl Feature for MetricsFeature {
    type Params = ();
    type Exports = (contracts::Metrics,);

    fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
        let samples = cx.state::<contracts::Metrics>().driven_by(MetricsActor::new);
        samples.emit(Tick);
        Ok(Self { _samples: samples })
    }
}
