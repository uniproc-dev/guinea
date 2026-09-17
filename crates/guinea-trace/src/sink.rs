//! Where records go.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::Trace;

pub type Observer = Rc<dyn Fn(&Trace)>;

thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

static OBSERVED_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Hands every record produced on this thread to `observer`, replacing any
/// observer already set.
pub fn observe(observer: impl Fn(&Trace) + 'static) {
    let before = OBSERVER.with(|slot| slot.borrow_mut().replace(Rc::new(observer)));
    if before.is_none() {
        OBSERVED_THREADS.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn stop_observing() {
    if OBSERVER.with(|slot| slot.borrow_mut().take()).is_some() {
        OBSERVED_THREADS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Whether devtools are watching this thread.
pub fn is_observed() -> bool {
    OBSERVER.with(|slot| slot.borrow().is_some())
}

/// Whether devtools are watching any thread.
pub fn is_observed_anywhere() -> bool {
    OBSERVED_THREADS.load(Ordering::Relaxed) > 0
}

fn observer() -> Option<Observer> {
    OBSERVER.with(|slot| slot.borrow().clone())
}

pub(crate) fn wanted() -> bool {
    OBSERVER.with(|slot| slot.borrow().is_some())
        || tracing::enabled!(target: "guinea", Level::DEBUG)
}

pub(crate) fn emit(trace: Trace) {
    match &trace {
        Trace::Begin(record) | Trace::Mark(record) => match record.parent {
            Some(parent) => tracing::debug!(
                target: "guinea",
                id = record.id.get(),
                parent = parent.get(),
                kind = record.point.kind(),
                "{} ← {} {}",
                record.id,
                parent,
                record.point
            ),
            None => tracing::debug!(
                target: "guinea",
                id = record.id.get(),
                kind = record.point.kind(),
                "{} {}",
                record.id,
                record.point
            ),
        },
        Trace::End { id, took } => tracing::trace!(
            target: "guinea",
            id = id.get(),
            took_us = took.as_micros() as u64,
            "{id} done in {took:?}"
        ),
    }

    if let Some(observer) = observer() {
        observer(&trace);
    }
}

/// A plain subscriber: one line per event, no span nesting, writing to
/// `writer` and filtered by `targets`.
pub fn init_subscriber<W>(writer: W, targets: Targets) -> anyhow::Result<()>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(targets)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true)
                .compact(),
        )
        .try_init()
        .map_err(|error| anyhow::anyhow!("installing the tracing subscriber: {error}"))
}
