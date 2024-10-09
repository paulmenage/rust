mod condvar;
mod mutex;
mod once;
mod once_box;
mod rwlock;
mod thread_parking;

pub use condvar::Condvar;
pub use mutex::Mutex;
pub use once::{Once, OnceState};
#[allow(unused)] // Only used on some platforms.
use once_box::OnceBox;
pub use rwlock::RwLock;
pub use thread_parking::Parker;

use core::time::Duration;

/// A trait that provides futex semantics for `std::sys::futex::{Atomic,
/// SmallAtomic}`.  Generally implemented on an `AtomicU<N>` type, but
/// this allows the futex implementation to abstract the type more if
/// necessary.
pub(crate) trait Futex {
    /// If the value of the futex object does not equal `expected`,
    /// sleep until woken by `wake()` or `wake_all()`.  Returns false on
    /// timeout, and true in all other cases.
    fn wait(&self, expected: u32, timeout: Option<Duration>) -> bool;

    /// Wakes up one thread that's waiting on this futex.
    ///
    /// May return true if this actually woke up such a thread, but some
    /// platforms always return false; must return false if no thread
    /// was waiting on this futex.
    fn wake(&self) -> bool;

    /// Wakes up all threads that are waiting on this futex.
    fn wake_all(&self);
}
