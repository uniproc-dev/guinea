use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

type Service = Arc<dyn Any + Send + Sync>;

/// A lock poisoned by a panic elsewhere - distinct from "no such service".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Poisoned;

impl std::fmt::Display for Poisoned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("shared state lock was poisoned by a panic")
    }
}

impl std::error::Error for Poisoned {}

#[derive(Clone, Default)]
pub struct SharedState {
    inner: Arc<RwLock<HashMap<TypeId, Service>>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert<T>(&self, value: T) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let id = TypeId::of::<T>();
        let mut map = self.inner.write().ok()?;
        let previous = map.insert(id, Arc::new(value));
        previous.and_then(|svc| svc.downcast::<T>().ok())
    }

    pub fn insert_arc<T>(&self, value: Arc<T>) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let id = TypeId::of::<T>();
        let mut map = self.inner.write().ok()?;
        let previous = map.insert(id, value);
        previous.and_then(|svc| svc.downcast::<T>().ok())
    }

    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.try_get().ok().flatten()
    }

    /// Like [`get`](Self::get), but tells a poisoned lock apart from a missing
    /// service.
    pub fn try_get<T>(&self) -> Result<Option<Arc<T>>, Poisoned>
    where
        T: Send + Sync + 'static,
    {
        let map = self.inner.read().map_err(|_| Poisoned)?;
        Ok(map
            .get(&TypeId::of::<T>())
            .and_then(|svc| svc.clone().downcast::<T>().ok()))
    }

    pub fn remove<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let id = TypeId::of::<T>();
        let mut map = self.inner.write().ok()?;
        map.remove(&id)?.downcast::<T>().ok()
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        let id = TypeId::of::<T>();
        self.inner
            .read()
            .map(|m| m.contains_key(&id))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{Poisoned, SharedState};
    use std::sync::Arc;

    #[test]
    fn test_shared_state_clone() {
        let shared = SharedState::new();
        let clone = shared.clone();

        shared.insert("hello".to_string());

        assert_eq!(
            clone.get::<String>().as_ref().map(|s| s.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn insert_get_remove_roundtrip() {
        let shared = SharedState::new();

        assert!(!shared.contains::<String>());
        assert!(shared.insert("hello".to_string()).is_none());
        assert!(shared.contains::<String>());
        assert_eq!(
            shared.get::<String>().as_ref().map(|s| s.as_str()),
            Some("hello")
        );

        let removed = shared.remove::<String>();
        assert_eq!(removed.as_ref().map(|s| s.as_str()), Some("hello"));
        assert!(shared.get::<String>().is_none());
    }

    #[test]
    fn overwrite_returns_previous() {
        let shared = SharedState::new();
        shared.insert(7u32);
        let prev = shared.insert(9u32);
        assert_eq!(prev.map(|v| *v), Some(7u32));
        assert_eq!(shared.get::<u32>().map(|v| *v), Some(9u32));
    }

    #[test]
    fn insert_arc_keeps_identity() {
        let shared = SharedState::new();
        let service = Arc::new(42usize);
        let ptr = Arc::as_ptr(&service);
        shared.insert_arc(Arc::clone(&service));

        let fetched = shared.get::<usize>().expect("service should exist");
        assert_eq!(Arc::as_ptr(&fetched), ptr);
    }

    #[test]
    fn try_get_reports_a_missing_service_and_a_poisoned_lock_differently() {
        let shared = SharedState::new();
        assert_eq!(shared.try_get::<u32>().map(|v| v.is_none()), Ok(true));

        let poisoner = shared.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.inner.write().unwrap();
            panic!("poison the lock");
        })
        .join();

        assert_eq!(shared.try_get::<u32>().err(), Some(Poisoned));
        assert_eq!(shared.get::<u32>().map(|v| *v), None);
    }
}
