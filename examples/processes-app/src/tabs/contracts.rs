use guinea_macros::{ReducerState, reducer};

/// Proof that `TabsLayout`'s `Scope` (and the cell in it) survives a
/// `Processes <-> Services` navigation: `install_count` increments once per
/// real (re)install, not once per switch.
#[derive(ReducerState)]
pub struct TabsViewState {
    pub install_count: i32,
}

#[reducer]
pub fn TabsReducer(state: &mut TabsViewState, msg: i32) {
    state.install_count = msg;
}
