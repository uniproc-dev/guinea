//! Timers, owned by the context that set them up.
//!
//! An application's timers live as long as the application; a feature's, as
//! long as its scope. There is no timer without an owner: dropping what the
//! context holds stops it. Each remembers where it was set up, which is how
//! devtools tell one from another, and ticks on the UI thread.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::panic::Location;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use guinea_core::actor::invoke_on_ui;

/// How long between ticks.
pub enum Period {
    Fixed(Duration),
    /// Asked again before every tick: for a period the user can change.
    Varying(Box<dyn Fn() -> Duration>),
}

impl Period {
    pub fn varying(period: impl Fn() -> Duration + 'static) -> Self {
        Period::Varying(Box::new(period))
    }

    fn next(&self) -> Duration {
        match self {
            Period::Fixed(period) => *period,
            Period::Varying(period) => period(),
        }
    }
}

impl From<Duration> for Period {
    fn from(period: Duration) -> Self {
        Period::Fixed(period)
    }
}

/// What is known about a running timer.
#[derive(Clone, Debug)]
pub struct TimerInfo {
    pub id: u64,
    pub name: Option<&'static str>,
    /// Where it was set up.
    pub place: &'static Location<'static>,
    /// The feature that set it up.
    pub feature: Option<&'static str>,
    /// The scope that owns it, as `Scope::key` names it; `None` for the
    /// application.
    pub scope: Option<usize>,
    /// The period of the last tick, or of the first one to come.
    pub period: Duration,
}

struct Entry {
    info: TimerInfo,
    period: Period,
    /// Which chain of wake-ups is the live one. Changing the period starts a
    /// new chain, and whatever the old one had already queued is dropped
    /// when it comes due.
    generation: u64,
    active: Option<Box<dyn Fn() -> bool>>,
    run: Box<dyn FnMut()>,
    traced: bool,
}

thread_local! {
    static RUNNING: RefCell<HashMap<u64, Weak<RefCell<Entry>>>> = RefCell::new(HashMap::new());
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// What keeps a timer running; the context that set it up holds it.
pub struct Ticking {
    entry: Rc<RefCell<Entry>>,
}

impl Drop for Ticking {
    fn drop(&mut self) {
        let id = self.entry.borrow().info.id;
        RUNNING.with(|running| running.borrow_mut().remove(&id));
    }
}

impl guinea_core::scope::Teardown for Ticking {
    fn teardown(self) {
        drop(self);
    }
}

/// A timer that was set up, for saying more about it.
///
/// Holding it does not keep the timer running, and dropping it does not stop
/// it: that is up to the context.
#[derive(Clone)]
pub struct Timer {
    entry: Weak<RefCell<Entry>>,
}

impl Timer {
    fn change(self, change: impl FnOnce(&mut Entry)) -> Self {
        if let Some(entry) = self.entry.upgrade() {
            change(&mut entry.borrow_mut());
        }
        self
    }

    /// What devtools call it, instead of where it was set up.
    pub fn named(self, name: &'static str) -> Self {
        self.change(|entry| entry.info.name = Some(name))
    }

    /// Ticks only while `active` says so; the period runs on regardless.
    pub fn when(self, active: impl Fn() -> bool + 'static) -> Self {
        self.change(|entry| entry.active = Some(Box::new(active)))
    }

    /// Changes how often it ticks, from now.
    ///
    /// The wait already under way is cut short rather than waited out, which
    /// is the difference from a [`Period::Varying`] that answers differently
    /// the next time it is asked: an hourly timer told to tick every second
    /// does so within the second, not within the hour.
    pub fn period(self, period: impl Into<Period>) -> Self {
        let Some(entry) = self.entry.upgrade() else {
            return self;
        };

        let (id, generation, next) = {
            let mut entry = entry.borrow_mut();
            entry.period = period.into();
            entry.generation += 1;

            let next = entry.period.next();
            entry.info.period = next;

            (entry.info.id, entry.generation, next)
        };

        wake_after(id, generation, next);
        self
    }

    /// Leaves no trace and does not show up in devtools: for tooling that
    /// watches the application and must not be seen in what it watches.
    pub fn untraced(self) -> Self {
        self.change(|entry| entry.traced = false)
    }

    pub fn id(&self) -> Option<u64> {
        self.entry.upgrade().map(|entry| entry.borrow().info.id)
    }
}

/// Starts a timer: what keeps it running, for the context to hold, and the
/// timer itself, for the caller to name.
pub(crate) fn start(
    place: &'static Location<'static>,
    feature: Option<&'static str>,
    scope: Option<usize>,
    period: Period,
    run: impl FnMut() + 'static,
) -> (Ticking, Timer) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let first = period.next();

    let entry = Rc::new(RefCell::new(Entry {
        info: TimerInfo {
            id,
            name: None,
            place,
            feature,
            scope,
            period: first,
        },
        period,
        generation: 0,
        active: None,
        run: Box::new(run),
        traced: true,
    }));

    RUNNING.with(|running| running.borrow_mut().insert(id, Rc::downgrade(&entry)));
    wake_after(id, 0, first);

    let timer = Timer {
        entry: Rc::downgrade(&entry),
    };
    (Ticking { entry }, timer)
}

/// The timers running on this thread that devtools may see, oldest first.
pub fn running() -> Vec<TimerInfo> {
    let mut timers: Vec<TimerInfo> = RUNNING.with(|running| {
        running
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .filter(|entry| entry.borrow().traced)
            .map(|entry| entry.borrow().info.clone())
            .collect()
    });

    timers.sort_by_key(|timer| timer.id);
    timers
}

fn tick(id: u64, generation: u64) {
    let entry = RUNNING.with(|running| running.borrow().get(&id).and_then(Weak::upgrade));
    let Some(entry) = entry else {
        return;
    };

    // Left over from a period that has since changed: the chain it belonged
    // to ended when the new one was armed.
    if entry.borrow().generation != generation {
        return;
    }

    let (active, traced) = {
        let entry = entry.borrow();
        (entry.active.as_ref().is_none_or(|active| active()), entry.traced)
    };

    if active {
        let mut run = std::mem::replace(&mut entry.borrow_mut().run, Box::new(|| {}));

        {
            let _tick = traced
                .then(|| guinea_core::trace::enter(|| guinea_core::trace::Point::Tick { timer: id }));
            run();
        }

        entry.borrow_mut().run = run;
    }

    let next = {
        let mut entry = entry.borrow_mut();
        let next = entry.period.next();
        entry.info.period = next;

        next
    };

    // With the generation this tick came from, not the one the entry holds
    // now: a `Timer::period` from inside `run` has already armed its own
    // chain, and this one is over.
    wake_after(id, generation, next);
}

struct Wake {
    at: Instant,
    id: u64,
    generation: u64,
}

/// One thread for every timer: it sleeps until the nearest is due and hands
/// the tick to the UI thread.
fn wake_after(id: u64, generation: u64, after: Duration) {
    static SCHEDULER: OnceLock<Mutex<Sender<Wake>>> = OnceLock::new();

    let scheduler = SCHEDULER.get_or_init(|| {
        let (wakes, inbox) = channel();
        std::thread::Builder::new()
            .name("guinea-timers".into())
            .spawn(move || schedule(inbox))
            .expect("spawning the timer thread");
        Mutex::new(wakes)
    });

    let wake = Wake {
        at: Instant::now() + after,
        id,
        generation,
    };
    let _ = scheduler
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .send(wake);
}

fn schedule(inbox: Receiver<Wake>) {
    let mut due: BinaryHeap<Reverse<(Instant, u64, u64)>> = BinaryHeap::new();

    loop {
        let now = Instant::now();
        while let Some(Reverse((at, id, generation))) = due.peek().copied() {
            if at > now {
                break;
            }
            due.pop();
            invoke_on_ui(move || tick(id, generation));
        }

        let wake = match due.peek() {
            Some(Reverse((at, _, _))) => match inbox.recv_timeout(at.saturating_duration_since(now)) {
                Ok(wake) => wake,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match inbox.recv() {
                Ok(wake) => wake,
                Err(_) => return,
            },
        };
        due.push(Reverse((wake.at, wake.id, wake.generation)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::actor::event_bus::EventBus;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    /// `invoke_on_ui` (under `test-utils`) queues onto `EventBus`'s single
    /// *global* task queue, so one test's `wait()` would drain another's
    /// ticks. This keeps the module's tests one at a time.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn wait(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
        EventBus::process_queue();
    }

    /// Waits for the counter to reach `expected`, or gives up after two
    /// seconds and lets the assertion report what it actually saw.
    fn wait_for(counter: &Arc<AtomicUsize>, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);

        while Instant::now() < deadline {
            EventBus::process_queue();
            if counter.load(Ordering::SeqCst) >= expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn counting(period: u64) -> (Ticking, Timer, Arc<AtomicUsize>) {
        let counter = Arc::new(AtomicUsize::new(0));

        let c = counter.clone();
        let (ticking, timer) = start(
            Location::caller(),
            None,
            None,
            Duration::from_millis(period).into(),
            move || {
                c.fetch_add(1, Ordering::SeqCst);
            },
        );

        (ticking, timer, counter)
    }

    #[test]
    fn a_timer_ticks_every_period() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_ticking, _timer, counter) = counting(30);

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        wait_for(&counter, 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        wait_for(&counter, 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_timer_stops_with_what_keeps_it() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (ticking, _timer, counter) = counting(30);

        wait_for(&counter, 1);
        drop(ticking);

        wait(80);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_timer_dropped_before_its_first_tick_never_ticks() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (ticking, _timer, counter) = counting(200);

        drop(ticking);

        wait(250);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn changing_the_period_cuts_the_wait_short_and_ends_the_old_chain() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_ticking, timer, counter) = counting(60_000);

        let timer = timer.period(Duration::from_millis(20));
        wait_for(&counter, 1);
        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "a minute's wait was not waited out"
        );

        let _timer = timer.period(Duration::from_secs(60));
        let seen = counter.load(Ordering::SeqCst);

        wait(80);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            seen,
            "the twenty-millisecond chain kept ticking after the period changed"
        );
    }

    #[test]
    fn a_timer_ticks_only_while_active() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_ticking, timer, counter) = counting(30);

        let active = Arc::new(AtomicBool::new(false));
        let a = active.clone();
        let _timer = timer.when(move || a.load(Ordering::SeqCst));

        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        active.store(true, Ordering::SeqCst);
        wait_for(&counter, 1);
        assert!(counter.load(Ordering::SeqCst) >= 1);

        active.store(false, Ordering::SeqCst);
        let seen = counter.load(Ordering::SeqCst);
        wait(60);
        assert_eq!(counter.load(Ordering::SeqCst), seen);
    }

    #[test]
    fn a_tick_may_list_the_running_timers_itself() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let seen = Arc::new(AtomicUsize::new(0));

        let s = seen.clone();
        let (_ticking, _timer) = start(
            Location::caller(),
            None,
            None,
            Duration::from_millis(30).into(),
            move || {
                s.store(running().len(), Ordering::SeqCst);
            },
        );

        wait_for(&seen, 1);
        assert!(seen.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn a_running_timer_says_where_it_was_set_up_and_what_it_is_called() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (ticking, timer, _counter) = counting(1_000);
        let (_kept, hidden, _) = counting(1_000);

        let timer = timer.named("sweep");
        let hidden = hidden.untraced().id();

        let id = timer.id().expect("running");
        let listed = running();
        let info = listed.iter().find(|info| info.id == id).expect("listed");
        assert_eq!(info.name, Some("sweep"));
        assert!(info.place.file().ends_with("timers.rs"));
        assert_eq!(info.period, Duration::from_millis(1_000));
        assert!(listed.iter().all(|info| Some(info.id) != hidden));

        drop(ticking);
        assert!(running().iter().all(|info| info.id != id));
        assert_eq!(timer.id(), None);
    }
}
