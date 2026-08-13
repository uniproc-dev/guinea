use guinea_core::actor::invoke_on_ui;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct LoopState {
    interval: Box<dyn Fn() -> u64>,
    active: Box<dyn Fn() -> bool>,
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

impl guinea_core::scope::Teardown for LoopHandle {
    fn teardown(self) {
        drop(self);
    }
}

pub struct Reactor;

impl Reactor {
    pub fn new() -> Self {
        Self
    }

    pub fn add_loop(
        &self,
        interval: impl Fn() -> u64 + 'static,
        active: impl Fn() -> bool + 'static,
        f: impl FnMut() + 'static,
    ) -> LoopHandle {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let running = Arc::new(AtomicBool::new(true));

        let state = Rc::new(RefCell::new(LoopState {
            interval: Box::new(interval),
            active: Box::new(active),
            f: Box::new(f),
        }));
        let delay = Duration::from_millis((state.borrow().interval)());
        LOOPS.with(|loops| {
            loops.borrow_mut().insert(id, state);
        });

        schedule_wake(id, delay, running.clone());
        LoopHandle { id, running }
    }

    pub fn add_heartbeat(
        &self,
        interval: impl Fn() -> u64 + 'static,
        f: impl FnMut() + 'static,
    ) -> LoopHandle {
        self.add_loop(interval, || true, f)
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
        if (state.active)() {
            (state.f)();
        }
        Duration::from_millis((state.interval)())
    };

    schedule_wake(id, next_delay, running);
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::actor::event_bus::EventBus;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration as StdDuration;

    /// `invoke_on_ui` (under `test-utils`) queues onto `EventBus`'s single
    /// *global* `TEST_TASK_QUEUE` - there's no per-test isolation. Run two of
    /// these tests in parallel (cargo's default) and one test's `wait()` will
    /// happily drain the *other* test's still-pending timer wakeups via
    /// `process_queue()`, corrupting both tests' counters. A plain `Mutex`
    /// held for each test's duration serializes just this module without
    /// needing a new dependency or touching the shared queue itself.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Real time, not a fake clock - `wait` sleeps past `ms`, then drains
    /// whatever `invoke_on_ui` queued (`test-utils` routes it through
    /// `EventBus`'s task queue instead of a real `UiDispatcher`).
    fn wait(ms: u64) {
        std::thread::sleep(StdDuration::from_millis(ms));
        EventBus::process_queue();
    }

    #[test]
    fn loop_fires_on_interval() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let _h = reactor.add_loop(|| 30, || true, move || {
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
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_loop(|| 30, || true, move || {
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
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let active = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let a = active.clone();
        let _h = reactor.add_loop(|| 30, move || a.load(Ordering::SeqCst), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        active.store(true, Ordering::SeqCst);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        active.store(false, Ordering::SeqCst);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn loop_drop_before_first_tick() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_loop(|| 200, || true, move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        wait(250);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn heartbeat_always_fires() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let _h = reactor.add_heartbeat(|| 30, move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn heartbeat_stops_on_drop() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let reactor = Reactor::new();
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let c = counter.clone();
            let _h = reactor.add_heartbeat(|| 30, move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
            wait(60);
        }

        wait(80);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
