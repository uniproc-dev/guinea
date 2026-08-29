use guinea_core::scope::Reducer;
use guinea_widgets::chart::RingSeries;

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Metrics {
    pub cpu: RingSeries,
    pub memory: RingSeries,
}

#[derive(Clone)]
pub enum Sampled {
    At { at: u64, cpu: f32, memory: f32 },
}

impl Reducer for Metrics {
    type Update = Sampled;

    fn reduce(&mut self, update: Sampled) {
        match update {
            Sampled::At { at, cpu, memory } => {
                self.cpu.push((at, cpu));
                self.memory.push((at, memory));
            }
        }
    }
}
