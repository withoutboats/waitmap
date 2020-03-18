use std::fmt::{Debug, Error, Formatter};
use std::task::Waker;

use smallvec::SmallVec;

pub struct WakerSet {
    wakers: SmallVec<[Option<Waker>; 1]>,
}

impl Debug for WakerSet {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), Error> {
        write!(fmt, "waker count: {}", self.len())
    }
}

impl WakerSet {
    pub fn new() -> WakerSet {
        WakerSet {
            wakers: SmallVec::new(),
        }
    }

    /// Return the number of `Wakers` in the set.
    pub fn len(&self) -> usize {
        self.wakers.len()
    }

    pub fn replace(&mut self, waker: Waker, idx: &mut usize) {
        let len = self.len();
        if *idx >= len {
            debug_assert!(len != std::usize::MAX); // usize::MAX is used as a sentinel
            *idx = len;
            self.wakers.push(Some(waker));
        } else {
            self.wakers[*idx] = Some(waker);
        }
    }

    pub fn remove(&mut self, idx: usize) {
        self.wakers[idx] = None;
    }

    pub fn wake(self) {
        for waker in self.wakers {
            if let Some(waker) = waker {
                waker.wake()
            }
        }
    }
}
