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
    max_entries: usize,
}

impl<T: Clone> MemoryCache<T> {
    pub fn new(default_ttl: Option<Duration>) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            default_ttl,
            max_entries: 1024,
        }
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
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
        let mut inner = match self.inner.write() {
            Ok(g) => g,
            Err(_) => return,
        };
        if inner.len() >= self.max_entries && !inner.contains_key(key) {
            inner.clear();
        }
        inner.insert(key.to_string(), CacheEntry { value, expires_at });
    }

    pub fn set_with_ttl(&self, key: &str, value: T, ttl: Duration) {
        let mut inner = match self.inner.write() {
            Ok(g) => g,
            Err(_) => return,
        };
        if inner.len() >= self.max_entries && !inner.contains_key(key) {
            inner.clear();
        }
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

    pub fn evict_expired(&self) -> usize {
        let mut inner = match self.inner.write() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let now = Instant::now();
        let before = inner.len();
        inner.retain(|_, entry| match entry.expires_at {
            Some(exp) => exp > now,
            None => true,
        });
        before - inner.len()
    }
}