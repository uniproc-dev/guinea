use guinea_macros::routes;

use crate::layouts::tabs::TabsLayout;
use crate::pages::metrics::Metrics;
use crate::pages::processes::Processes;
use crate::pages::services::Services;

// The same tree as the other two front ends. The backend is named because the
// plugins pull guinea in with its default features, which turns winui on too -
// so this build has two backends whether the application wants them or not.
routes! {
    backend = guinea::slint::Slint,
    Route {
        layout(TabsLayout) {
            page(Processes, "/:context/processes") { context: String }
            page(Services, "/:context/services") { context: String }
            page(Metrics, "/:context/metrics") { context: String }
        }
    }
}
