//! The route tree, Dioxus-style: `#[layout(TabsLayout)]` names the shared
//! ancestor both leaves render under; the variant's own identifier names its
//! leaf `Page` type (`Processes`/`Services`, from `page.rs`).

use std::borrow::Cow;

use guinea::router::ToUri;
use guinea::uri::AppUri;
use guinea_macros::Routable;

use crate::processes::Processes;
use crate::services::Services;
use crate::tabs::TabsLayout;

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(TabsLayout)]
    #[route("/:context/processes")]
    Processes { context: String },

    #[route("/:context/services")]
    #[end_layout]
    Services { context: String },
}

impl ToUri for Route {
    fn to_uri(&self) -> AppUri {
        match self {
            Route::Processes { context } => AppUri::new(context.clone(), Cow::Borrowed("processes"), vec![]),
            Route::Services { context } => AppUri::new(context.clone(), Cow::Borrowed("services"), vec![]),
        }
    }
}
