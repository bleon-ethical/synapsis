use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

struct CacheEntry<T> {
    value: T,
    expires_at: Option<Instant>,
}

pub struct MemoryCache<T> {
    inner: RwLock<HashMap<String, CacheEntry<T>>>,
    default_ttl: Option<Duration>,
}

impl<T: Clone> MemoryCache<T> {
    pub fn new(default_ttl: Option<Duration>) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            default_ttl,
        }
    }

    pub fn get(&self, key: &str) -> Option<T> {
        let inner = self.inner.read().ok()?;
        inner.get(key).and_then(|entry| {
            if let Some(expires) = entry.expires_at {
                if Instant::now() > expires {
                    return None;
                }
            }
            Some(entry.value.clone())
        })
    }

    pub fn set(&self, key: &str, value: T) {
        let expires_at = self.default_ttl.map(|ttl| Instant::now() + ttl);
        let mut inner = self.inner.write().ok()?;
        inner.insert(key.to_string(), CacheEntry { value, expires_at });
    }

    pub fn set_with_ttl(&self, key: &str, value: T, ttl: Duration) {
        let mut inner = self.inner.write().ok()?;
        inner.insert(
            key.to_string(),
            CacheEntry {
                value,
                expires_at: Some(Instant::now() + ttl),
            },
        );
    }

    pub fn remove(&self, key: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove(key);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.clear();
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}
