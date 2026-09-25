//! Actions and events a tool can send from outside, as JSON.
//!
//! Always compiled, and empty until something is registered. A type opts in
//! with `#[derive(guinea::Remote)]` and says which it is -
//! `#[remote(action)]`, `#[remote(event)]` or both - and the derive registers
//! it here under its short name. Nothing is reachable by default: what an
//! agent may do to a running application is what its author listed.
//!
//! An action goes to whichever scope answers it, the way a page's
//! [`Dispatch`](crate::feature::Dispatch) would send it; an event goes out on
//! the global bus as if it had arrived on the UI thread. Either is one
//! [`Point::Action`](crate::trace::Point::Action), and its id is handed back,
//! so what it set off can be followed in the trace.

use std::rc::Rc;

use serde::de::DeserializeOwned;

use crate::actor::event_bus::{Event, GlobalEventBus};
use crate::actor::short_type_name;
use crate::scope::Scope;
use crate::trace::{self, Point};

/// Hands the decoded action to a scope if the scope answers it: `None` when
/// it does not, the action's id when it did.
pub type Emit = fn(&Rc<Scope>, &str) -> Option<Result<u64, String>>;

/// An action a tool may send, registered by the derive.
pub struct RemoteAction {
    /// The type's name, without its path: what a tool sends it by.
    pub name: &'static str,
    pub emit: Emit,
}

/// An event a tool may publish, registered by the derive.
pub struct RemoteEvent {
    pub name: &'static str,
    pub publish: fn(&str) -> Result<u64, String>,
}

inventory::collect!(RemoteAction);
inventory::collect!(RemoteEvent);

/// The action registered as `name`.
pub fn action(name: &str) -> Option<&'static RemoteAction> {
    inventory::iter::<RemoteAction>().find(|action| action.name == name)
}

/// The event registered as `name`.
pub fn event(name: &str) -> Option<&'static RemoteEvent> {
    inventory::iter::<RemoteEvent>().find(|event| event.name == name)
}

/// Every action a tool may send, by name.
pub fn actions() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = inventory::iter::<RemoteAction>()
        .map(|action| action.name)
        .collect();
    names.sort_unstable();
    names
}

/// Every event a tool may publish, by name.
pub fn events() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = inventory::iter::<RemoteEvent>()
        .map(|event| event.name)
        .collect();
    names.sort_unstable();
    names
}

/// What the derive registers for an action.
pub fn emit_json<M: DeserializeOwned + 'static>(
    scope: &Rc<Scope>,
    json: &str,
) -> Option<Result<u64, String>> {
    let answer = scope.first_answerer::<M>()?;

    let action: M = match serde_json::from_str(json) {
        Ok(action) => action,
        Err(error) => return Some(Err(format!("{}: {error}", short_type_name::<M>()))),
    };

    let entered = trace::enter(|| Point::Action {
        message: short_type_name::<M>(),
    });
    let cause = entered.id().get();
    answer(action);

    Some(Ok(cause))
}

/// What the derive registers for an event. On the UI thread, where the global
/// bus lives.
pub fn publish_json<M: Event + DeserializeOwned>(json: &str) -> Result<u64, String> {
    let event: M = serde_json::from_str(json)
        .map_err(|error| format!("{}: {error}", short_type_name::<M>()))?;

    let entered = trace::enter(|| Point::Action {
        message: short_type_name::<M>(),
    });
    let cause = entered.id().get();
    GlobalEventBus::bus().publish(event);

    Ok(cause)
}
