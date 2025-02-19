//! Least Recently Used (LRU) cache.

#![feature(extract_if)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::{collections::LinkedList, sync::Arc};
use mutex_api::Mutex;

/// Least Recently Used (LRU) cache.
#[derive(Debug)]
pub struct Lru<LruMutex>(LruMutex);

impl<LruMutex, K, V> Lru<LruMutex>
where
    LruMutex: Mutex<Data = LruMap<K, V>>,
{
    /// Creates a new LRU cache with the given size.
    pub fn new(size: usize) -> Self
    where
        V: Default,
    {
        Self(LruMutex::new(LruMap::new(size)))
    }

    /// Returns a reference to the cached value associated with the key.
    ///
    /// If the value is cached, returns a reference to it.
    /// If the value is not cached, recycles the least recently used (LRU) unreferenced cache and returns a reference to it.
    /// If all values are referenced, returns `None`.
    ///
    /// When returned value is dropped, the key is promoted to the most recently used (MRU) position.
    pub fn get(&self, key: K) -> Option<LruValue<LruMutex, K, V>>
    where
        K: PartialEq + Clone,
    {
        self.0.lock().get(key, &self.0)
    }
}

/// An key-value maps of Least Recently Used (LRU) cache.
#[derive(Default)]
pub struct LruMap<K, V> {
    list: LinkedList<(Option<K>, Arc<V>)>,
}

impl<K, V> LruMap<K, V> {
    /// Creates a new `LruMap` with the given size.
    fn new(size: usize) -> Self
    where
        V: Default,
    {
        assert!(size > 0);

        let mut list = LinkedList::new();
        for _ in 0..size {
            list.push_back((None, Arc::new(V::default())));
        }
        Self { list }
    }

    /// Returns a reference to the cached value associated with the key.
    ///
    /// If the value is cached, returns a reference to the value.
    /// If the value is not cached, recycles the least recently used (LRU) value.
    /// If all buffers are in use, returns `None`.
    ///
    /// When returned value is dropped, the key is promoted to the most recently used (MRU) position.
    fn get<'a, LruMutex>(
        &mut self,
        key: K,
        list: &'a LruMutex,
    ) -> Option<LruValue<'a, LruMutex, K, V>>
    where
        LruMutex: Mutex<Data = Self>,
        K: PartialEq + Clone,
    {
        // Find the value with the key
        if let Some((_k, v)) = self.list.iter().find(|(k, _v)| k.as_ref() == Some(&key)) {
            return Some(LruValue {
                list,
                key,
                value: Arc::clone(v),
            });
        }

        // Not cached
        // Recycle the least recently used value.
        if let Some(buf) = self.list.iter_mut().rev().find_map(|(k, v)| {
            if Arc::strong_count(v) == 1 {
                *k = Some(key.clone());
                Some((k, v))
            } else {
                None
            }
        }) {
            return Some(LruValue {
                list,
                key,
                value: Arc::clone(buf.1),
            });
        }

        None
    }

    /// Promotes the cached value associated with the key to the most recently used (MRU) position.
    fn promote(&mut self, key: &K)
    where
        K: PartialEq,
    {
        let buf = self
            .list
            .extract_if(|(k, _v)| k.as_ref() == Some(key))
            .next();
        if let Some((k, v)) = buf {
            self.list.push_front((k, v))
        }
    }
}

/// A reference to the cached value associated with the key.
#[derive(Debug)]
pub struct LruValue<'list, LruMutex, K, V>
where
    LruMutex: Mutex<Data = LruMap<K, V>>,
    K: PartialEq,
{
    list: &'list LruMutex,
    key: K,
    value: Arc<V>,
}

impl<LruMutex, K, V> Drop for LruValue<'_, LruMutex, K, V>
where
    LruMutex: Mutex<Data = LruMap<K, V>>,
    K: PartialEq,
{
    fn drop(&mut self) {
        self.list.lock().promote(&self.key);
    }
}

impl<LruMutex, K, V> LruValue<'_, LruMutex, K, V>
where
    LruMutex: Mutex<Data = LruMap<K, V>>,
    K: PartialEq,
{
    /// Returns the cache key.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Returns a reference to the cached value.
    pub fn value(&self) -> &V {
        &self.value
    }

    /// Pins the cached value to prevent it from being evicted.
    ///
    /// The value will not be evicted until it is unpinned and no references are held.
    pub fn pin(&self) {
        let _ = Arc::into_raw(Arc::clone(&self.value));
    }

    /// Unpins the cached value, allowing it to be evicted.
    ///
    /// The value will be evicted if it is not pinned and no references are held.
    ///
    /// # Safety
    ///
    /// This function should only be called if [`Self::pin()`] was previously called.
    /// Calling [`Self::unpin()`] more times than [`Self::pin()`] may cause a use-after-free error.
    pub unsafe fn unpin(&self) {
        // clones arc and get pointer
        let ptr = Arc::into_raw(Arc::clone(&self.value));
        unsafe {
            // drops two times -> decrements pin
            let _ = Arc::from_raw(ptr);
            let _ = Arc::from_raw(ptr);
        };
    }

    /// Gets the number of references to this data.
    pub fn pin_count(&self) -> usize {
        // subtract 1 because LruList always owns the reference
        Arc::strong_count(&self.value) - 1
    }
}

impl<LruMutex, K, V> Clone for LruValue<'_, LruMutex, K, V>
where
    LruMutex: Mutex<Data = LruMap<K, V>>,
    K: PartialEq + Clone,
{
    fn clone(&self) -> Self {
        Self {
            list: self.list,
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    #[test]
    fn test_lru_new() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(3);
        assert_eq!(lru.0.lock().unwrap().list.len(), 3);
    }

    #[test]
    fn test_lru_get_and_evict() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(2);
        assert!(lru.get(1).is_some());
        assert!(lru.get(2).is_some());
        assert!(lru.get(3).is_some());
    }

    #[test]
    fn test_lru_get_never_evict_referenced() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(2);

        let _c1 = lru.get(1).unwrap();
        let c2 = lru.get(2).unwrap();
        assert!(lru.get(3).is_none());

        // after drop reference, new entry can be obtained
        drop(c2);
        assert!(lru.get(3).is_some());
    }

    #[test]
    fn test_lru_get_same_cache() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(2);
        let _c1 = lru.get(1).unwrap();
        let _c2 = lru.get(2).unwrap();
        assert!(lru.get(1).is_some());
        assert!(lru.get(2).is_some());
        assert!(lru.get(3).is_none());
    }

    #[test]
    fn test_lru_promote() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(3);
        let value1 = lru.get(1).unwrap();
        let value2 = lru.get(2).unwrap();
        let value3 = lru.get(3).unwrap();
        drop(value1);
        assert_eq!(
            lru.0.lock().unwrap().list.front().as_ref().unwrap().0,
            Some(1)
        );
        drop(value2);
        assert_eq!(
            lru.0.lock().unwrap().list.front().as_ref().unwrap().0,
            Some(2)
        );
        drop(value3);
        assert_eq!(
            lru.0.lock().unwrap().list.front().as_ref().unwrap().0,
            Some(3)
        );
    }

    #[test]
    fn test_lru_pin_unpin() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(1);
        let value = lru.get(1).unwrap();
        assert_eq!(value.pin_count(), 1);
        value.pin();
        assert_eq!(value.pin_count(), 2);
        unsafe { value.unpin() };
        assert_eq!(value.pin_count(), 1);
    }

    #[test]
    fn test_lru_clone() {
        let lru: Lru<Mutex<LruMap<i32, i32>>> = Lru::new(1);
        let value = lru.get(1).unwrap();
        let value_clone = value.clone();
        assert_eq!(value.pin_count(), 2);
        drop(value);
        assert_eq!(value_clone.pin_count(), 1);
    }
}
