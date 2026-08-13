mod events;
mod l10n;
mod metrics;
mod processes;
mod routes;
mod services;
mod startup;
mod tabs;

use routes::Route;

fn initial_route() -> Route {
    Route::Processes {
        context: "ubuntu".to_string(),
    }
}

pub(crate) fn root(cx: &mut windows_reactor::RenderCx) -> windows_reactor::Element {
    guinea::router::RouterRx::<Route>::render(cx, initial_route())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,guinea=debug,guinea_processes_app_example=debug".into()),
        )
        .init();

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    amethystate::init_global(
        amethystate::StoreBuilder::for_app("guinea-processes-app-example", "settings")
            .expect("resolve app config dir"),
    );

    guinea_core::l10n::L10n::<l10n::L10n>::load(l10n::L10n::new(unic_langid::langid!("en")));

    guinea::app::App::new()
        .feature(startup::Startup)
        .on_route_change(|from, to| tracing::debug!(?from, to, "route"))
        .run(
            windows_reactor::App::new()
                .title("guinea · processes")
                .inner_size(420.0, 420.0),
            guinea::router::RouterRoot::at(initial_route()),
        )
}
