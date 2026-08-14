use guinea_core::actor::{UiDispatcher, UiTask, set_ui_dispatcher};
use windows_reactor::{UiMarshaller, WinUIDispatcher};

struct ReactorDispatcher(UiMarshaller);

impl UiDispatcher for ReactorDispatcher {
    fn init(&self) {}

    fn dispatch(&self, task: UiTask) {
        self.0.dispatch(task);
    }
}

/// Routes background work back onto the UI thread. Must run on that thread,
/// after its dispatcher queue exists.
pub(crate) fn install() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let dispatcher = WinUIDispatcher::for_current_thread()
            .expect("guinea::App::run must start on a thread with a DispatcherQueue");
        set_ui_dispatcher(ReactorDispatcher(dispatcher.marshaller()));
    });
}
