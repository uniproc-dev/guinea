//! What ends a background task when the actor that started it is gone.

use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

/// A cancellation token, shared by an actor and everything it spawned.
///
/// Clones share one flag: cancelling any of them cancels them all, and a
/// token cancelled once stays cancelled.
#[derive(Clone, Default)]
pub struct Cancel {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    cancelled: AtomicBool,
    woken: Notify,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ends the work this token stands for, and wakes whoever waits on it.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.woken.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Waits for [`Cancel::cancel`], and returns at once if it already came.
    pub async fn cancelled(&self) {
        let mut woken = pin!(self.inner.woken.notified());
        woken.as_mut().enable();

        if self.is_cancelled() {
            return;
        }

        woken.await;
    }

    /// `fut`'s output, or `None` when cancellation came first - in which case
    /// `fut` is dropped where it last awaited.
    pub async fn guard<F: Future>(&self, fut: F) -> Option<F::Output> {
        tokio::select! {
            biased;
            () = self.cancelled() => None,
            out = fut => Some(out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_guarded_future_is_dropped_when_the_token_is_cancelled() {
        let cancel = Cancel::new();
        let cancelling = cancel.clone();

        tokio::spawn(async move { cancelling.cancel() });

        let forever = std::future::pending::<()>();
        assert!(cancel.guard(forever).await.is_none());
    }

    #[tokio::test]
    async fn a_future_that_finishes_first_still_answers() {
        let cancel = Cancel::new();

        assert_eq!(cancel.guard(async { 7 }).await, Some(7));
        assert!(!cancel.is_cancelled());
    }

    #[tokio::test]
    async fn waiting_on_a_token_cancelled_earlier_returns_at_once() {
        let cancel = Cancel::new();
        cancel.cancel();

        cancel.cancelled().await;
        assert!(cancel.guard(async { 7 }).await.is_none());
    }
}
