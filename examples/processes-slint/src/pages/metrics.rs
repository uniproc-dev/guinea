use guinea::feature::FeatureInitContext;
use guinea::slint::{Page, PageCx, ToSlint};
use guinea::uri::AppUri;
use guinea_widgets::chart::RingSeries;
use slint::ComponentHandle;

use processes_core::metrics::contracts::MetricsReducer;

use crate::ui::{AppWindow, MetricsModel};

pub struct Metrics;

impl Page for Metrics {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::metrics::install::install(ctx, uri)
    }

    fn bind(cx: PageCx) {
        let root = cx.root::<AppWindow>();

        cx.bind_to::<MetricsReducer, _>(&root, |root, state| {
            let model = root.global::<MetricsModel>();
            let history = values(&state.cpu);

            model.set_cpu(latest(&history));
            model.set_memory(latest(&values(&state.memory)));
            model.set_cpu_history(history.to_slint());
        });
    }
}

fn values(series: &RingSeries) -> Vec<f32> {
    let (older, newer) = series.as_slices();
    older.iter().chain(newer).map(|(_, value)| *value).collect()
}

fn latest(values: &[f32]) -> f32 {
    values.last().copied().unwrap_or(0.0)
}
