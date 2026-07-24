use guinea::feature::FeatureInitContext;
use guinea::router::{Page, PageCx};
use guinea::uri::AppUri;
use windows_reactor::{Element, text_block, title, vstack};

use super::contracts::ServicesReducer;

pub struct Services;

impl Page for Services {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        super::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx) -> Element {
        let (state, _dispatch) = cx.use_reducer::<ServicesReducer>();

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
