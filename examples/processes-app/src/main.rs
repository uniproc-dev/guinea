mod events;
mod l10n;
mod metrics;
mod processes;
mod routes;
mod services;
mod tabs;

use guinea::router::RouterRx;
use guinea_core::actor::{UiDispatcher, UiTask, set_ui_dispatcher};
use windows_reactor::{App, Element, RenderCx, UiMarshaller, WinUIDispatcher};

use routes::Route;

/// Bridges guinea-core's actor-facing `UiDispatcher` (used by `ctx.spawn_bg`
/// et al. to hop background work back onto the UI thread) onto
/// `windows-reactor`'s own `WinUIDispatcher`/`UiMarshaller` - no raw
/// `DispatcherQueue`/COM handling needed, windows-reactor already wraps it.
struct ReactorDispatcher(UiMarshaller);

impl UiDispatcher for ReactorDispatcher {
    fn init(&self) {}

    fn dispatch(&self, task: UiTask) {
        self.0.dispatch(task);
    }
}

/// `root` runs on the UI thread as part of `App::render`'s own startup, by
/// which point a `DispatcherQueue` already exists for this thread (windows-
/// reactor's own `host.rs` relies on the same assumption) - but `root` can
/// run many times, so this only actually registers once.
fn ensure_ui_dispatcher() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let dispatcher = WinUIDispatcher::for_current_thread()
            .expect("root() must run on the UI thread, which should already have a DispatcherQueue");
        set_ui_dispatcher(ReactorDispatcher(dispatcher.marshaller()));
    });
}

pub(crate) fn root(cx: &mut RenderCx) -> Element {
    ensure_ui_dispatcher();

    RouterRx::render(
        cx,
        Route::Processes {
            context: "ubuntu".to_string(),
        },
        amethystate::global_store(),
    )
}

fn main() -> anyhow::Result<()> {

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    amethystate::init_global(
        amethystate::StoreBuilder::for_app("guinea-processes-app-example", "settings")
            .expect("resolve app config dir"),
    );

    guinea_core::l10n::L10n::<l10n::L10n>::load(l10n::L10n::new(unic_langid::langid!("en")));

    App::new()
        .title("guinea · processes")
        .inner_size(420.0, 420.0)
        .render(root)
        .map_err(|e| anyhow::anyhow!("windows-reactor app failed: {e:?}"))
}
