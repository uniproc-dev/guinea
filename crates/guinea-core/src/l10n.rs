use crate::scope::{GlobalScope, NoopActions, Reducer, Subscription};
use std::marker::PhantomData;

struct Marker<S>(PhantomData<S>);

impl<S: Clone + Default + 'static> Reducer for Marker<S> {
    type State = S;
    type Push = S;
    type Group = ();
    type Actions = NoopActions;

    fn reduce(state: &mut Self::State, msg: Self::Push) {
        *state = msg;
    }
}

/// A set of localised strings that can be identified by, and rebuilt from, a
/// BCP-47 language tag.
///
/// Implemented by `guinea::fluent_loader!`. Code that has to handle languages
/// without knowing the concrete resolver - persisting the choice, offering a
/// language picker - goes through this; tags rather than
/// `unic_langid::LanguageIdentifier` keep that dependency out of the core.
pub trait Localization: Clone + Default + 'static {
    /// Returns `None` if the tag is malformed.
    fn for_tag(tag: &str) -> Option<Self>;

    fn tag(&self) -> String;
}

pub struct L10n<S>(PhantomData<S>);

impl<S: Clone + Default + 'static> L10n<S> {
    pub fn load(strings: S) {
        GlobalScope::instance().push::<Marker<S>>(strings);
    }

    pub fn current() -> S {
        GlobalScope::instance().state::<Marker<S>>().borrow().clone()
    }

    pub fn subscribe(callback: impl Fn(S) + 'static) -> Subscription {
        let scope = GlobalScope::instance();
        let scope_for_cb = scope.clone();
        scope.subscribe::<Marker<S>>(move || {
            callback(scope_for_cb.state::<Marker<S>>().borrow().clone());
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default, PartialEq, Debug)]
    struct Strings {
        greeting: String,
    }

    #[test]
    fn load_then_current_roundtrips() {
        L10n::<Strings>::load(Strings {
            greeting: "hello".into(),
        });
        assert_eq!(L10n::<Strings>::current().greeting, "hello");
    }

    #[test]
    fn subscribers_see_every_subsequent_load_until_dropped() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_for_cb = seen.clone();
        let sub = L10n::<Strings>::subscribe(move |s| seen_for_cb.borrow_mut().push(s.greeting));

        L10n::<Strings>::load(Strings { greeting: "a".into() });
        L10n::<Strings>::load(Strings { greeting: "b".into() });
        assert_eq!(*seen.borrow(), vec!["a", "b"]);

        drop(sub);
        L10n::<Strings>::load(Strings { greeting: "c".into() });
        assert_eq!(
            *seen.borrow(),
            vec!["a", "b"],
            "no notifications after the Subscription is dropped"
        );
    }
}
