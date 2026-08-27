use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{
        AtomicBool,
        Ordering::{self, Relaxed},
    },
};

use crate::{cpu, println};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    daif: u64,
}

const MAX_LOCK_ATTEMPTS: u32 = 1 << 24;

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(value),
        }
    }

    #[allow(dead_code)]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        let daif = cpu::stop_interrupts();
        let exchanged =
            self.locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed);

        match exchanged {
            Ok(_) => Some(SpinLockGuard { lock: self, daif }),
            Err(_) => {
                cpu::restore_interrupts(daif);
                None
            }
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let daif = cpu::stop_interrupts();
        let mut attempts = 0;
        loop {
            while self.locked.load(Relaxed) {
                attempts += 1;

                if attempts > MAX_LOCK_ATTEMPTS {
                    // If uart start using spinlock, this will be a problem, ignore for now
                    println!(
                        "failed to acquire lock after {} attempts",
                        MAX_LOCK_ATTEMPTS
                    );
                    crate::halt();
                }

                core::hint::spin_loop();
            }

            if self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return SpinLockGuard { lock: self, daif };
            }

            attempts += 1;
        }
    }
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        cpu::restore_interrupts(self.daif);
    }
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<'a, T> Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}
