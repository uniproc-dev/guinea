use guinea_macros::routes;

use crate::pages::metrics::Metrics;
use crate::pages::processes::Processes;
use crate::pages::services::Services;
use crate::pages::tabs::TabsLayout;

routes! {
    Route {
        layout(TabsLayout) {
            page(Processes, "/:context/processes") { context: String }
            page(Services, "/:context/services") { context: String }
            page(Metrics, "/:context/metrics") { context: String }
        }
    }
}
