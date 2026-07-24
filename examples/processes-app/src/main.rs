//! Runnable windows-reactor app demonstrating the guinea feature lifecycle
//! end to end, with a nested route: a `TabsLayout` ancestor wrapping two
//! sibling leaves (`Processes`/`Services`). Clicking a tab button (rendered
//! by `TabsLayout` itself, via `NavigateHandle` read through
//! `use_navigate`) navigates between them - `Router::navigate`'s
//! chain-diffing keeps `TabsLayout`'s `Scope` alive across the switch (see
//! its "shell installed Nx" label, which stays at 1x no matter how many
//! times you switch tabs) and tears down only the leaf position. Clicking a
//! process row's Kill button dispatches through the feature's Actions into
//! the actor, whose Port push reduces into the `Scope` and re-renders the
//! list.
//!
//! ```text
//! cargo run                 # from examples/processes-app/
//! ```

mod processes;
mod routes;
mod services;
mod tabs;

use guinea::router::RouterRx;
use windows_reactor::{App, Element, RenderCx};

use routes::Route;

fn root(cx: &mut RenderCx) -> Element {
    RouterRx::render(
        cx,
        Route::Processes {
            context: "ubuntu".to_string(),
        },
    )
}

fn main() -> anyhow::Result<()> {
    App::new()
        .title("guinea · processes")
        .inner_size(420.0, 420.0)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
