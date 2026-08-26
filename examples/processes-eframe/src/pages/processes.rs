use guinea::eframe::{Page, PageCx};
use guinea::feature::FeatureInitContext;
use guinea::uri::AppUri;

use processes_core::processes::contracts::{Kill, ProcessesReducer};
use processes_core::processes::pid_at;

pub struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::processes::install::install(ctx, uri)
    }

    fn render(cx: &mut PageCx<'_>) {
        let (state, actions) = cx.read::<ProcessesReducer>();

        egui::ScrollArea::vertical().show(cx.ui(), |ui| {
            for (index, item) in state.items.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(item);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Read at click time rather than captured: the actor
                        // refreshes the list, and this row may not be the row
                        // it was when the frame started.
                        if ui.button("Kill").clicked()
                            && let Some(pid) = pid_at(&state.items, index)
                        {
                            actions.emit(Kill(pid));
                        }
                    });
                });
            }
        });
    }
}
