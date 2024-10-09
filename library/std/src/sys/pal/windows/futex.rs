use core::ffi::c_void;
use core::sync::atomic::{
    AtomicBool, AtomicI16, AtomicI32, AtomicI64, AtomicI8, AtomicIsize, AtomicPtr, AtomicU16,
    AtomicU32, AtomicU64, AtomicU8, AtomicUsize,
};
use core::time::Duration;
use core::{mem, ptr};

use super::api::{self, WinError};
use crate::sys::{c, dur2timeout};
use crate::sys::sync::Futex;

/// An atomic for use as a futex that is at least 8-bits but may be larger.
pub type SmallAtomic = AtomicU8;
/// Must be the underlying type of SmallAtomic
pub type SmallPrimitive = u8;

pub unsafe trait Waitable {
    type Atomic;
}
macro_rules! unsafe_waitable_int {
    ($(($int:ty, $atomic:ty)),*$(,)?) => {
        $(
            unsafe impl Waitable for $int {
                type Atomic = $atomic;
            }
        )*
    };
}
unsafe_waitable_int! {
    (bool, AtomicBool),
    (i8, AtomicI8),
    (i16, AtomicI16),
    (i32, AtomicI32),
    (i64, AtomicI64),
    (isize, AtomicIsize),
    (u8, AtomicU8),
    (u16, AtomicU16),
    (u32, AtomicU32),
    (u64, AtomicU64),
    (usize, AtomicUsize),
}
unsafe impl<T> Waitable for *const T {
    type Atomic = AtomicPtr<T>;
}
unsafe impl<T> Waitable for *mut T {
    type Atomic = AtomicPtr<T>;
}

pub fn wait_on_address<W: Waitable>(
    address: &W::Atomic,
    compare: W,
    timeout: Option<Duration>,
) -> bool {
    unsafe {
        let addr = ptr::from_ref(address).cast::<c_void>();
        let size = mem::size_of::<W>();
        let compare_addr = (&raw const compare).cast::<c_void>();
        let timeout = timeout.map(dur2timeout).unwrap_or(c::INFINITE);
        c::WaitOnAddress(addr, compare_addr, size, timeout) == c::TRUE
    }
}

pub fn wake_by_address_single<W: Waitable>(address: &W::Atomic) {
    unsafe {
        let addr = ptr::from_ref(address).cast::<c_void>();
        c::WakeByAddressSingle(addr);
    }
}

pub fn wake_by_address_all<W: Waitable>(address: &W::Atomic) {
    unsafe {
        let addr = ptr::from_ref(address).cast::<c_void>();
        c::WakeByAddressAll(addr);
    }
}

impl<W: Waitable> Futex for W::Atomic {
    pub fn wait(&self, expected: W, timeout: Option<Duration>) -> bool {
        // return false only on timeout
        wait_on_address(self, expected, timeout) || api::get_last_error() != WinError::TIMEOUT
    }

    pub fn wake(&self) -> bool {
        wake_by_address_single(self);
        false
    }

    pub fn wake_all(&self) {
        wake_by_address_all(self)
    }
}
