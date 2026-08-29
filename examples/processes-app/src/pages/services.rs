use guinea::feature::FeatureInitContext;
use guinea::winui::{Page, PageCx};
use windows_reactor::{Element, text_block, title, vstack};

use processes_core::services::contracts::Services as Running;

pub struct Services;

impl Page for Services {
    type Params = crate::routes::ServicesParams;

    type Installs = processes_core::services::ServicesFeature;

    fn install(
        ctx: &FeatureInitContext,
        _params: &Self::Params,
    ) -> anyhow::Result<Self::Installs> {
        ctx.install(&())
    }

    fn view(cx: &mut PageCx<Self>) -> Element {
        let (state, _dispatch) = cx.use_reducer::<Running, _>();

        let rows: Vec<Element> = state
            .items
            .iter()
            .map(|row| text_block(row.clone()).into())
            .collect();

        vstack((title("Services"), vstack(rows).spacing(6.0)))
            .spacing(16.0)
            .into()
    }
}
