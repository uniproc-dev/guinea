use guinea_core::messages;
use guinea_macros::{ReducerState, port, reducer};

messages! {
    pub Processes {
        Kill(u32),
        Refresh,
    }
}

#[derive(Clone)]
pub enum ProcessesMessages {
    SetItems(Vec<String>),
}

#[port]
pub trait ProcessesPort {
    fn send(&self, msg: ProcessesMessages);
}

#[derive(ReducerState)]
pub struct ProcessesViewState {
    pub items: Vec<String>,
}

#[reducer]
#[dispatch(Processes)]
pub fn processes_reducer(state: &mut ProcessesViewState, msg: ProcessesMessages) {
    match msg {
        ProcessesMessages::SetItems(items) => state.items = items,
    }
}
