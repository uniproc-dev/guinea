//! Asking a scope whether it may be torn down.
//!
//! A guard is a stage of navigation, not a redirect. Redirect-as-guard
//! corrupts history - the place you were refused entry to is still in the back
//! stack, so going back walks into the refusal again - and every ecosystem
//! that tried it reached that conclusion separately.
//!
//! It lives here rather than in the router because the thing being asked is a
//! [`Scope`](crate::scope::Scope): "may I be dropped" is a question about a
//! scope's own state, and the router only decides when to ask it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// What to put to the user when a guard wants an answer.
///
/// Plain data, no toolkit: every backend can draw three strings, and none of
/// them can draw each other's dialog type. Built at the moment of asking
/// rather than at install, so it is in whatever language is current then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub text: String,
    pub confirm: String,
    pub cancel: String,
}

impl Ask {
    pub fn new(
        text: impl Into<String>,
        confirm: impl Into<String>,
        cancel: impl Into<String>,
    ) -> Self {
        Self {
            text: text.into(),
            confirm: confirm.into(),
            cancel: cancel.into(),
        }
    }
}

/// What a guard answers.
///
/// `Allow` allocates nothing, so the overwhelming majority of navigations stay
/// exactly as synchronous as they were. "Optionally async" is the right to
/// return the third variant, not a cost paid by the first.
/// `Clone` because a backend may have to answer from a place that cannot reach
/// the state the answer was computed from - it keeps the verdict beside the
/// state and hands out copies.
#[derive(Clone)]
pub enum Verdict {
    Allow,
    Block,
    /// Not yet. The question goes to the user; the [`Decision`] carries the
    /// answer back.
    Ask(Ask, Decision),
}

impl Verdict {
    /// A question for the router to put, answered through `Router::answer`.
    ///
    /// A guard that means to answer from somewhere else - an actor finishing a
    /// save, say - builds the [`Decision`] itself, keeps a clone, and returns
    /// [`Verdict::Ask`] directly.
    pub fn ask(ask: Ask) -> Self {
        Verdict::Ask(ask, Decision::new())
    }
}

/// The answer to a guard's question, before there is one.
///
/// A token rather than a future, deliberately. There is no `LocalSet` in the
/// tree and the future would be `!Send`; this is an actor system rather than a
/// combinator one, so the guard hands the token wherever the answer will come
/// from and someone calls [`allow`](Self::allow) or [`block`](Self::block).
///
/// Settling twice does nothing: a superseded navigation may leave a token
/// nobody will ever answer, and a dialog closed twice must not answer twice.
#[derive(Clone)]
pub struct Decision {
    inner: Rc<Inner>,
}

/// What runs once the answer arrives.
type Resume = Box<dyn FnOnce(bool)>;

struct Inner {
    settled: Cell<bool>,
    answer: Cell<bool>,
    /// Installed by whoever is waiting - the router, in practice.
    resume: RefCell<Option<Resume>>,
}

impl Default for Decision {
    fn default() -> Self {
        Self::new()
    }
}

impl Decision {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                settled: Cell::new(false),
                answer: Cell::new(false),
                resume: RefCell::new(None),
            }),
        }
    }

    /// Go ahead with the navigation that was parked.
    pub fn allow(&self) {
        self.settle(true);
    }

    /// Stay where we are.
    pub fn block(&self) {
        self.settle(false);
    }

    pub fn is_settled(&self) -> bool {
        self.inner.settled.get()
    }

    /// What to run once the answer arrives. Replaces any previous one.
    ///
    /// If the answer already arrived, `resume` runs at once - a guard is free
    /// to settle its own token before returning, and nothing should depend on
    /// which came first.
    pub fn on_answer(&self, resume: impl FnOnce(bool) + 'static) {
        if self.inner.settled.get() {
            resume(self.answered_with());
            return;
        }
        *self.inner.resume.borrow_mut() = Some(Box::new(resume));
    }

    fn settle(&self, allowed: bool) {
        if self.inner.settled.replace(true) {
            return;
        }
        self.inner.answer.set(allowed);
        // Taken out before running: `resume` is free to start another
        // navigation, which would otherwise re-enter this borrow.
        let resume = self.inner.resume.borrow_mut().take();
        if let Some(resume) = resume {
            resume(allowed);
        }
    }

    fn answered_with(&self) -> bool {
        self.inner.answer.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reaches_whoever_was_waiting() {
        let seen = Rc::new(Cell::new(None));
        let decision = Decision::new();

        let recorder = seen.clone();
        decision.on_answer(move |allowed| recorder.set(Some(allowed)));

        decision.allow();
        assert_eq!(seen.get(), Some(true));
    }

    #[test]
    fn an_answer_that_arrived_first_is_not_lost() {
        let decision = Decision::new();
        decision.block();

        let seen = Rc::new(Cell::new(None));
        let recorder = seen.clone();
        decision.on_answer(move |allowed| recorder.set(Some(allowed)));

        assert_eq!(
            seen.get(),
            Some(false),
            "a guard may settle its own token before it returns"
        );
    }

    #[test]
    fn a_second_answer_changes_nothing() {
        let count = Rc::new(Cell::new(0));
        let decision = Decision::new();

        let counter = count.clone();
        decision.on_answer(move |_| counter.set(counter.get() + 1));

        decision.allow();
        decision.block();
        assert_eq!(count.get(), 1, "a dialog closed twice must not answer twice");
    }
}
