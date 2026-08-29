//! Where a node finds the navigator.
//!
//! A node's `update` is handed an [`UpdateCx`](crate::UpdateCx) and nothing
//! else - iced calls it as a free function, with no application object in
//! reach. So the navigator is parked here, per thread, by [`crate::run`]: one
//! window loop, one router, one route type.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use guinea_router::router::{NavigateHandle, RouteChain};

use crate::Iced;

thread_local! {
    static NAVIGATOR: RefCell<Option<Rc<dyn Any>>> = const { RefCell::new(None) };
}

pub(crate) fn install<R>(nav: NavigateHandle<Iced, R>)
where
    R: RouteChain<Iced> + Clone + PartialEq + 'static,
{
    NAVIGATOR.with(|slot| *slot.borrow_mut() = Some(Rc::new(nav) as Rc<dyn Any>));
}

pub(crate) fn clear() {
    NAVIGATOR.with(|slot| *slot.borrow_mut() = None);
}

pub(crate) fn current<R>() -> NavigateHandle<Iced, R>
where
    R: RouteChain<Iced> + Clone + PartialEq + 'static,
{
    NAVIGATOR.with(|slot| {
        let parked = slot.borrow().clone().unwrap_or_else(|| {
            panic!(
                "navigate::<{}>() with no navigator on this thread - either run() has not started yet, or this node is being updated on a thread that does not own the window",
                std::any::type_name::<R>()
            )
        });

        parked
            .downcast::<NavigateHandle<Iced, R>>()
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
