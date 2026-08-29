use guinea::feature::FeatureInitContext;
use guinea_plugin_l10n::{Localization, ui::use_l10n};
use guinea::winui::{UseNavigate, UseRouteChange};
use guinea::winui::{Layout, LayoutCx};
use windows_reactor::{Element, ReactorWindow, button, hstack, text_block, vstack};

use processes_core::l10n::L10n;
use crate::routes::Route;

use processes_core::tabs::contracts::Tabs;

pub struct TabsLayout;

impl Layout for TabsLayout {
    type Params = crate::routes::TabsLayoutParams;

    type Installs = processes_core::tabs::TabsFeature;

    fn install(ctx: &FeatureInitContext, params: &Self::Params) -> anyhow::Result<Self::Installs> {
        ctx.install(params.context.as_str())
    }

    fn view(cx: &mut LayoutCx<Self>) -> Element {
        let (tabs, _) = cx.use_reducer::<Tabs, _>();
        cx.use_route_change(|from, to| tracing::debug!(?from, to, "route"));

        let nav = cx.use_navigate::<Route>();
        let l10n = use_l10n::<L10n>(cx);

        let is_russian = l10n.tag() == "ru";
        let lang_button_label = if is_russian { "English" } else { "Русский" };

        // What this layout was reached with, not one invented here: `routes!`
        // derived it from the pages below.
        let context = || tabs.context.clone();

        let tab_bar = hstack((
            button("Processes").on_click(nav.to_handler(Route::Processes {
                context: context(),
            })),
            button("Services").on_click(nav.to_handler(Route::Services {
                context: context(),
            })),
            button("Metrics").on_click(nav.to_handler(Route::Metrics {
                context: context(),
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
                let next = if is_russian { "en" } else { "ru" };
                if let Some(strings) = L10n::for_tag(next) {
                    guinea_plugin_l10n::L10n::<L10n>::load(strings);
                }
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
