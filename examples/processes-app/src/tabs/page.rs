use guinea::feature::FeatureInitContext;
use guinea::l10n::use_l10n;
use guinea::router::{Layout, LayoutCx, UseNavigate};
use guinea::uri::AppUri;
use windows_reactor::{Element, ReactorWindow, button, hstack, text_block, vstack};

use crate::l10n::L10n;
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
        let l10n = use_l10n::<L10n>(cx);

        let is_russian = *l10n.language() == unic_langid::langid!("ru");
        let lang_button_label = if is_russian { "English" } else { "Русский" };

        let tab_bar = hstack((
            button("Processes").on_click(nav.to_handler(Route::Processes {
                context: "ubuntu".to_string(),
            })),
            button("Services").on_click(nav.to_handler(Route::Services {
                context: "ubuntu".to_string(),
            })),

            button("Open window").on_click(|| {
                let _ = ReactorWindow::new()
                    .title("guinea · processes (2)")
                    .inner_size(420.0, 420.0)
                    .render(crate::root);
            }),

            // L10n::load re-renders every open window's use_l10n, not just
            // this one - open a second window and flip the language here.
            button(lang_button_label).on_click(move || {
                let next = if is_russian {
                    unic_langid::langid!("en")
                } else {
                    unic_langid::langid!("ru")
                };
                guinea_core::l10n::L10n::<L10n>::load(L10n::new(next));
            }),
        ))
        .spacing(8.0);

        let toast = tabs
            .last_killed
            .clone()
            .map(|name| l10n.process_killed_toast(name))
            .unwrap_or_default();

        vstack((
            text_block(l10n.app_title()),
            tab_bar,
            text_block(format!("shell installed {}x", tabs.install_count)),
            text_block(format!("kills (this window): {}", tabs.kills_this_window)),
            text_block(format!("kills (all windows): {}", tabs.kills_all_windows)),
            text_block(toast),
            cx.outlet(),
        ))
        .spacing(8.0)
        .into()
    }
}
