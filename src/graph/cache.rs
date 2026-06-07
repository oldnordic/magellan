//! LRU cache for frequently accessed graph query results
//!
//! Provides LRU caching for file nodes and symbol lookups
//! to reduce database round-trips for hot data.
//!
//! # Thread Safety
//!
//! `LruCache<K, V>` is NOT thread-safe (single-threaded, `&mut self` access).
//!
//! `ThreadSafeCache<K, V>` wraps `LruCache` in `parking_lot::Mutex` for safe
//! concurrent access from `&self` methods (e.g., `SymbolNavigator` queries).
//!
//! # Usage Pattern
//!
//! - `FileNodeCache` (single-threaded): accessed through `CodeGraph` with `&mut self`
//! - Navigator caches (thread-safe): accessed through `SymbolNavigator` with `&self`

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Cache statistics for monitoring effectiveness
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub size: usize,
}

impl CacheStats {
    /// Calculate cache hit rate as a percentage (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Simple LRU cache implementation
///
/// Uses HashMap for O(1) lookups and VecDeque for tracking access order.
/// When capacity is reached, the least recently used item is evicted.
pub struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
    hits: usize,
    misses: usize,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    /// Create a new LRU cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            order: VecDeque::with_capacity(capacity),
            hits: 0,
            misses: 0,
        }
    }

    /// Get a value from the cache by key
    ///
    /// Returns a reference to the value if present, None otherwise.
    /// On cache hit, the item is moved to the front of the LRU order.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.hits += 1;
            // Move to front of order
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
                self.order.push_front(key.clone());
            }
            self.map.get(key)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a key-value pair into the cache
    ///
    /// If the key already exists, the value is updated and the key is moved to front.
    /// If the cache is at capacity, the least recently used item is evicted.
    pub fn put(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            // Update existing: remove from current position
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        } else if self.order.len() >= self.capacity {
            // Evict oldest (least recently used)
            if let Some(old) = self.order.pop_back() {
                self.map.remove(&old);
            }
        }
        // Clone key for both order tracking and map insertion
        let key_clone = key.clone();
        self.order.push_front(key_clone);
        self.map.insert(key, value);
    }

    /// Invalidate a specific cache entry
    ///
    /// Removes the key and its value from the cache if present.
    pub fn invalidate(&mut self, key: &K) {
        self.map.remove(key);
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
    }

    /// Clear all entries from the cache
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get current cache size
    pub fn _len(&self) -> usize {
        self.map.len()
    }

    /// Check if cache is empty
    pub fn _is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            size: self.map.len(),
        }
    }

    /// Get hit rate as a percentage (0.0 to 1.0)
    pub fn _hit_rate(&self) -> f64 {
        self.stats().hit_rate()
    }
}

/// Specialized cache for file nodes
///
/// Caches FileNode lookups by file path to avoid repeated database queries.
pub type FileNodeCache = LruCache<String, crate::graph::schema::FileNode>;

/// Specialized cache for symbol vectors
///
/// Caches symbol vectors by file path for faster symbol lookups.
/// Currently unused internally but provided for API completeness and future use.
#[expect(dead_code)] // Future use: symbol vector caching
pub type SymbolCache = LruCache<String, Vec<crate::ingest::SymbolFact>>;

/// Thread-safe LRU cache wrapper for concurrent access from `&self` methods.
///
/// Wraps `LruCache` in `parking_lot::Mutex` so that navigator queries
/// (which borrow `&CodeGraph`) can update cache state without requiring
/// `&mut self`.
pub struct ThreadSafeCache<K, V> {
    inner: parking_lot::Mutex<LruCache<K, V>>,
}

impl<K: Hash + Eq + Clone, V> ThreadSafeCache<K, V> {
    /// Create a new thread-safe LRU cache with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: parking_lot::Mutex::new(LruCache::new(capacity)),
        }
    }

    /// Get a cloned value from the cache.
    ///
    /// Returns `Some(V)` on hit (cloned to release the lock immediately),
    /// `None` on miss.
    pub fn get_cloned(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let mut guard = self.inner.lock();
        guard.get(key).cloned()
    }

    /// Insert a key-value pair into the cache.
    pub fn put(&self, key: K, value: V) {
        let mut guard = self.inner.lock();
        guard.put(key, value);
    }

    /// Invalidate a specific cache entry.
    pub fn invalidate(&self, key: &K) {
        let mut guard = self.inner.lock();
        guard.invalidate(key);
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        let mut guard = self.inner.lock();
        guard.clear();
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let guard = self.inner.lock();
        guard.stats()
    }
}

// ThreadSafeCache is Send+Sync because parking_lot::Mutex provides synchronization
unsafe impl<K: Send, V: Send> Send for ThreadSafeCache<K, V> {}
unsafe impl<K: Send + Sync, V: Send + Sync> Sync for ThreadSafeCache<K, V> {}

/// Cache key for navigator entity lookups by ID.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct EntityCacheKey(pub i64);

/// Cache key for navigator symbol resolution by name.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct NameCacheKey(pub String);

/// Cache key for navigator edge expansion by entity ID.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ExpandCacheKey(pub i64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_cache_basic_operations() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);

        // Empty cache
        assert_eq!(cache.get(&"a".to_string()), None);
        assert_eq!(cache._len(), 0);
        assert!(cache._is_empty());

        // Insert and get
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"b".to_string()), Some(&2));
        assert_eq!(cache._len(), 2);
        assert!(!cache._is_empty());
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache: LruCache<String, i32> = LruCache::new(2);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        // Access 'a' to make it more recently used than 'b'
        cache.get(&"a".to_string());

        // Insert 'c' - should evict 'b' (least recently used)
        cache.put("c".to_string(), 3);

        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"b".to_string()), None); // Evicted
        assert_eq!(cache.get(&"c".to_string()), Some(&3));
        assert_eq!(cache._len(), 2);
    }

    #[test]
    fn test_lru_cache_update_existing() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        // Update 'a' and verify it moves to front
        cache.put("a".to_string(), 10);

        // Add 'd' - should evict 'b' (now LRU since 'a' was updated)
        cache.put("d".to_string(), 4);

        assert_eq!(cache.get(&"a".to_string()), Some(&10));
        assert_eq!(cache.get(&"b".to_string()), None); // Evicted
        assert_eq!(cache.get(&"c".to_string()), Some(&3));
        assert_eq!(cache.get(&"d".to_string()), Some(&4));
        assert_eq!(cache._len(), 3);
    }

    #[test]
    fn test_lru_cache_invalidate() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        // Invalidate 'b'
        cache.invalidate(&"b".to_string());

        assert_eq!(cache.get(&"a".to_string()), Some(&1));
        assert_eq!(cache.get(&"b".to_string()), None);
        assert_eq!(cache.get(&"c".to_string()), Some(&3));
        assert_eq!(cache._len(), 2);
    }

    #[test]
    fn test_lru_cache_clear() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        cache.clear();

        assert_eq!(cache._len(), 0);
        assert!(cache._is_empty());
        assert_eq!(cache.get(&"a".to_string()), None);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        // Generate some hits and misses
        cache.get(&"a".to_string()); // hit
        cache.get(&"b".to_string()); // hit
        cache.get(&"c".to_string()); // miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.size, 2);

        let hit_rate = cache._hit_rate();
        assert!((hit_rate - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_hit_rate_empty() {
        let cache: LruCache<String, i32> = LruCache::new(3);
        assert_eq!(cache._hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_hit_rate_all_hits() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);
        cache.put("a".to_string(), 1);
        cache.get(&"a".to_string());
        cache.get(&"a".to_string());
        assert_eq!(cache._hit_rate(), 1.0);
    }

    #[test]
    fn test_cache_hit_rate_all_misses() {
        let mut cache: LruCache<String, i32> = LruCache::new(3);
        cache.get(&"a".to_string());
        cache.get(&"b".to_string());
        assert_eq!(cache._hit_rate(), 0.0);
    }

    #[test]
    fn test_thread_safe_cache_basic() {
        let cache: ThreadSafeCache<String, i32> = ThreadSafeCache::new(3);

        // Empty cache returns None
        assert_eq!(cache.get_cloned(&"a".to_string()), None);

        // Insert and get
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        assert_eq!(cache.get_cloned(&"a".to_string()), Some(1));
        assert_eq!(cache.get_cloned(&"b".to_string()), Some(2));

        let stats = cache.stats();
        assert_eq!(stats.size, 2);
    }

    #[test]
    fn test_thread_safe_cache_eviction() {
        let cache: ThreadSafeCache<String, i32> = ThreadSafeCache::new(2);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        // Access 'a' to make it more recently used
        cache.get_cloned(&"a".to_string());

        // Insert 'c' — should evict 'b'
        cache.put("c".to_string(), 3);

        assert_eq!(cache.get_cloned(&"a".to_string()), Some(1));
        assert_eq!(cache.get_cloned(&"b".to_string()), None);
        assert_eq!(cache.get_cloned(&"c".to_string()), Some(3));
    }

    #[test]
    fn test_thread_safe_cache_invalidate() {
        let cache: ThreadSafeCache<String, i32> = ThreadSafeCache::new(3);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        cache.invalidate(&"a".to_string());
        assert_eq!(cache.get_cloned(&"a".to_string()), None);
        assert_eq!(cache.get_cloned(&"b".to_string()), Some(2));
        assert_eq!(cache.stats().size, 1);
    }

    #[test]
    fn test_thread_safe_cache_clear() {
        let cache: ThreadSafeCache<String, i32> = ThreadSafeCache::new(3);
        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        cache.clear();
        assert_eq!(cache.stats().size, 0);
        assert_eq!(cache.get_cloned(&"a".to_string()), None);
    }

    #[test]
    fn test_thread_safe_cache_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(ThreadSafeCache::<String, i32>::new(100));
        let mut handles = Vec::new();

        for i in 0..4 {
            let c = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                let key = format!("key_{}", i);
                c.put(key.clone(), i);
                assert_eq!(c.get_cloned(&key), Some(i));
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(cache.stats().size, 4);
    }
}
