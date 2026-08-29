//! Where a view finds the navigator.
//!
//! The other two backends hand it over: the reactor puts it in a component
//! context, the terminal passes it to the key handler. Slint has neither - a
//! `.slint` callback lands in a closure a page built long before, with nothing
//! but what it captured. So the navigator is parked here, per thread, by
//! [`crate::run`]: one window loop, one router, one route type.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use guinea_router::router::{NavigateHandle, RouteChain};

use crate::Slint;

thread_local! {
    static NAVIGATOR: RefCell<Option<Rc<dyn Any>>> = const { RefCell::new(None) };
}

pub(crate) fn install<R>(nav: NavigateHandle<Slint, R>)
where
    R: RouteChain<Slint> + Clone + PartialEq + 'static,
{
    NAVIGATOR.with(|slot| *slot.borrow_mut() = Some(Rc::new(nav) as Rc<dyn Any>));
}

pub(crate) fn clear() {
    NAVIGATOR.with(|slot| *slot.borrow_mut() = None);
}

pub(crate) fn current<R>() -> NavigateHandle<Slint, R>
where
    R: RouteChain<Slint> + Clone + PartialEq + 'static,
{
    NAVIGATOR.with(|slot| {
        let parked = slot.borrow().clone().unwrap_or_else(|| {
            panic!(
                "navigate::<{}>() with no navigator on this thread - either run() has not started yet, or this view is being built on a thread that does not own the window",
                std::any::type_name::<R>()
            )
        });

        parked
            .downcast::<NavigateHandle<Slint, R>>()
            .unwrap_or_else(|_| {
                panic!(
                    "navigate::<{}>() but run() was given a different route type",
                    std::any::type_name::<R>()
                )
            })
            .as_ref()
            .clone()
    })
}
