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

pub struct WaitMap<K, V, S = RandomState> {
    map: DashMap<K, WaitEntry<V>, S>,
}

impl<K: Hash + Eq, V> WaitMap<K, V> {
    pub fn new() -> WaitMap<K, V> {
        WaitMap { map: DashMap::with_hasher(RandomState::default()) }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher + Clone> WaitMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> WaitMap<K, V, S> {
        WaitMap { map: DashMap::with_hasher(hasher) }
    }

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

    pub fn wait<'a, Q: ?Sized + Hash + Eq>(&'a self, qey: &'a Q)
        -> impl Future<Output = Option<Ref<'_, K, V, S>>> + 'a
    where
        K: Borrow<Q> + From<&'a Q>,
    {
        let key = K::from(qey);
        self.map.entry(key).or_insert(Waiting(WakerSet::new()));
        Wait::new(&self.map, qey)
    }

    pub fn wait_mut<'a, Q: ?Sized + Hash + Eq>(&'a self, qey: &'a Q)
        -> impl Future<Output = Option<RefMut<'_, K, V, S>>> + 'a
    where
        K: Borrow<Q> + From<&'a Q>,
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
