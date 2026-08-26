use guinea_core::scope::{NoopActions, Reducer};

/// Which row has the focus.
///
/// A reducer rather than a field in the front end, so it lives in the scope
/// the router installed for the page: every page keeps its own row, and the
/// row dies with the page instead of following the user to the next one.
pub struct Cursor;

/// A step, and how many rows there were when it was taken - the list is
/// refreshed by an actor and can shrink under the focus.
pub struct Move {
    pub delta: isize,
    pub len: usize,
}

impl Reducer for Cursor {
    type State = usize;
    type Push = Move;
    type Group = ();
    type Actions = NoopActions;

    fn reduce(state: &mut Self::State, msg: Self::Push) {
        let Some(last) = msg.len.checked_sub(1) else {
            *state = 0;
            return;
        };
        let next = (*state as isize).saturating_add(msg.delta);
        *state = next.clamp(0, last as isize) as usize;
    }
}

/// The focused row, clamped to a list that may have shrunk since the last key.
pub fn focused(cursor: usize, len: usize) -> usize {
    cursor.min(len.saturating_sub(1))
}
