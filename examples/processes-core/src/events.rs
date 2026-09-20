use guinea::prelude::Event;

#[derive(Clone, Event)]
pub struct ProcessKilled {
    pub name: String,
}
