use crate::into_signal::IntoSignal;
use guinea_core::actor::invoke_on_ui;
use guinea_core::signal::Signal;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct LoopState {
    interval: Signal<u64>,
    active: Signal<bool>,
    f: Box<dyn FnMut()>,
}

thread_local! {
    static LOOPS: RefCell<HashMap<u64, Rc<RefCell<LoopState>>>> = RefCell::new(HashMap::new());
}

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct LoopHandle {
    id: u64,
    running: Arc<AtomicBool>,
}

impl Drop for LoopHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        LOOPS.with(|loops| {
            loops.borrow_mut().remove(&self.id);
        });
    }
}

pub struct Reactor;

impl Reactor {
    pub fn new() -> Self {
        Self
    }

    pub fn add_loop(
        &self,
        interval: impl IntoSignal<u64>,
        active: impl IntoSignal<bool>,
        f: impl FnMut() + 'static,
    ) -> LoopHandle {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let running = Arc::new(AtomicBool::new(true));

        let state = Rc::new(RefCell::new(LoopState {
            interval: interval.into_signal(),
            active: active.into_signal(),
            f: Box::new(f),
        }));
        let delay = Duration::from_millis(state.borrow().interval.get());
        LOOPS.with(|loops| {
            loops.borrow_mut().insert(id, state);
        });

        schedule_wake(id, delay, running.clone());
        LoopHandle { id, running }
    }

    pub fn add_heartbeat(
        &self,
        interval: impl IntoSignal<u64>,
        f: impl FnMut() + 'static,
    ) -> LoopHandle {
        self.add_loop(interval, Signal::new(true), f)
    }
}

impl Default for Reactor {
    fn default() -> Self {
        Self::new()
    }
}

fn schedule_wake(id: u64, delay: Duration, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if !running.load(Ordering::Relaxed) {
            return;
        }
        invoke_on_ui(move || wake(id, running));
    });
}

fn wake(id: u64, running: Arc<AtomicBool>) {
    if !running.load(Ordering::Relaxed) {
        return;
    }

    let Some(state) = LOOPS.with(|loops| loops.borrow().get(&id).cloned()) else {
        return; // LoopHandle was dropped between scheduling and waking.
    };

    let next_delay = {
        let mut state = state.borrow_mut();
        if state.active.get() {
            (state.f)();
        }
        Duration::from_millis(state.interval.get())
    };

    schedule_wake(id, next_delay, running);
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::actor::event_bus::EventBus;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    /// Real time, not a fake clock - `wait` sleeps past `ms`, then drains
    /// whatever `invoke_on_ui` queued (`test-utils` routes it through
    /// `EventBus`'s task queue instead of a real `UiDispatcher`).
    fn wait(ms: u64) {
        std::thread::sleep(StdDuration::from_millis(ms));
        EventBus::process_queue();
    }

    #[test]
    fn loop_fires_on_interval() {
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let _h = reactor.add_loop(Signal::new(30), Signal::new(true), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn loop_stops_on_handle_drop() {
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_loop(Signal::new(30), Signal::new(true), move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
            wait(60);
            assert_eq!(counter.load(Ordering::SeqCst), 1);
        }

        wait(80);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn loop_respects_active_signal() {
        let reactor = Reactor::new();
        let active = Signal::new(false);
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let _h = reactor.add_loop(Signal::new(30), active.clone(), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        active.set(true, None);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        active.set(false, None);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn loop_drop_before_first_tick() {
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_loop(Signal::new(200), Signal::new(true), move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        wait(250);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn heartbeat_always_fires() {
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let _h = reactor.add_heartbeat(Signal::new(30), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn heartbeat_stops_on_drop() {
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_heartbeat(Signal::new(30), move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
            wait(60);
        }

        wait(80);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
