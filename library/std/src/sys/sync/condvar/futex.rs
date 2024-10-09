use crate::sync::atomic::Ordering::Relaxed;
use crate::sys::futex::Atomic;
use crate::sys::sync::Futex;
use crate::sys::sync::Mutex;
use crate::time::Duration;

pub struct Condvar {
    // The value of this atomic is simply incremented on every notification.
    // This is used by `.wait()` to not miss any notifications after
    // unlocking the mutex and before waiting for notifications.
    futex: Atomic,
}

impl Condvar {
    #[inline]
    pub const fn new() -> Self {
        Self{futex: Atomic::new(0)}
    }

    // All the memory orderings here are `Relaxed`,
    // because synchronization is done by unlocking and locking the mutex.

    pub fn notify_one(&self) {
        self.futex.fetch_add(1, Relaxed);
        self.futex.wake();
    }

    pub fn notify_all(&self) {
        self.futex.fetch_add(1, Relaxed);
        self.futex.wake_all();
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        self.wait_optional_timeout(mutex, None);
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, timeout: Duration) -> bool {
        self.wait_optional_timeout(mutex, Some(timeout))
    }

    unsafe fn wait_optional_timeout(&self, mutex: &Mutex, timeout: Option<Duration>) -> bool {
        // Examine the notification counter _before_ we unlock the mutex.
        let futex_value = self.futex.load(Relaxed);

        // Unlock the mutex before going to sleep.
        mutex.unlock();

        // Wait, but only if there hasn't been any
        // notification since we unlocked the mutex.
        let r = self.futex.wait(futex_value, timeout);

        // Lock the mutex again.
        mutex.lock();

        r
    }
}
