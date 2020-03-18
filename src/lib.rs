//! Async concurrent hashmap built on top of [dashmap](https://docs.rs/dashmap/).
//!
//! # Wait
//! [`WaitMap`](crate::WaitMap) is a concurrent hashmap with an asynchronous `wait` operation.
//! ```
//! # extern crate async_std;
//! # extern crate waitmap;
//! # use async_std::main;
//! # use waitmap::WaitMap;
//! # #[async_std::main]
//! # async fn main() -> std::io::Result<()> {
//! let map: WaitMap<String, i32> = WaitMap::new();
//! # map.insert(String::from("Rosa Luxemburg"), 1);
//!
//! // This will wait until a value is put under the key "Rosa Luxemburg"
//! if let Some(value) = map.wait("Rosa Luxemburg").await {
//!     // ...
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Waits are cancellable. Cancelled waits evaluate to `None`.
//! ```
//! # extern crate async_std;
//! # extern crate waitmap;
//! # use async_std::{main, task};
//! # use std::time::Duration;
//! # use std::sync::Arc;
//! # use waitmap::WaitMap;
//! # #[async_std::main]
//! # async fn main() -> std::io::Result<()> {
//! let map: Arc<WaitMap<String, String>> = Arc::new(WaitMap::new());
//! let map1 = map.clone();
//!
//! let handle = task::spawn(async move {
//!     let result = map.wait("Voltairine de Cleyre").await;
//!     assert!(result.is_none());
//! });
//!
//! task::spawn(async move {
//!     task::sleep(Duration::from_millis(100)).await; // avoid deadlock
//!     map1.cancel("Voltairine de Cleyre");
//! });
//!
//! task::block_on(handle);
//! # Ok(())
//! # }
//! ```

mod wait;
mod waker_set;

use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{Hash, BuildHasher};
use std::mem;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry::*;
use dashmap::mapref::one;

use WaitEntry::*;
use wait::{Wait, WaitMut};
use waker_set::WakerSet;

/// An asynchronous concurrent hashmap.
pub struct WaitMap<K, V, S = RandomState> {
    map: DashMap<K, WaitEntry<V>, S>,
}

impl<K: Hash + Eq, V> WaitMap<K, V> {
    /// Make a new `WaitMap` using the default hasher.
    pub fn new() -> WaitMap<K, V> {
        WaitMap { map: DashMap::with_hasher(RandomState::default()) }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher + Clone> WaitMap<K, V, S> {
    /// Make a new `WaitMap` using a custom hasher.
    /// ```
    /// # extern crate async_std;
    /// # extern crate waitmap;
    /// # use async_std::main;
    /// # use waitmap::WaitMap;
    /// use std::collections::hash_map::RandomState;
    /// # #[async_std::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let map: WaitMap<i32, String> = WaitMap::with_hasher(RandomState::new());
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_hasher(hasher: S) -> WaitMap<K, V, S> {
        WaitMap { map: DashMap::with_hasher(hasher) }
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If there are any pending `wait` calls for this key, they are woken up.
    ///
    /// If the map did have this key present, the value is updated and the old value is returned.
    /// ```
    /// # extern crate async_std;
    /// # extern crate waitmap;
    /// # use async_std::{main, sync::Arc, prelude::*};
    /// # use waitmap::WaitMap;
    /// # #[async_std::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let map: Arc<WaitMap<String, i32>> = Arc::new(WaitMap::new());
    ///
    /// let insert_fut = async { map.insert("hi".to_string(), 0) };
    /// let wait_fut = map.wait("hi");
    ///
    /// let (insert_res, wait_res) = insert_fut.join(wait_fut).await;
    /// assert!(insert_res.is_none());
    /// assert!(wait_res.is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        match self.map.entry(key) {
            Occupied(mut entry)  => {
                match mem::replace(entry.get_mut(), Filled(value)) {
                    Waiting(wakers) => {
                        drop(entry); // drop early to release lock before waking other tasks
                        wakers.wake();
                        None
                    }
                    Filled(value)   => Some(value),
                }
            }
            Vacant(slot)     => {
                slot.insert(Filled(value));
                None
            }
        }
    }

    pub fn get<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<Ref<'_, K, V, S>>
        where K: Borrow<Q>
    {
        Some(Ref { inner: self.map.get(key)? })
    }

    pub fn get_mut<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> Option<RefMut<'_, K, V, S>>
        where K: Borrow<Q>
    {
        Some(RefMut { inner: self.map.get_mut(key)? })
    }

    pub fn wait<'a: 'f, 'b: 'f, 'f, Q: ?Sized + Hash + Eq>(&'a self, qey: &'b Q)
        -> impl Future<Output = Option<Ref<'a, K, V, S>>> + 'f
    where
        K: Borrow<Q> + From<&'b Q>,
    {
        let key = K::from(qey);
        self.map.entry(key).or_insert(Waiting(WakerSet::new()));
        Wait::new(&self.map, qey)
    }

    pub fn wait_mut<'a: 'f, 'b: 'f, 'f, Q: ?Sized + Hash + Eq>(&'a self, qey: &'b Q)
        -> impl Future<Output = Option<RefMut<'a, K, V, S>>> + 'f
    where
        K: Borrow<Q> + From<&'b Q>,
    {
        let key = K::from(qey);
        self.map.entry(key).or_insert(Waiting(WakerSet::new()));
        WaitMut::new(&self.map, qey)
    }

    pub fn cancel<Q: ?Sized + Hash + Eq>(&self, key: &Q) -> bool 
        where K: Borrow<Q>
    {
        if let Some((_, entry)) = self.map.remove_if(key, |_, entry| {
            if let Waiting(_) = entry { true } else { false }
        }) {
            if let Waiting(wakers) = entry {
                wakers.wake();
            }
            true
        } else { false }
    }

    /// Cancels all outstanding `waits` on the map.
    /// ```
    /// # extern crate async_std;
    /// # extern crate waitmap;
    /// # use async_std::{main, stream, prelude::*};
    /// # use waitmap::WaitMap;
    /// # #[async_std::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let map: WaitMap<String, i32> = WaitMap::new();
    /// let mut waitstream =
    ///     stream::from_iter(vec![map.wait("we"), map.wait("are"), map.wait("waiting")]);
    ///
    /// map.cancel_all();
    ///
    /// let mut num_cancelled = 0;
    /// while let Some(wait_fut) = waitstream.next().await {
    ///     assert!(wait_fut.await.is_none());
    ///     num_cancelled += 1;
    /// }
    ///
    /// assert!(num_cancelled == 3);
    /// # Ok(())
    /// # }
    /// ```
    pub fn cancel_all(&self) {
        self.map.retain(|_, entry| {
            if let Waiting(wakers) = entry {
                // NB: In theory, there is a deadlock risk: if a task is awoken before the
                // retain is completed, it may see a waiting entry with an empty waker set,
                // rather than a missing entry.
                //
                // However, this is prevented by the memory guards already present in DashMap.
                // No other task will be able to view this entry until the guard on this shard
                // has been dropped, which will not occur until this shard's unretained members
                // have actually been removed.
                mem::replace(wakers, WakerSet::new()).wake();
                false
            } else { true }
        })
    }
}

enum WaitEntry<V> {
    Waiting(WakerSet),
    Filled(V),
}

/// A shared reference to a `WaitMap` key-value pair.
/// ```
/// # extern crate async_std;
/// # extern crate waitmap;
/// # use async_std::main;
/// # use waitmap::{Ref, WaitMap};
/// # #[async_std::main]
/// # async fn main() -> std::io::Result<()> {
/// let map: WaitMap<String, i32> = WaitMap::new();
/// let emma = "Emma Goldman".to_string();
///
/// map.insert(emma.clone(), 0);
/// let kv: Ref<String, i32, _> = map.get(&emma).unwrap();
///
/// assert!(*kv.key() == emma);
/// assert!(*kv.value() == 0);
/// assert!(kv.pair() == (&"Emma Goldman".to_string(), &0));
/// # Ok(())
/// # }
/// ```
pub struct Ref<'a, K, V, S> {
    inner: one::Ref<'a, K, WaitEntry<V>, S>,
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> Ref<'a, K, V, S> {
    pub fn key(&self) -> &K {
        self.inner.key()
    }

    pub fn value(&self) -> &V {
        match self.inner.value() {
            Filled(value)   => value,
            _               => panic!()
        }
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.key(), self.value())
    }
}

/// An exclusive reference to a `WaitMap` key-value pair.
pub struct RefMut<'a, K, V, S> {
    inner: one::RefMut<'a, K, WaitEntry<V>, S>,
}

impl<'a, K: Eq + Hash, V, S: BuildHasher> RefMut<'a, K, V, S> {
    pub fn key(&self) -> &K {
        self.inner.key()
    }

    pub fn value(&self) -> &V {
        match self.inner.value() {
            Filled(value)   => value,
            _               => panic!()
        }
    }

    pub fn value_mut(&mut self) -> &mut V {
        match self.inner.value_mut() {
            Filled(value)   => value,
            _               => panic!()
        }
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.key(), self.value())
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        match self.inner.pair_mut() {
            (key, Filled(value))    => (key, value),
            _                       => panic!(),
        }
    }
}
