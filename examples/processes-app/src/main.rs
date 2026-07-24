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
    App::new()
        .title("guinea · processes")
        .inner_size(420.0, 420.0)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
