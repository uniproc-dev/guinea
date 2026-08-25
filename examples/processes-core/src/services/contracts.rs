use guinea_macros::{ReducerState, port, reducer};

#[derive(Clone)]
pub enum ServicesMessages {
    SetItems(Vec<String>),
}

#[port]
pub trait ServicesPort {
    fn send(&self, msg: ServicesMessages);
}

#[derive(ReducerState)]
pub struct ServicesViewState {
    pub items: Vec<String>,
}

#[reducer]
pub fn services_reducer(state: &mut ServicesViewState, msg: ServicesMessages) {
    match msg {
        ServicesMessages::SetItems(items) => state.items = items,
    }
}
