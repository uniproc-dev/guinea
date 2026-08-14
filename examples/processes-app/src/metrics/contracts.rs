use guinea_macros::{ReducerState, port, reducer};
use guinea_widgets::chart::RingSeries;

#[derive(Clone)]
pub enum MetricsMsg {
    Sample { at: u64, cpu: f32, memory: f32 },
}

#[port]
pub trait MetricsPort {
    fn send(&self, msg: MetricsMsg);
}

#[derive(ReducerState)]
pub struct MetricsViewState {
    pub cpu: RingSeries,
    pub memory: RingSeries,
}

#[reducer]
pub fn metrics_reducer(state: &mut MetricsViewState, msg: MetricsMsg) {
    match msg {
        MetricsMsg::Sample { at, cpu, memory } => {
            state.cpu.push((at, cpu));
            state.memory.push((at, memory));
        }
    }
}
