//! What devtools read that is not a trace: panels a backend or a plugin
//! offers, for a root or for the whole application.
//!
//! What happened is in [`crate::trace`]; [`mark_anywhere`] is how something
//! off the UI thread adds to it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::trace::{self, Point};

pub use crate::trace::is_observed;

/// Something a backend knows about a window that the rest of guinea does not:
/// its component tree, say. Devtools draw it without interpreting it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panel {
    pub id: &'static str,
    pub title: &'static str,
    pub nodes: Vec<PanelNode>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelNode {
    pub label: String,
    pub kind: String,
    pub properties: Vec<(String, String)>,
    pub children: Vec<PanelNode>,
}

type Provider = Rc<dyn Fn() -> Option<Panel>>;

thread_local! {
    static PANELS: RefCell<Vec<(u64, Option<u64>, Provider)>> = const { RefCell::new(Vec::new()) };
    static NEXT_PANEL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn offer(root: Option<u64>, provider: impl Fn() -> Option<Panel> + 'static) -> PanelGuard {
    let id = NEXT_PANEL.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    PANELS.with(|panels| panels.borrow_mut().push((id, root, Rc::new(provider))));
    PanelGuard { id }
}

fn built(root: Option<u64>) -> Vec<Panel> {
    let providers: Vec<Provider> = PANELS.with(|panels| {
        panels
            .borrow()
            .iter()
            .filter(|(_, owner, _)| *owner == root)
            .map(|(_, _, provider)| provider.clone())
            .collect()
    });
    providers.iter().filter_map(|provider| provider()).collect()
}

/// Offers a panel for the root numbered `root`, for as long as the guard
/// lives. `provider` is asked only while devtools are reading.
#[must_use = "the panel is withdrawn when the guard is dropped"]
pub fn contribute(root: u64, provider: impl Fn() -> Option<Panel> + 'static) -> PanelGuard {
    offer(Some(root), provider)
}

/// Offers a panel about the whole application rather than one window: what a
/// plugin holds, say.
#[must_use = "the panel is withdrawn when the guard is dropped"]
pub fn contribute_to_app(provider: impl Fn() -> Option<Panel> + 'static) -> PanelGuard {
    offer(None, provider)
}

/// Every panel offered for `root`, built now.
pub fn panels(root: u64) -> Vec<Panel> {
    built(Some(root))
}

/// Every panel offered for the application, built now.
pub fn app_panels() -> Vec<Panel> {
    built(None)
}

pub struct PanelGuard {
    id: u64,
}

impl Drop for PanelGuard {
    fn drop(&mut self) {
        let id = self.id;
        let _ = PANELS.try_with(|panels| panels.borrow_mut().retain(|(own, _, _)| *own != id));
    }
}

/// Marks `point` under what is running now, from any thread.
///
/// On the thread devtools watch, it is marked at once. Anywhere else it is
/// marked there when the UI thread next runs, under the cause that was current
/// here, so a write from background work still leads back to what started it.
pub fn mark_anywhere(point: impl FnOnce() -> Point + Send + 'static) {
    if trace::is_observed() || !trace::is_observed_anywhere() {
        trace::mark(point);
        return;
    }
    let cause = trace::current();
    let marked = crate::actor::try_invoke_on_ui(move || {
        trace::mark_under(cause, point);
    });
    if let Err(unsent) = marked {
        unsent();
    }
}

/// A `tracing` layer that puts the application's own events into the trace,
/// under whatever caused them, while devtools watch.
///
/// ```no_run
/// use tracing_subscriber::prelude::*;
///
/// tracing_subscriber::registry()
///     .with(tracing_subscriber::fmt::layer())
///     .with(guinea_core::devtools::layer())
///     .init();
/// ```
pub fn layer() -> LogLayer {
    LogLayer
}

pub struct LogLayer;

thread_local! {
    static LOGGING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        if meta.target() == "guinea" || !trace::is_observed_anywhere() || LOGGING.get() {
            return;
        }
        LOGGING.set(true);
        let mut text = Text::default();
        event.record(&mut text);
        let (level, target) = (*meta.level(), meta.target());
        mark_anywhere(move || Point::Log {
            level,
            target,
            text: text.finish(),
        });
        LOGGING.set(false);
    }
}

#[derive(Default)]
struct Text {
    message: String,
    fields: Vec<String>,
}

impl Text {
    fn finish(self) -> String {
        let mut text = self.message;
        for field in self.fields {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&field);
        }
        text
    }
}

impl tracing::field::Visit for Text {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_panel() -> Option<Panel> {
        Some(Panel {
            id: "test",
            title: "Test",
            nodes: Vec::new(),
        })
    }

    #[test]
    fn a_panel_is_offered_for_its_root_until_the_guard_goes() {
        let guard = contribute(7, test_panel);
        assert_eq!(panels(7).len(), 1);
        assert!(panels(8).is_empty());
        assert!(app_panels().is_empty());

        drop(guard);
        assert!(panels(7).is_empty());
    }

    #[cfg(not(feature = "test-utils"))]
    struct Queue(std::sync::mpsc::Sender<crate::actor::UiTask>);

    #[cfg(not(feature = "test-utils"))]
    impl crate::actor::UiDispatcher for Queue {
        fn init(&self) {}

        fn dispatch(&self, task: crate::actor::UiTask) {
            let _ = self.0.send(task);
        }
    }

    #[test]
    #[cfg(not(feature = "test-utils"))]
    fn a_mark_from_another_thread_lands_on_the_watched_one_under_its_cause() {
        let (tx, rx) = std::sync::mpsc::channel();
        crate::actor::set_ui_dispatcher(Queue(tx));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        trace::observe(move |record| {
            if let trace::Trace::Mark(record) = record {
                sink.borrow_mut().push((record.parent, record.point.kind()));
            }
        });

        let action = trace::mark(|| Point::Action { message: "Save" });
        std::thread::spawn(move || {
            let _resumed = trace::resume(Some(action));
            mark_anywhere(|| Point::Note("written".into()));
        })
        .join()
        .expect("writer");
        for task in rx.try_iter() {
            task();
        }
        trace::stop_observing();

        assert_eq!(
            *seen.borrow(),
            [(None, "action"), (Some(action), "note")],
            "the write is marked here, under the action that caused it"
        );
    }

    #[test]
    fn an_ordinary_event_is_traced_under_what_caused_it() {
        use tracing_subscriber::layer::SubscriberExt;

        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        trace::observe(move |record| {
            if let trace::Trace::Mark(record) = record
                && let Point::Log { text, .. } = &record.point
            {
                sink.borrow_mut().push((record.parent, text.clone()));
            }
        });

        let subscriber = tracing_subscriber::registry().with(layer());
        let action = trace::mark(|| Point::Action { message: "Kill" });
        tracing::subscriber::with_default(subscriber, || {
            let _resumed = trace::resume(Some(action));
            tracing::info!(pid = 42, "process killed");
            tracing::debug!(target: "guinea", "a guinea point, logged: not traced twice");
        });
        trace::stop_observing();

        assert_eq!(
            *seen.borrow(),
            [(Some(action), "process killed pid=42".to_string())]
        );
    }

    #[test]
    fn an_application_panel_belongs_to_no_root() {
        let guard = contribute_to_app(test_panel);
        assert_eq!(app_panels().len(), 1);
        assert!(panels(0).is_empty());

        drop(guard);
        assert!(app_panels().is_empty());
    }
}
