use std::rc::Rc;

use guinea::feature::FeatureInitContext;
use guinea::slint::{Layout, LayoutCx, ToSlint};
use guinea::uri::AppUri;
use guinea_plugin_l10n::Localization;
use slint::ComponentHandle;

use processes_core::l10n::L10n;
use processes_core::tabs::contracts::{TabsReducer, TabsViewState};

use crate::routes::Route;
use crate::ui::{AppWindow, TabsModel};

pub struct TabsLayout;

impl Layout for TabsLayout {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::tabs::install::install(ctx, uri)
    }

    fn bind(cx: LayoutCx) {
        let root = cx.root::<AppWindow>();
        let model = root.global::<TabsModel>();

        let binding = cx.binding::<TabsReducer>();
        let refresh: Rc<dyn Fn()> = {
            let root = root.clone_strong();
            let binding = binding.clone();
            Rc::new(move || {
                let model = root.global::<TabsModel>();
                let strings = L10n::current();
                model.set_app_title(strings.app_title().to_slint());
                model.set_language_label(language_label(&strings).to_slint());
                model.set_status(status_line(&strings, &binding.peek()).to_slint());
            })
        };

        cx.bind::<TabsReducer>({
            let refresh = refresh.clone();
            move |_| refresh()
        });

        // The language is process-wide, not a reducer of this scope, so its
        // subscription has no scope of its own to die with - `own` gives it
        // this layout's.
        cx.own(guinea_plugin_l10n::L10n::<L10n>::subscribe({
            let refresh = refresh.clone();
            move |_| refresh()
        }));

        let nav = cx.navigate::<Route>();
        model.on_go(move |index| {
            let context = "ubuntu".to_string();
            match index {
                0 => nav.to(Route::Processes { context }),
                1 => nav.to(Route::Services { context }),
                _ => nav.to(Route::Metrics { context }),
            }
        });

        model.on_toggle_language(|| {
            let next = if L10n::current().tag() == "ru" {
                "en"
            } else {
                "ru"
            };
            if let Some(strings) = L10n::for_tag(next) {
                guinea_plugin_l10n::L10n::<L10n>::load(strings);
            }
        });
    }
}

fn language_label(strings: &L10n) -> &'static str {
    if strings.tag() == "ru" {
        "English"
    } else {
        "Русский"
    }
}

fn status_line(strings: &L10n, state: &TabsViewState) -> String {
    let killed = state
        .last_killed
        .as_deref()
        .map(|name| format!(" · {}", strings.process_killed_toast(name.to_string())))
        .unwrap_or_default();

    format!(
        "kills here {} · everywhere {}{killed}",
        state.kills_this_window, state.kills_all_windows,
    )
}
