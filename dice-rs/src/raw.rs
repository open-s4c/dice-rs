//! Raw FFI bindings to the dice C library.
//!
//! This module provides direct, unsafe bindings to dice's C API. These functions
//! have strict safety requirements and should generally not be called directly.
//! Use the safe wrappers in the parent module (e.g., [`subscribe_scoped!`](crate::subscribe_scoped),
//! [`MempoolAllocator`](crate::MempoolAllocator)) instead.
//!
//! # General Safety Requirements
//!
//! All functions in this module assume:
//! - The dice library has been properly initialized (typically via `dice-self` or external setup)
//! - The program is linked with the appropriate dice modules for the features being used
//! - Callbacks registered with dice must not unwind (panic) across the FFI boundary
//!
//! # Linking
//!
//! Functions are linked against:
//! - `libdice` (static) - core pubsub and mempool functionality
//! - `libshim` (static) - logging bridge for Rust integration

use crate::{Chain, DiceResult, Metadata, TypeId};
use libc::c_void;

/// Function pointer type for dice pubsub callbacks.
///
/// Callbacks receive:
/// - `chain`: The chain this callback was registered on
/// - `ty`: The event type ID
/// - `event`: Pointer to the event data (may be null for marker events)
/// - `md`: Pointer to callback metadata (always valid during callback execution)
///
/// # Safety
///
/// Implementations must:
/// - Not unwind (panic) - this crosses an FFI boundary
/// - Not store `md` or references derived from it beyond the callback's return
/// - Handle null `event` pointers for marker events (events with no payload)
pub type PsCallbackF = Option<
    unsafe extern "C" fn(
        chain: Chain,
        ty: TypeId,
        event: *const c_void,
        md: *mut Metadata,
    ) -> DiceResult,
>;

#[link(name = "dice", kind = "static")]
unsafe extern "C" {
    /// Subscribe a callback to a dice event chain.
    ///
    /// Registers `cb` to be called whenever an event of type `ty` is published
    /// on `chain`. Multiple callbacks can be registered for the same event type;
    /// they are called in priority order (lower `prio` values first).
    ///
    /// # Safety
    ///
    /// - `chain` must be a valid [`Chain`] variant
    /// - `ty` must be a valid event type ID recognized by dice
    /// - `cb`, if `Some`, must be a valid function pointer that:
    ///   - Does not unwind (is `extern "C"` safe)
    ///   - Correctly handles the event pointer based on the event type
    /// - `prio` should be > 4 to avoid conflicting with dice-internal priorities
    ///
    /// # Returns
    ///
    /// Returns 0 on success, non-zero on failure.
    pub fn ps_subscribe(chain: Chain, ty: TypeId, cb: PsCallbackF, prio: i32) -> i32;

    /// Publish an event to a dice chain.
    ///
    /// Triggers all callbacks registered for event type `ty` on `chain`.
    ///
    /// # Safety
    ///
    /// - `chain` must be a valid [`Chain`] variant
    /// - `ty` must be a valid event type ID
    /// - `event` must be either:
    ///   - A valid pointer to a properly initialized event struct matching `ty`, or
    ///   - Null for marker events (events with no payload)
    /// - `md` must be a valid pointer to a [`Metadata`] instance
    /// - The event data must remain valid for the duration of all callback invocations
    ///
    /// # Returns
    ///
    /// Returns 0 on success, non-zero on failure.
    pub fn ps_publish(chain: Chain, ty: TypeId, event: *const c_void, md: *mut Metadata) -> i32;
}

/// Thread-related FFI bindings from the `dice-self` module.
///
/// These functions provide access to dice's thread identification and thread-local
/// storage facilities. They require the `dice-self` feature and module to be linked.
///
/// # Safety
///
/// All functions in this module require that:
/// - The `dice-self` module is linked into the final binary
/// - The calling thread has been registered with dice (i.e., the call originates
///   from within a dice callback or a dice-managed thread)
#[cfg(feature = "dice-self")]
pub mod thread {
    use crate::DiceThreadId;

    /// Destructor configuration for thread-local storage values.
    ///
    /// When a TLS slot is freed (e.g., on thread exit), if `free` is `Some`,
    /// it will be called with `arg` and the TLS value pointer.
    #[repr(C)]
    pub struct TlsDestructor {
        /// Optional destructor function. Called with `(arg, tls_value_ptr)`.
        pub free: Option<extern "C" fn(arg: *mut core::ffi::c_void, ptr: *mut core::ffi::c_void)>,
        /// User-provided argument passed to the destructor.
        pub arg: *mut core::ffi::c_void,
    }

    impl Default for TlsDestructor {
        fn default() -> Self {
            Self {
                free: None,
                arg: std::ptr::null_mut(),
            }
        }
    }

    use super::*;
    #[link(name = "dice", kind = "static")]
    unsafe extern "C" {
        /// Get the dice thread ID for the current thread.
        ///
        /// # Safety
        ///
        /// - `mt` must be a valid pointer to a [`Metadata`] instance from an active callback
        /// - Must be called from within a dice-managed context (callback or dice thread)
        ///
        /// # Returns
        ///
        /// The dice thread ID, or 0 if called outside a valid dice thread context.
        pub fn self_id(mt: *mut Metadata) -> DiceThreadId;

        /// Check if the current thread has been retired by dice.
        ///
        /// # Safety
        ///
        /// - `mt` must be a valid pointer to a [`Metadata`] instance
        pub fn self_retired(mt: *mut Metadata) -> bool;

        /// Allocate or retrieve thread-local storage by key and size.
        ///
        /// # Safety
        ///
        /// - `mt` must be a valid pointer to a [`Metadata`] instance
        /// - `key` is used to identify the TLS slot
        /// - `size` specifies the allocation size if the slot doesn't exist
        ///
        /// # Returns
        ///
        /// Pointer to the TLS data, or null on failure.
        pub fn self_tls(mt: *mut Metadata, key: *const c_void, size: usize) -> *mut libc::c_void;

        /// Get thread-local storage value by key.
        ///
        /// # Safety
        ///
        /// - `mt` must be a valid pointer to a [`Metadata`] instance
        ///
        /// # Returns
        ///
        /// The stored pointer for `key`, or null if no value has been set for this key.
        pub fn self_tls_get(mt: *mut Metadata, key: libc::uintptr_t) -> *mut libc::c_void;

        /// Set thread-local storage value by key.
        ///
        /// Associates `value` with `key` for the current thread. The optional `dtor`
        /// will be called when the TLS slot is freed.
        ///
        /// # Safety
        ///
        /// - `mt` must be a valid pointer to a [`Metadata`] instance
        /// - `key` should be unique per logical TLS variable (e.g., use address of a static)
        /// - `value` must remain valid until the TLS slot is freed or overwritten
        /// - If `dtor.free` is `Some`, it must be a valid function pointer
        pub fn self_tls_set(
            mt: *mut Metadata,
            key: libc::uintptr_t,
            value: *mut libc::c_void,
            dtor: TlsDestructor,
        );
    }
}

#[link(name = "dice", kind = "static")]
unsafe extern "C" {
    /// Allocate memory from dice's mempool with specified alignment.
    ///
    /// This is the preferred allocation function when specific alignment is required.
    ///
    /// # Safety
    ///
    /// - `alignment` must be a power of two
    /// - `size` must be non-zero
    /// - The returned pointer must be freed with [`mempool_free`], not `free()` or other deallocators
    ///
    /// # Returns
    ///
    /// A pointer to the allocated memory with the requested alignment, or null on failure.
    /// The memory is uninitialized.
    pub fn mempool_aligned_alloc(alignment: libc::size_t, size: libc::size_t) -> *mut libc::c_void;

    /// Allocate memory from dice's mempool with default alignment (no alignment)
    ///
    /// # Safety
    ///
    /// - `size` must be non-zero
    /// - The returned pointer must be freed with [`mempool_free`]
    ///
    /// # Returns
    ///
    /// A pointer to the allocated memory, or null on failure. The memory is uninitialized.
    pub fn mempool_alloc(size: libc::size_t) -> *mut libc::c_void;

    /// Free memory previously allocated from dice's mempool.
    ///
    /// # Safety
    ///
    /// - `ptr` must have been returned by [`mempool_alloc`] or [`mempool_aligned_alloc`]
    /// - `ptr` must not have been previously freed
    /// - `ptr` may be null (no-op in that case)
    pub fn mempool_free(ptr: *mut libc::c_void);
}

#[link(name = "shim", kind = "static")]
unsafe extern "C" {
    /// Write a log message through dice's logging infrastructure.
    ///
    /// # Safety
    ///
    /// - `level` should be a valid log level: 0 (FATAL), 1 (INFO), or 2 (DEBUG)
    /// - `msg` must be a valid pointer to a null-terminated C string
    /// - `msg` must remain valid for the duration of the call
    pub fn dice_log_write(level: i32, msg: *const std::os::raw::c_char);
}
