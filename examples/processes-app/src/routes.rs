use guinea_macros::routes;

use crate::processes::Processes;
use crate::services::Services;
use crate::tabs::TabsLayout;

routes! {
    Route {
        layout(TabsLayout) {
            page(Processes, "/:context/processes") { context: String }
            page(Services, "/:context/services") { context: String }
        }
    }
}
