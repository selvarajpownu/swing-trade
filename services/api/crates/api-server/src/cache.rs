//! A small in-memory cache with a fixed time-to-live.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

/// Async TTL cache. Values are cloned out, so keep them cheap to clone
/// (wrap large ones in `Arc`).
///
/// When full, expired entries are dropped first, then the oldest entry.
pub struct TtlCache<K, V> {
    ttl: Duration,
    capacity: usize,
    entries: RwLock<HashMap<K, (Instant, V)>>,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// A cache holding at most `capacity` entries for `ttl` each.
    /// A zero `ttl` disables caching.
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            ttl,
            capacity: capacity.max(1),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// The configured time-to-live.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// A fresh value for `key`, if any.
    pub async fn get(&self, key: &K) -> Option<V> {
        let entries = self.entries.read().await;
        entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, v)| v.clone())
    }

    /// Stores `value` under `key`, replacing any previous value.
    pub async fn insert(&self, key: K, value: V) {
        if self.ttl.is_zero() {
            return;
        }
        let mut entries = self.entries.write().await;
        if entries.len() >= self.capacity && !entries.contains_key(&key) {
            let ttl = self.ttl;
            entries.retain(|_, (at, _)| at.elapsed() < ttl);
            if entries.len() >= self.capacity {
                let oldest = entries
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(k, _)| k.clone());
                if let Some(k) = oldest {
                    entries.remove(&k);
                }
            }
        }
        entries.insert(key, (Instant::now(), value));
    }

    /// Number of stored entries, including expired ones not yet evicted.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// True when nothing is stored.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_and_expires() {
        let cache = TtlCache::new(Duration::from_secs(60), 10);
        assert_eq!(cache.get(&"a").await, None);
        cache.insert("a", 1).await;
        assert_eq!(cache.get(&"a").await, Some(1));
        cache.insert("a", 2).await;
        assert_eq!(cache.get(&"a").await, Some(2));
        assert_eq!(cache.len().await, 1);

        let short = TtlCache::new(Duration::from_millis(20), 10);
        short.insert("a", 1).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(short.get(&"a").await, None);
    }

    #[tokio::test]
    async fn zero_ttl_disables_caching() {
        let cache = TtlCache::new(Duration::ZERO, 10);
        cache.insert("a", 1).await;
        assert!(cache.is_empty().await);
        assert_eq!(cache.get(&"a").await, None);
    }

    #[tokio::test]
    async fn evicts_oldest_when_full() {
        let cache = TtlCache::new(Duration::from_secs(60), 2);
        cache.insert("a", 1).await;
        tokio::time::sleep(Duration::from_millis(2)).await;
        cache.insert("b", 2).await;
        tokio::time::sleep(Duration::from_millis(2)).await;
        cache.insert("c", 3).await;
        assert_eq!(cache.len().await, 2);
        assert_eq!(cache.get(&"a").await, None);
        assert_eq!(cache.get(&"b").await, Some(2));
        assert_eq!(cache.get(&"c").await, Some(3));
    }
}
