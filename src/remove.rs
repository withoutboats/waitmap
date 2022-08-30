use std::borrow::Borrow;
use std::future::Future;
use std::hash::{BuildHasher, Hash};
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use dashmap::DashMap;
use crate::{Filled, WaitEntry, Waiting};

/// A Future created by `WaitMap::remove_wait`. The future resolves to the the (Key, Value) as a
/// move when the value is available.
pub struct Remove<'a, 'b, K, V, S, Q> where K: Hash + Eq + Borrow<Q>,
                                            S: BuildHasher + Clone,
                                            Q: ?Sized + Hash + Eq, {
    // We need the mutex here because we *will* be modifying the map, it's possible that some one is
    // checking the variant of the entry while we're removing it! It might make more sense if you
    // look at the Future impl.
    map: Mutex<&'a DashMap<K, WaitEntry<V>, S>>,
    key: &'b Q,

    /// The index of the waker in the waker set.
    /// Note:
    /// If the index is `usize::MAX`, then the waker is not in the set.
    idx: usize,
}

impl<'a, 'b, K, V, S, Q> Remove<'a, 'b, K, V, S, Q> where K: Hash + Eq + Borrow<Q>,
                                                          S: BuildHasher + Clone,
                                                          Q: ?Sized + Hash + Eq, {
    pub(crate) fn new(map: &'a DashMap<K, WaitEntry<V>, S>, key: &'b Q) -> Self {
        Remove {
            map: Mutex::new(map),
            key,
            idx: usize::MAX,
        }
    }
}

impl<'a, 'b, K, V, S, Q> Future for Remove<'a, 'b, K, V, S, Q> where K: Hash + Eq + Borrow<Q>,
                                                                     S: BuildHasher + Clone,
                                                                     Q: ?Sized + Hash + Eq, {
    type Output = Option<(K, V)>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        // we need to hold an exclusive lock since some one might be checking the variant of the entry
        // while we're removing it!
        let this = self.get_mut();
        let map = this.map.lock().unwrap();
        let remove;

        // we need to drop the reference returned by `get_mut` to prevent a dead lock
        {
            match map.get_mut(this.key) {
                Some(mut entry) => match entry.value_mut() {
                    Waiting(wakers) => {
                        wakers.replace(ctx.waker().clone(), &mut this.idx);
                        return Poll::Pending;
                    }
                    Filled(_) => remove = true
                },
                None => return Poll::Ready(None)
            };
        }

        if remove {
            match map.remove(this.key) {
                Some((key, wait_entry)) => {
                    eprintln!("removed successfully");
                    this.idx = usize::MAX;
                    let value = match wait_entry {
                        Filled(value) => value,
                        Waiting(_) => unreachable!("we should not be here if the entry is waiting")
                    };
                    Poll::Ready(Some((key, value)))
                }
                None => {
                    Poll::Ready(None)
                }
            }
        } else {
            unreachable!("we should not be here if remove is never set")
        }
    }
}

impl<'a, 'b, K, V, S, Q> Drop for Remove<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    fn drop(&mut self) {
        if self.idx == usize::MAX { return; }
        if let Some(mut entry) = self.map.lock().unwrap().get_mut(self.key) {
            if let Waiting(wakers) = entry.value_mut() {
                wakers.remove(self.idx);
            }
        }
    }
}
