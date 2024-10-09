use super::hermit_abi;
use crate::ptr::null;
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
    fn wait(&self, expected: u32, timeout: Option<Duration>) -> bool {
        // Calculate the timeout as a relative timespec.
        //
        // Overflows are rounded up to an infinite timeout (None).
        let timespec = timeout.and_then(|dur| {
            Some(hermit_abi::timespec {
                tv_sec: dur.as_secs().try_into().ok()?,
                tv_nsec: dur.subsec_nanos().try_into().ok()?,
            })
        });

        let r = unsafe {
            hermit_abi::futex_wait(
                self.as_ptr(),
                expected,
                timespec.as_ref().map_or(null(), |t| t as *const hermit_abi::timespec),
                hermit_abi::FUTEX_RELATIVE_TIMEOUT,
            )
        };

        r != -hermit_abi::errno::ETIMEDOUT
    }

    #[inline]
    fn wake(&self) -> bool {
        unsafe { hermit_abi::futex_wake(futex.as_ptr(), 1) > 0 }
    }

    #[inline]
    fn wake_all(&self) {
        unsafe {
            hermit_abi::futex_wake(self.as_ptr(), i32::MAX);
        }
    }
}
