use guinea_core::actor::traits::Message;


#[derive(Clone)]
pub struct ProcessKilled {
    pub name: String,
}

impl Message for ProcessKilled {}
