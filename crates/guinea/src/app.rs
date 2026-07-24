use crate::feature::{AppFeature, AppFeatureDeinitContext, AppFeatureInitContext, IntoAppFeature};
use crate::lifecycle_tracker::AppLifecycle;
use crate::reactor::Reactor;
use guinea_core::SharedState;
use guinea_core::actor::{UiDispatcher, UiThreadToken};
use guinea_core::trace::in_named_scope;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

struct SlintDispatcher;

impl UiDispatcher for SlintDispatcher {
    fn init(&self) {
        guinea_core::actor::set_ui_dispatcher(SlintDispatcher);
    }

    fn dispatch(&self, task: guinea_core::actor::UiTask) {
        let _ = slint::invoke_from_event_loop(task);
    }
}

pub trait UiContext {
    fn new_token(&self) -> UiThreadToken;
}

impl<TWindow: ComponentHandle + 'static> UiContext for TWindow {
    fn new_token(&self) -> UiThreadToken {
        UiThreadToken::dangerously_create_token_unchecked()
    }
}

pub trait Window: ComponentHandle + UiContext + 'static {}
impl<TWindow: ComponentHandle + UiContext + 'static> Window for TWindow {}

pub struct App<TWindow> {
    ui: TWindow,
    shared: SharedState,
    runtime: tokio::runtime::Runtime,
    inner: Rc<RefCell<AppInner>>,
}

struct AppInner {
    reactor: Reactor,
    app_features: Vec<Box<dyn AppFeature>>,
    root_tracker: AppLifecycle,
}

impl<TWindow: Window> App<TWindow> {
    pub fn new(ui: TWindow) -> anyhow::Result<Self> {
        Self::with_dispatcher(ui, SlintDispatcher)
    }

    pub fn with_dispatcher(
        ui: TWindow,
        dispatcher: impl UiDispatcher + 'static,
    ) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;
        let _guard = runtime.enter();

        dispatcher.init();

        let shared = SharedState::new();

        Ok(Self {
            ui,
            shared,
            runtime,
            inner: Rc::new(RefCell::new(AppInner {
                reactor: Reactor::new(),
                app_features: Vec::new(),
                root_tracker: AppLifecycle::new(),
            })),
        })
    }

    pub fn app_feature<I: IntoAppFeature + 'static>(
        self,
        mut into_feature: I,
    ) -> anyhow::Result<Self> {
        let _guard = self.runtime.enter();

        let full_name = std::any::type_name::<I>();
        let clean_name = full_name
            .split('<')
            .next()
            .unwrap_or(full_name)
            .split("::")
            .last()
            .unwrap_or("Unknown");

        in_named_scope(
            "core.app.feature_install",
            Some("feature,status,level"),
            Some(format!("{}|ok|app", clean_name)),
            || {
                let mut inner = self.inner.borrow_mut();
                let mut init_ctx = AppFeatureInitContext {
                    token: self.ui.new_token(),
                    reactor: &inner.reactor,
                    shared: &self.shared,
                    tracker: &inner.root_tracker,
                };

                let mut feature = into_feature.into_feature();

                match feature.install(&mut init_ctx) {
                    Ok(_) => {
                        tracing::info!(
                            feature = clean_name,
                            status = "ok",
                            level = "app",
                            "feature.install"
                        );
                        inner.app_features.push(Box::new(feature));
                        drop(inner);
                        Ok(self)
                    }
                    Err(e) => {
                        tracing::error!(
                            feature = clean_name,
                            status = "error",
                            level = "app",
                            error = %e,
                            "feature.install"
                        );
                        Err(e)
                    }
                }
            },
        )
    }

    pub fn ui(&self) -> &TWindow {
        &self.ui
    }

    pub fn shared(&self) -> &SharedState {
        &self.shared
    }

    pub fn run(self) -> anyhow::Result<()> {
        let _guard = self.runtime.enter();

        let result = self.ui.run();

        tracing::info!("Application shutting down, executing app feature cleanups...");

        let inner = self.inner.borrow();
        let token = self.ui.new_token();

        let mut deinit_ctx = AppFeatureDeinitContext {
            token: token.clone(),
            reactor: &inner.reactor,
            shared: &self.shared,
        };

        inner.root_tracker.clone().shutdown(&token, &mut deinit_ctx);

        drop(inner);

        result.map_err(|e| anyhow::anyhow!("UI execution error: {}", e))
    }
}
