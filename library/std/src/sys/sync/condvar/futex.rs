use crate::sync::atomic::Ordering::Relaxed;
use crate::sys::futex::Futex;
use crate::sys::sync::Mutex;
use crate::time::Duration;

// The value of the futex is simply incremented on every notification.
// This is used by `.wait()` to not miss any notifications after
// unlocking the mutex and before waiting for notifications.
pub struct Condvar(Futex);

impl Condvar {
    #[inline]
    pub const fn new() -> Self {
        Self(Futex::new(0))
    }

    // All the memory orderings here are `Relaxed`,
    // because synchronization is done by unlocking and locking the mutex.

    pub fn notify_one(&self) {
        self.0.fetch_add(1, Relaxed);
        self.0.wake();
    }

    pub fn notify_all(&self) {
        self.0.fetch_add(1, Relaxed);
        self.0.wake_all();
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        self.wait_optional_timeout(mutex, None);
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, timeout: Duration) -> bool {
        self.wait_optional_timeout(mutex, Some(timeout))
    }

    unsafe fn wait_optional_timeout(&self, mutex: &Mutex, timeout: Option<Duration>) -> bool {
        // Examine the notification counter _before_ we unlock the mutex.
        let futex_value = self.0.load(Relaxed);

        // Unlock the mutex before going to sleep.
        mutex.unlock();

        // Wait, but only if there hasn't been any
        // notification since we unlocked the mutex.
        let r = self.0.wait(futex_value, timeout);

        // Lock the mutex again.
        mutex.lock();

        r
    }
}
