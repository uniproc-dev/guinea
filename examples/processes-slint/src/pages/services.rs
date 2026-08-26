use guinea::feature::FeatureInitContext;
use guinea::slint::{Page, PageCx};
use guinea::uri::AppUri;
use slint::ComponentHandle;

use processes_core::services::contracts::ServicesReducer;

use crate::ui::{AppWindow, ServicesModel};

pub struct Services;

impl Page for Services {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::services::install::install(ctx, uri)
    }

    fn bind(cx: PageCx) {
        cx.root::<AppWindow>()
            .global::<ServicesModel>()
            .set_items(cx.rows::<ServicesReducer, _>(|state| &state.items));
    }
}
