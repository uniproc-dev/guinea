use guinea::feature::FeatureInitContext;
use guinea::slint::{Page, PageCx};
use guinea::uri::AppUri;
use slint::ComponentHandle;

use processes_core::processes::contracts::{Kill, ProcessesReducer};
use processes_core::processes::pid_at;

use crate::ui::{AppWindow, ProcessesModel};

pub struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::processes::install::install(ctx, uri)
    }

    fn bind(cx: PageCx) {
        let root = cx.root::<AppWindow>();
        let model = root.global::<ProcessesModel>();

        // Set once: the model reads the reducer's state and converts the rows
        // Slint actually asks for, so a refresh costs nothing here.
        model.set_items(cx.rows::<ProcessesReducer, _>(|state| &state.items));

        // Read at click time rather than captured: the actor refreshes the
        // list, and the row under the button may not be the row that was there
        // when the page was installed.
        let binding = cx.binding::<ProcessesReducer>();
        model.on_kill(move |index| {
            let pid = pid_at(&binding.peek().items, index as usize);
            if let Some(pid) = pid {
                binding.actions().emit(Kill(pid));
            }
        });
    }
}
