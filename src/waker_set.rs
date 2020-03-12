use std::task::Waker;

use smallvec::SmallVec;

pub struct WakerSet {
    wakers: SmallVec<[Option<Waker>; 1]>,
}

impl WakerSet {
    pub fn new() -> WakerSet {
        WakerSet {
            wakers: SmallVec::new(),
        }
    }

    pub fn replace(&mut self, waker: Waker, idx: &mut usize) {
        let len = self.wakers.len();
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
                println!("waking a waker");
                waker.wake()
            }
        }
    }
}
