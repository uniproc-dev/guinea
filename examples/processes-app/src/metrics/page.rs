use guinea::feature::FeatureInitContext;
use guinea::router::{Page, PageCx};
use guinea::uri::AppUri;
use guinea::widgets::chart::{HoverInfo, Interpolation, Series, line_chart};
use guinea::widgets::color::{hex, hex_alpha};
use windows_canvas::ColorF;
use windows_reactor::{Element, ElementExt, text_block, title, vstack};

use super::contracts::MetricsReducer;

pub struct Metrics;

const CPU_COLOR: ColorF = hex(0x3a82f5);
const MEMORY_COLOR: ColorF = hex(0xf59e0a);

impl Page for Metrics {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        super::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx) -> Element {
        let (state, _) = cx.use_reducer::<MetricsReducer>();
        let hover_ref = cx.use_ref(None::<HoverInfo>);

        let series = vec![
            Series {
                color: CPU_COLOR,
                interpolation: Interpolation::Smooth,
                fill: Some(hex_alpha(0x3a82f5, 38)),
                points: state.cpu.as_points(),
            },
            Series {
                color: MEMORY_COLOR,
                interpolation: Interpolation::Linear,
                fill: None,
                points: state.memory.as_points(),
            },
        ];

        let hover_for_cb = hover_ref.clone();
        let chart = line_chart(cx, series, move |info| {
            *hover_for_cb.borrow_mut() = info;
        })
        .width(380.0)
        .height(220.0);

        let tooltip = match hover_ref.borrow().as_ref() {
            Some(h) => format!("CPU {:.0}% · RAM {:.0}%", h.values[0], h.values.get(1).copied().unwrap_or(0.0)),
            None => "hover the chart for values".to_string(),
        };

        vstack((title("Metrics"), text_block(tooltip), chart)).spacing(12.0).into()
    }
}
