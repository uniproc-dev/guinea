use guinea_macros::routes;

use crate::layouts::tabs::TabsLayout;
use crate::pages::draft::Draft;
use crate::pages::login::Login;
use crate::pages::metrics::Metrics;
use crate::pages::processes::Processes;
use crate::pages::services::Services;

// The same tree as the other four front ends. The backend is named because the
// plugins pull guinea in with its default features, which turns winui on too -
// so this build has two backends whether the application wants them or not.
routes! {
    backend = guinea::iced::Iced,
    Route {
        layout(TabsLayout) {
            page(Processes) { context: String }
            page(Services) { context: String }
            page(Metrics) link("/:context/metrics") { context: String }
            page(Login) { context: String }
            page(Draft) { context: String }
        }
    }
}
