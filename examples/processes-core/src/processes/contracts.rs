//! Plain Rust, and this file is the point of the whole exercise.
//!
//! It used to be five declarations - `messages!`, `#[port]`, `#[reducer]`,
//! `#[dispatch]`, `#[derive(ReducerState)]` - and the central type of the
//! feature, the one every other file names, did not appear in any of them: it
//! was `ProcessesReducer`, produced by upper-camel-casing the name of a
//! function. What is left is a struct, an enum, and an impl.

use guinea_core::messages;
use guinea_core::scope::Reducer;

// What the actor answers to: kill the process with this pid, and list them
// again. Which actor that is, is settled where the two are already listed
// together - in `actor!` - and not here: this file knows state, and an actor
// named in it would be exactly the leak the layering exists to prevent.
messages! {
    Kill(u32),
    Refresh,
}

/// The state, which is the reducer.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Processes {
    pub items: Vec<String>,
}

/// What changes it. Only the actor produces these.
#[derive(Clone)]
pub enum Listed {
    Items(Vec<String>),
}

impl Reducer for Processes {
    type Update = Listed;

    fn reduce(&mut self, update: Listed) {
        match update {
            Listed::Items(items) => self.items = items,
        }
    }
}
