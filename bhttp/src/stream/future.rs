use std::{
    future::Future,
    pin::{pin, Pin},
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncReadExt, TryStream, TryStreamExt};

use crate::Error;

fn noop_context() -> Context<'static> {
    use std::{
        ptr::null,
        task::{RawWaker, RawWakerVTable, Waker},
    };

    const fn noop_raw_waker() -> RawWaker {
        unsafe fn noop_clone(_data: *const ()) -> RawWaker {
            noop_raw_waker()
        }

        unsafe fn noop(_data: *const ()) {}

        const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);
        RawWaker::new(null(), &NOOP_WAKER_VTABLE)
    }

    pub fn noop_waker_ref() -> &'static Waker {
        struct SyncRawWaker(RawWaker);
        unsafe impl Sync for SyncRawWaker {}

        static NOOP_WAKER_INSTANCE: SyncRawWaker = SyncRawWaker(noop_raw_waker());

        // SAFETY: `Waker` is #[repr(transparent)] over its `RawWaker`.
        unsafe { &*(std::ptr::addr_of!(NOOP_WAKER_INSTANCE.0).cast()) }
    }

    Context::from_waker(noop_waker_ref())
}

/// Drives the given future (`f`) until it resolves.
/// Executes the indicated function (`p`) each time the
/// poll returned `Poll::Pending`.
pub trait SyncResolve {
    type Output;

    fn sync_resolve(&mut self) -> Self::Output {
        self.sync_resolve_with(|_| {})
    }

    fn sync_resolve_with<P: Fn(Pin<&mut Self>)>(&mut self, p: P) -> Self::Output;
}

impl<F: Future + Unpin> SyncResolve for F {
    type Output = F::Output;

    fn sync_resolve_with<P: Fn(Pin<&mut Self>)>(&mut self, p: P) -> Self::Output {
        let mut cx = noop_context();
        let mut fut = Pin::new(self);
        let mut v = fut.as_mut().poll(&mut cx);
        while v.is_pending() {
            p(fut.as_mut());
            v = fut.as_mut().poll(&mut cx);
        }
        if let Poll::Ready(v) = v {
            v
        } else {
            unreachable!();
        }
    }
}

pub trait SyncCollect {
    type Item;

    fn sync_collect(self) -> Result<Vec<Self::Item>, Error>;
}

impl<S: TryStream<Error = Error>> SyncCollect for S {
    type Item = S::Ok;

    fn sync_collect(self) -> Result<Vec<Self::Item>, Error> {
        pin!(self.try_collect::<Vec<_>>()).sync_resolve()
    }
}

pub trait SyncRead {
    fn sync_read_exact(&mut self, amount: usize) -> Vec<u8>;
    fn sync_read_to_end(&mut self) -> Vec<u8>;
}

impl<S: AsyncRead + Unpin> SyncRead for S {
    fn sync_read_exact(&mut self, amount: usize) -> Vec<u8> {
        let mut buf = vec![0; amount];
        let res = self.read_exact(&mut buf[..]);
        pin!(res).sync_resolve().unwrap();
        buf
    }

    fn sync_read_to_end(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        let res = self.read_to_end(&mut buf);
        pin!(res).sync_resolve().unwrap();
        buf
    }
}
