use guinea::prelude::Message;


#[derive(Clone)]
pub struct ProcessKilled {
    pub name: String,
}

impl Message for ProcessKilled {}
