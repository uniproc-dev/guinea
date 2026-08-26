//! Where wiring finds the application's root component.
//!
//! Globals hang off the root, and a page needs them while it is being
//! installed - long before it is asked to draw anything, and with nothing but
//! a `SegmentProps` in hand. So [`crate::run`] parks the window here, per
//! thread: one window loop, one root.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

thread_local! {
    static ROOT: RefCell<Option<Rc<dyn Any>>> = const { RefCell::new(None) };
}

pub(crate) fn install<W: ComponentHandle + 'static>(root: W) {
    ROOT.with(|slot| *slot.borrow_mut() = Some(Rc::new(root) as Rc<dyn Any>));
}

pub(crate) fn clear() {
    ROOT.with(|slot| *slot.borrow_mut() = None);
}

pub(crate) fn current<W: ComponentHandle + 'static>() -> W {
    ROOT.with(|slot| {
        let parked = slot.borrow().clone().unwrap_or_else(|| {
            panic!(
                "root::<{}>() with no root on this thread - either run() has not started yet, or this is not the thread that owns the window",
                std::any::type_name::<W>()
            )
        });

        parked
            .downcast::<W>()
            .unwrap_or_else(|_| {
                panic!(
                    "root::<{}>() but run() was given a window of another type",
                    std::any::type_name::<W>()
                )
            })
            .clone_strong()
    })
}
