use guinea_core::messages;
use guinea_core::scope::Reducer;

// The one action this feature answers. Who answers it is not written down
// anywhere - see `install`.
messages! { Refresh }

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Services {
    pub items: Vec<String>,
}

#[derive(Clone)]
pub enum Listed {
    Items(Vec<String>),
}

impl Reducer for Services {
    type Update = Listed;

    fn reduce(&mut self, update: Listed) {
        match update {
            Listed::Items(items) => self.items = items,
        }
    }
}
