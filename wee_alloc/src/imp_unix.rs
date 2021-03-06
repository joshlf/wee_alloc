use alloc::allocator::{Alloc, Layout};
use const_init::ConstInit;
use core::cell::UnsafeCell;
use libc;
use mmap_alloc::MapAllocBuilder;
use units::{Bytes, Pages};

pub(crate) fn alloc_pages(pages: Pages) -> *mut u8 {
    unsafe {
        let bytes: Bytes = pages.into();
        let layout = Layout::from_size_align_unchecked(bytes.0, 1);
        // TODO: when we can detect failure of wasm intrinsics, then both
        // `alloc_pages` implementations should return results, rather than
        // panicking on failure.
        MapAllocBuilder::default()
            .build()
            .alloc(layout)
            .expect("failed to allocate page")
    }
}

// Cache line size on an i7. Good enough.
const CACHE_LINE_SIZE: usize = 64;

pub(crate) struct Exclusive<T> {
    lock: UnsafeCell<libc::pthread_mutex_t>,
    inner: UnsafeCell<T>,
    // Because we can't do `repr(align = "64")` yet, we have to pad a full cache
    // line to ensure that there is no false sharing.
    _no_false_sharing: [u8; CACHE_LINE_SIZE],
}

impl<T: ConstInit> ConstInit for Exclusive<T> {
    const INIT: Self = Exclusive {
        lock: UnsafeCell::new(libc::PTHREAD_MUTEX_INITIALIZER),
        inner: UnsafeCell::new(T::INIT),
        _no_false_sharing: [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ],
    };
}

impl<T> Exclusive<T> {
    /// Get exclusive, mutable access to the inner value.
    ///
    /// # Safety
    ///
    /// Does not assert that `pthread`s calls return OK, unless the
    /// "extra_assertions" feature is enabled. This means that if `f` re-enters
    /// this method for the same `Exclusive` instance, there will be undetected
    /// mutable aliasing, which is UB.
    #[inline]
    pub(crate) unsafe fn with_exclusive_access<'a, F, U>(&'a self, f: F) -> U
    where
        F: FnOnce(&'a mut T) -> U,
    {
        let code = libc::pthread_mutex_lock(&mut *self.lock.get());
        extra_assert_eq!(code, 0, "pthread_mutex_lock should run OK");

        let result = f(&mut *self.inner.get());

        let code = libc::pthread_mutex_unlock(&mut *self.lock.get());
        extra_assert_eq!(code, 0, "pthread_mutex_unlock should run OK");

        result
    }
}
