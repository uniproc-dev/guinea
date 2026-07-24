use guinea_macros::{ReducerState, actions, port, reducer};

#[derive(Clone)]
pub enum ProcessesMessages {
    SetItems(Vec<String>),
}

#[port]
pub trait ProcessesPort {
    fn send(&self, msg: ProcessesMessages);
}

#[actions]
pub trait ProcessesActions {
    fn on_kill<F>(&self, handler: F)
    where
        F: Fn(u32) + 'static;
}

#[derive(ReducerState)]
pub struct ProcessesViewState {
    pub items: Vec<String>,
}

#[reducer]
#[dispatch(ProcessesActions)]
pub fn processes_reducer(state: &mut ProcessesViewState, msg: ProcessesMessages) {
    match msg {
        ProcessesMessages::SetItems(items) => state.items = items,
    }
}
