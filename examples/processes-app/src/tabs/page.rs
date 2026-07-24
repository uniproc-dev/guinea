use guinea::feature::FeatureInitContext;
use guinea::router::{Layout, LayoutCx, UseNavigate};
use guinea::uri::AppUri;
use windows_reactor::{Element, button, hstack, text_block, vstack};

use crate::routes::Route;

use super::contracts::TabsReducer;

pub struct TabsLayout;

impl Layout for TabsLayout {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        super::install::install(ctx, uri)
    }

    fn view(cx: &mut LayoutCx) -> Element {
        let (tabs, _) = cx.use_reducer::<TabsReducer>();
        let nav = cx.use_navigate::<Route>();

        let tab_bar = hstack((
            button("Processes").on_click(nav.to_handler(Route::Processes {
                context: "ubuntu".to_string(),
            })),
            button("Services").on_click(nav.to_handler(Route::Services {
                context: "ubuntu".to_string(),
            })),
        ))
        .spacing(8.0);

        vstack((
            tab_bar,
            text_block(format!("shell installed {}x", tabs.install_count)),
            cx.outlet(),
        ))
        .spacing(8.0)
        .into()
    }
}
