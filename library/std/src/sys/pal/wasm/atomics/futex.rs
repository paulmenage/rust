#[cfg(target_arch = "wasm32")]
use core::arch::wasm32 as wasm;
#[cfg(target_arch = "wasm64")]
use core::arch::wasm64 as wasm;

use crate::sync::atomic::AtomicU32;
use crate::sys::sync::Futex;
use crate::time::Duration;

/// An atomic for use as a futex that is at least 32-bits but may be larger
pub type Atomic = AtomicU32;
/// Must be the underlying type of Atomic
pub type Primitive = u32;

/// An atomic for use as a futex that is at least 8-bits but may be larger.
pub type SmallAtomic = AtomicU32;
/// Must be the underlying type of SmallAtomic
pub type SmallPrimitive = u32;

impl Futex for AtomicU32 {
    /// Wait for a futex_wake operation to wake us.
    ///
    /// Returns directly if the futex doesn't hold the expected value.
    ///
    /// Returns false on timeout, and true in all other cases.
    fn wait(&self, expected: u32, timeout: Option<Duration>) -> bool {
        let timeout = timeout.and_then(|t| t.as_nanos().try_into().ok()).unwrap_or(-1);
        unsafe {
            wasm::memory_atomic_wait32(
                self as *const AtomicU32 as *mut i32,
                expected as i32,
                timeout,
            ) < 2
        }
    }

    /// Wakes up one thread that's blocked on `futex_wait` on this futex.
    ///
    /// Returns true if this actually woke up such a thread,
    /// or false if no thread was waiting on this futex.
    fn wake(&self) -> bool {
        unsafe { wasm::memory_atomic_notify(self as *const AtomicU32 as *mut i32, 1) > 0 }
    }

    /// Wakes up all threads that are waiting on `futex_wait` on this futex.
    fn wake_all(&self) {
        unsafe {
            wasm::memory_atomic_notify(self as *const AtomicU32 as *mut i32, i32::MAX as u32);
        }
    }
}
