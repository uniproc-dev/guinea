mod events;
mod l10n;
mod processes;
mod routes;
mod services;
mod tabs;

use guinea::router::RouterRx;
use windows_reactor::{App, Element, RenderCx};

use routes::Route;

pub(crate) fn root(cx: &mut RenderCx) -> Element {
    RouterRx::render(
        cx,
        Route::Processes {
            context: "ubuntu".to_string(),
        },
    )
}

fn main() -> anyhow::Result<()> {
    // Once at startup - and again with a different `LanguageIdentifier` on
    // every locale switch. Every open window's `use_l10n` re-renders on
    // either.
    guinea_core::l10n::L10n::<l10n::L10n>::load(l10n::L10n::new(unic_langid::langid!("en")));

    App::new()
        .title("guinea · processes")
        .inner_size(420.0, 420.0)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
