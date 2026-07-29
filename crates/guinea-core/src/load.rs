use std::sync::Arc;

/// Status of a value that arrives asynchronously (a feature's `State` field
/// fed by a loader or a live push), so "not here yet" is representable as
/// data instead of requiring a separate suspense mechanism at the read site.
#[derive(Clone, Debug)]
pub enum Load<T> {
    NotAsked,
    Loading,
    Ready(T),
    Failed(Arc<anyhow::Error>),
}

impl<T> Default for Load<T> {
    fn default() -> Self {
        Load::Loading
    }
}

/// `anyhow::Error` has no equality of its own; two `Failed`s compare by
/// `Arc` identity (a re-fetched failure is a *new* value and must re-render).
impl<T: PartialEq> PartialEq for Load<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Load::NotAsked, Load::NotAsked) | (Load::Loading, Load::Loading) => true,
            (Load::Ready(a), Load::Ready(b)) => a == b,
            (Load::Failed(a), Load::Failed(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl<T: Eq> Eq for Load<T> {}

impl<T> Load<T> {
    pub fn failed(err: impl Into<anyhow::Error>) -> Self {
        Load::Failed(Arc::new(err.into()))
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Load::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading)
    }

    pub fn error(&self) -> Option<&anyhow::Error> {
        match self {
            Load::Failed(err) => Some(err),
            _ => None,
        }
    }
}
