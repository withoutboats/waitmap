use std::collections::hash_map::RandomState;
use std::hash::{Hash, BuildHasher};
use std::mem;

use dashmap::mapref::entry;
use dashmap::mapref::one;

use crate::{WaitEntry, RefMut};

use Entry::*;
use crate::WaitEntry::*;
use InnerVacantEntry::*;

pub enum Entry<'a, K, V, S = RandomState> {
    Occupied(OccupiedEntry<'a, K, V, S>),
    Vacant(VacantEntry<'a, K, V, S>),
}

impl<'a, K: Hash + Eq, V, S: BuildHasher> Entry<'a, K, V, S> {
    pub(crate) fn vacant(slot: entry::VacantEntry<'a, K, WaitEntry<V>, S>) -> Self {
        Vacant(VacantEntry {
            inner: ReallyVacant(slot),
        })
    }

    pub(crate) fn occupied(entry: entry::OccupiedEntry<'a, K, WaitEntry<V>, S>) -> Self {
        Occupied(OccupiedEntry {
            inner: entry,
        })
    }

    pub(crate) fn waiting(entry: entry::OccupiedEntry<'a, K, WaitEntry<V>, S>) -> Self {
        Vacant(VacantEntry {
            inner: WaitingVacant(entry.into_ref()),
        })
    }

    pub fn and_modify(mut self, f: impl FnOnce(&mut V)) -> Self {
        if let Occupied(entry) = &mut self {
            f(entry.get_mut());
        }
        self
    }

    pub fn key(&self) -> &K {
        match self {
            Occupied(entry) => entry.key(),
            Vacant(entry)   => entry.key(),
        }
    }

    pub fn or_default(self) -> RefMut<'a, K, V, S> where V: Default {
        self.or_insert_with(V::default)
    }

    pub fn or_insert(self, value: V) -> RefMut<'a, K, V, S> {
        self.or_insert_with(|| value)
    }

    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> RefMut<'a, K, V, S> {
        match self {
            Occupied(entry) => entry.into_ref(),
            Vacant(entry)   => entry.insert(f()),
        }
    }

    pub fn or_try_insert_with<E>(self, f: impl FnOnce() -> Result<V, E>)
        -> Result<RefMut<'a, K, V, S>, E>
    {
        match self {
            Occupied(entry) => Ok(entry.into_ref()),
            Vacant(entry)   => Ok(entry.insert(f()?)),
        }
    }
}

pub struct OccupiedEntry<'a, K, V, S> {
    inner: entry::OccupiedEntry<'a, K, WaitEntry<V>, S>,
}


impl<'a, K: Hash + Eq, V, S: BuildHasher> OccupiedEntry<'a, K, V, S> {
    pub fn get(&self) -> &V {
        self.inner.get().value()
    }

    pub fn get_mut(&mut self) -> &mut V {
        self.inner.get_mut().value_mut()
    }

    pub fn insert(&mut self, value: V) -> V {
        self.inner.insert(Filled(value)).into_value()
    }

    pub fn into_ref(self) -> RefMut<'a, K, V, S> {
        RefMut { inner: self.inner.into_ref() }
    }

    pub fn key(&self) -> &K {
        self.inner.key()
    }

    pub fn remove(self) -> V {
        self.inner.remove().into_value()
    }

    pub fn remove_entry(self) -> (K, V) {
        let (key, entry) = self.inner.remove_entry();
        (key, entry.into_value())
    }

    pub fn replace_entry(self, value: V) -> (K, V) {
        let (key, entry) = self.inner.replace_entry(Filled(value));
        (key, entry.into_value())
    }
}

pub struct VacantEntry<'a, K, V, S> {
    inner: InnerVacantEntry<'a, K, V, S>,
}

enum InnerVacantEntry<'a, K, V, S> {
    ReallyVacant(entry::VacantEntry<'a, K, WaitEntry<V>, S>),
    WaitingVacant(one::RefMut<'a, K, WaitEntry<V>, S>),
}

impl<'a, K: Hash + Eq, V, S: BuildHasher> VacantEntry<'a, K, V, S> {
    pub fn insert(self, value: V) -> RefMut<'a, K, V, S> {
        match self.inner {
            ReallyVacant(slot)          => RefMut { inner: slot.insert(Filled(value)) },
            WaitingVacant(mut entry)    => {
                if let Waiting(wakers) = mem::replace(entry.value_mut(), Filled(value)) {
                    wakers.wake();
                    RefMut { inner: entry }
                } else { panic!() }
            }
        }
    }

    pub fn key(&self) -> &K {
        match &self.inner {
            ReallyVacant(slot)      => slot.key(),
            WaitingVacant(entry)    => entry.key(),
        }
    }
}
