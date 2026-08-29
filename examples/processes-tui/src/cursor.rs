use guinea_core::scope::Reducer;

/// Which row has the focus.
///
/// A reducer rather than a field in the front end, so it lives in the scope
/// the router installed for the page: every page keeps its own row, and the
/// row dies with the page instead of following the user to the next one.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Cursor {
    pub row: usize,
}

/// A step, and how many rows there were when it was taken - the list is
/// refreshed by an actor and can shrink under the focus.
#[derive(Clone)]
pub struct Move {
    pub delta: isize,
    pub len: usize,
}

impl Reducer for Cursor {
    type Update = Move;

    fn reduce(&mut self, step: Move) {
        let Some(last) = step.len.checked_sub(1) else {
            self.row = 0;
            return;
        };
        let next = (self.row as isize).saturating_add(step.delta);
        self.row = next.clamp(0, last as isize) as usize;
    }
}
