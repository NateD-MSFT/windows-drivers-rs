use wdk_sys::{macros, NTSTATUS, WDFSPINLOCK, WDF_OBJECT_ATTRIBUTES};

use crate::nt_success;

// private module + public re-export avoids the module name repetition: https://github.com/rust-lang/rust-clippy/issues/8524
#[allow(clippy::module_name_repetitions)]

/// WDF Spin Lock.
///
/// Use framework spin locks to synchronize access to driver data from code that
/// runs at `IRQL` <= `DISPATCH_LEVEL`. When a driver thread acquires a spin
/// lock, the system sets the thread's IRQL to `DISPATCH_LEVEL`. When the thread
/// releases the lock, the system restores the thread's IRQL to its previous
/// level. A driver that is not using automatic framework synchronization might
/// use a spin lock to synchronize access to a device object's context space, if
/// the context space is writable and if more than one of the driver's event
/// callback functions access the space. Before a driver can use a framework
/// spin lock it must call [`SpinLock::try_new()`] to create a [`SpinLock`]. The
/// driver can then call [`SpinLock::acquire`] to acquire the lock and
/// [`SpinLock::release()`] to release it.
pub struct SpinLock {
    wdf_spin_lock: WDFSPINLOCK,
}
impl SpinLock {
    /// Try to construct a WDF Spin Lock object
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The error variant will contain a [`NTSTATUS`] of the failure. Full error documentation is available in the [WDFSpinLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfspinlockcreate#return-value)
    pub fn try_new(attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        let mut spin_lock = Self {
            wdf_spin_lock: core::ptr::null_mut(),
        };

        let nt_status;
        // SAFETY: The resulting ffi object is stored in a private member and not
        // accessible outside of this module, and this module guarantees that it is
        // always in a valid state.
        unsafe {
            #![allow(clippy::multiple_unsafe_ops_per_block)]
            nt_status = macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockCreate,
                attributes,
                &mut spin_lock.wdf_spin_lock,
            );
        }
        nt_success(nt_status).then_some(spin_lock).ok_or(nt_status)
    }

    /// Try to construct a WDF Spin Lock object. This is an alias for
    /// [`SpinLock::try_new()`]
    ///
    /// # Errors
    ///
    /// This function will return an error if WDF fails to contruct a timer. The error variant will contain a [`NTSTATUS`] of the failure. Full error documentation is available in the [WDFSpinLock Documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfsync/nf-wdfsync-wdfspinlockcreate#return-value)
    pub fn create(attributes: &mut WDF_OBJECT_ATTRIBUTES) -> Result<Self, NTSTATUS> {
        Self::try_new(attributes)
    }

    /// Acquire the spinlock
    pub fn acquire(&self) {
        // SAFETY: `wdf_spin_lock` is a private member of `SpinLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        unsafe {
            #![allow(clippy::multiple_unsafe_ops_per_block)]
            let [()] = [macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockAcquire,
                self.wdf_spin_lock
            )];
        }
    }

    /// Release the spinlock
    pub fn release(&self) {
        // SAFETY: `wdf_spin_lock` is a private member of `SpinLock`, originally created
        // by WDF, and this module guarantees that it is always in a valid state.
        unsafe {
            #![allow(clippy::multiple_unsafe_ops_per_block)]
            let [()] = [macros::call_unsafe_wdf_function_binding!(
                WdfSpinLockRelease,
                self.wdf_spin_lock
            )];
        }
    }
}

/// Errors relevant to working with spinlocks.
pub enum SpinLockError {
    /// Attempted to create a lock a second time.
    AlreadyCreated {
        /// The existing, held lock
        lock: SafeSpinLock,
    },
    /// Could not create the lock.
    CreateFailed,
    /// The lock is already held.
    AlreadyHeld {
        /// The existing, held lock
        lock: SafeSpinLock,
    },
    /// The lock is not yet initialized
    Uninitialized {
        /// The existing, held lock
        lock: SafeSpinLock,
    },
    /// The lock is initialized but not held
    NotHeld {
        /// The existing, initialized but unheld lock
        lock: SafeSpinLock,
    },
}

/// A state-based wrapper for a WDF SpinLock.
///
/// This maintains the same state as the raw wrapper above,
/// but makes it illegal to attempt to double-acquire or double-release it.
/// 
/// This currently does **not** implement Drop, so release still must be manually called.
/// (What does it mean for a lock to implement Drop in Rust in this context? Is that something
/// we even want?)
pub enum SafeSpinLock {
    /// The spinlock has not been initialized and cannot be used.
    Uninitialized,
    /// The spinlock has been initialized but is not held.
    Initialized {
        /// The internal raw spinlock.
        inner: SpinLock,
    },
    /// The spinlock is currently held and cannot be acquired again.
    Held {
        /// The internal raw spinlock.
        inner: SpinLock,
    },
}

impl SafeSpinLock {
    /// Attempt to create a spinlock and move this to the initialized state.
    pub fn create(
        self,
        attributes: &mut WDF_OBJECT_ATTRIBUTES,
    ) -> Result<SafeSpinLock, SpinLockError> {
        match self {
            SafeSpinLock::Uninitialized => match SpinLock::create(attributes) {
                Ok(spin) => Ok(SafeSpinLock::Initialized { inner: spin }),
                Err(_) => Err(SpinLockError::CreateFailed),
            },
            SafeSpinLock::Held { .. } | SafeSpinLock::Initialized { .. } => {
                Err(SpinLockError::AlreadyCreated { lock: self })
            }
        }
    }

    /// Acquire the spinlock
    pub fn acquire(self) -> Result<SafeSpinLock, SpinLockError> {
        match self {
            SafeSpinLock::Initialized { inner: spin } => {
                spin.acquire();
                Ok(SafeSpinLock::Initialized { inner: spin })
            }
            SafeSpinLock::Held { .. } => Err(SpinLockError::AlreadyHeld { lock: self }),
            SafeSpinLock::Uninitialized => Err(SpinLockError::Uninitialized { lock: self }),
        }
    }

    /// Release the spinlock
    pub fn release(self) -> Result<SafeSpinLock, SpinLockError> {
        match self {
            SafeSpinLock::Held { inner: spin } => {
                spin.release();
                Ok(SafeSpinLock::Held { inner: spin })
            }
            SafeSpinLock::Initialized { .. } => Err(SpinLockError::NotHeld { lock: self }),
            SafeSpinLock::Uninitialized => Err(SpinLockError::Uninitialized { lock: self }),
        }
    }
}
