use guinea::eframe::{Page, PageCx};
use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;

use processes_core::services::contracts::ServicesReducer;

pub struct Services;

impl Page for Services {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::services::install::install(ctx, uri)
    }

    fn render(cx: &mut PageCx<'_>) {
        let (state, _) = cx.read::<ServicesReducer>();

        egui::ScrollArea::vertical().show(cx.ui(), |ui| {
            for item in &state.items {
                ui.label(item);
            }
        });
    }
}
