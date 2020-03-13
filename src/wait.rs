use std::borrow::Borrow;
use std::future::Future;
use std::hash::{Hash, BuildHasher};
use std::pin::Pin;
use std::task::{Context, Poll};

use dashmap::DashMap;

use crate::WaitEntry;
use crate::WaitEntry::*;
use crate::{Ref, RefMut};

pub struct Wait<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    map: &'a DashMap<K, WaitEntry<V>, S>,
    key: &'b Q,
    idx: usize,
}

impl<'a, 'b, K, V, S, Q> Wait<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    pub(crate) fn new(map: &'a DashMap<K, WaitEntry<V>, S>, key: &'b Q) -> Self {
        Wait { map, key, idx: std::usize::MAX }
    }
}

impl<'a, 'b, K, V, S, Q> Future for Wait<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    type Output = Option<Ref<'a, K, V, S>>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.map.get_mut(self.key) {
            Some(mut entry) => match entry.value_mut() {
                Waiting(wakers)  => {
                    wakers.replace(ctx.waker().clone(), &mut self.idx);
                    Poll::Pending
                }
                Filled(_)        => {
                    let inner = entry.downgrade();
                    self.idx = std::usize::MAX;
                    Poll::Ready(Some(Ref { inner }))
                }
            }
            None        => Poll::Ready(None),
        }
    }
}

impl<'a, 'b, K, V, S, Q> Drop for Wait<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    fn drop(&mut self) {
        if self.idx == std::usize::MAX { return; }
        if let Some(mut entry) = self.map.get_mut(self.key) {
            if let Waiting(wakers) = entry.value_mut() {
                wakers.remove(self.idx);
            }
        }
    }
}

pub struct WaitMut<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    map: &'a DashMap<K, WaitEntry<V>, S>,
    key: &'b Q,
    idx: usize,
}

impl<'a, 'b, K, V, S, Q> WaitMut<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    pub(crate) fn new(map: &'a DashMap<K, WaitEntry<V>, S>, key: &'b Q) -> Self {
        WaitMut { map, key, idx: std::usize::MAX }
    }
}

impl<'a, 'b, K, V, S, Q> Future for WaitMut<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    type Output = Option<RefMut<'a, K, V, S>>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.map.get_mut(self.key) {
            Some(mut entry) => match entry.value_mut() {
                Waiting(wakers)  => {
                    wakers.replace(ctx.waker().clone(), &mut self.idx);
                    Poll::Pending
                }
                Filled(_)        => {
                    self.idx = std::usize::MAX;
                    Poll::Ready(Some(RefMut { inner: entry }))
                }
            }
            None        => Poll::Ready(None),
        }
    }
}

impl<'a, 'b, K, V, S, Q> Drop for WaitMut<'a, 'b, K, V, S, Q> where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    fn drop(&mut self) {
        if self.idx == std::usize::MAX { return; }
        if let Some(mut entry) = self.map.get_mut(self.key) {
            if let Waiting(wakers) = entry.value_mut() {
                wakers.remove(self.idx);
            }
        }
    }
}
