use guinea::feature::FeatureInitContext;
use guinea::winui::{Page, PageCx};
use guinea_widgets::chart::{HoverInfo, Interpolation, Series, line_chart};
use guinea_widgets::color::{hex, hex_alpha};
use windows_canvas::ColorF;
use windows_reactor::{Element, LayoutExt, text_block, title, vstack};

use processes_core::metrics::contracts::Metrics as Sampling;

pub struct Metrics;

const CPU_COLOR: ColorF = hex(0x3a82f5);
const MEMORY_COLOR: ColorF = hex(0xf59e0a);

impl Page for Metrics {
    type Params = crate::routes::MetricsParams;

    type Installs = processes_core::metrics::MetricsFeature;

    fn install(
        ctx: &FeatureInitContext,
        _params: &Self::Params,
    ) -> anyhow::Result<Self::Installs> {
        ctx.install(&())
    }

    fn view(cx: &mut PageCx<Self>) -> Element {
        let (state, _) = cx.use_reducer::<Sampling, _>();
        // State, not a ref: writing a ref does not schedule a render, so the
        // readout would only refresh when something else happened to redraw
        // the page.
        let (hover, set_hover) = cx.use_state(None::<HoverInfo>);

        let series = vec![
            Series {
                color: CPU_COLOR,
                interpolation: Interpolation::Smooth,
                fill: Some(hex_alpha(0x3a82f5, 38)),
                points: state.cpu.into_points(),
            },
            Series {
                color: MEMORY_COLOR,
                interpolation: Interpolation::Linear,
                fill: None,
                points: state.memory.into_points(),
            },
        ];

        let set_hover_from_chart = set_hover.clone();
        // The chart hands back an erased `Element`, and size is a capability
        // of the widget carrying it - so the size goes on the wrapper.
        let chart = windows_reactor::border(line_chart(cx, series, move |info| {
            set_hover_from_chart.call(info);
        }))
        .width(380.0)
        .height(220.0);

        let tooltip = match hover.as_ref() {
            Some(h) => format!("CPU {:.0}% · RAM {:.0}%", h.values[0], h.values.get(1).copied().unwrap_or(0.0)),
            None => "hover the chart for values".to_string(),
        };

        vstack((title("Metrics"), text_block(tooltip), chart)).spacing(12.0).into()
    }
}
