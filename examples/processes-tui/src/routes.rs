use guinea_macros::routes;

use crate::pages::metrics::Metrics;
use crate::pages::processes::Processes;
use crate::pages::services::Services;
use crate::layouts::tabs::TabsLayout;

// The same tree as the WinUI front end, aimed at the other backend. The pages
// differ; where they sit and what installs with them does not.
routes! {
    backend = guinea::ratatui::Tui,
    Route {
        layout(TabsLayout) {
            page(Processes, "/:context/processes") { context: String }
            page(Services, "/:context/services") { context: String }
            page(Metrics, "/:context/metrics") { context: String }
        }
    }
}
