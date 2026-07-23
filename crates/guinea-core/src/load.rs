/// Status of a value that arrives asynchronously (a feature's `State` field
/// fed by a loader or a live push), so "not here yet" is representable as
/// data instead of requiring a separate suspense mechanism at the read site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load<T> {
    NotAsked,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Default for Load<T> {
    fn default() -> Self {
        Load::Loading
    }
}

impl<T> Load<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading)
    }
}
