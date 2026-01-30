//! High-level Rust bindings and utilities for integrating with the `dice` engine.
//!
//! This module provides:
//! - type aliases for core `dice` identifiers
//! - the [`DiceEvent`] trait used by event types
//! - integration with dice's mempool allocator via [`MempoolAllocator`]
//! - metadata helpers for callbacks
//! - thread-local storage helpers for dice threads
//! - macros for subscribing Rust callbacks to dice chains and initializing logging
pub mod log;
pub mod raw;

use std::{
    alloc::{GlobalAlloc, Layout},
    marker::PhantomData,
};

pub mod events {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

// TODO: consider using newtype pattern for ChainId, TypeId and DiceThreadId

/// Identifier of a dice callback chain.
pub type ChainId = u16;

/// Identifier of a dice event type.
pub type TypeId = u16;

/// Identifier for a dice thread, as used by the `dice-self` module.
pub type DiceThreadId = u64;

/// Available dice chains.
///
/// These correspond to different interception and capture points in the dice
/// processing pipeline.
///
/// <div class="warning">
/// enabling `dice-self` module will disable intercept chains and enable capture chains
/// </div>
#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Chain {
    InterceptEvent = 1,
    InterceptBefore = 2,
    InterceptAfter = 3,
    CaptureEvent = 4,
    CaptureBefore = 5,
    CaptureAfter = 6,
}

/// Result codes returned from dice callbacks.
///
/// These values directly map to dice's C API.
#[repr(i32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiceResult {
    Ok = 0,
    StopChain = 1,
    DropEvent = 2,
    HandlerOff = 3,
    Invalid = -1,
    Error = -2,
}

/// Trait implemented by all dice event types.
///
/// This trait maps Rust types to dice's C event system. Each event type has a
/// unique [`ID`](Self::ID) that must match the corresponding C definition in dice.
///
/// # The Fallback Mechanism
///
/// Some dice events are **marker events** that signal an occurrence without carrying
/// any payload data. Examples include `ThreadStartEvent` and `ThreadExitEvent`.
/// In C, these are represented as empty structs, and dice passes a null pointer
/// instead of an actual event instance.
///
/// The `#[dice_event(...)]` attribute macro handles this by implementing [`fallback`](Self::fallback)
/// to return `Some(&Self)` for unit structs. This is safe because:
///
/// 1. **Unit structs have no fields**: There is no data to read, so no valid memory is needed
/// 2. **Rust allows zero-sized references**: A reference to a unit type doesn't dereference memory
/// 3. **Immutable access only**: The returned `&Self` is immutable, preventing any writes
///
/// For events with fields (non-unit structs), `fallback` returns `None`, and `from_raw`
/// will also return `None` if given a null pointer, allowing the caller to handle the error.
///
/// # Implementing This Trait
///
/// Use the `#[dice_event(EVENT_ID)]` attribute macro instead of implementing manually:
///
/// ```ignore
/// #[repr(C)]
/// #[derive(Copy, Clone, Debug)]
/// #[dice_event(raw::EVENT_THREAD_START)]
/// pub struct ThreadStartEvent;  // Unit struct - fallback will be Some
///
/// #[repr(C)]
/// #[derive(Copy, Clone, Debug)]
/// #[dice_event(raw::EVENT_MALLOC)]
/// pub struct MallocEvent {      // Has fields - fallback will be None
///     pub size: usize,
///     pub ret: *const (),
/// }
/// ```
pub trait DiceEvent: Sized {
    /// The event type ID, must match the C definition in dice.
    const ID: TypeId;

    /// Fallback reference used when dice provides a null event pointer.
    ///
    /// Returns `Some(&Self)` for unit structs (marker events), `None` otherwise.
    /// The `#[dice_event(...)]` macro implements this automatically.
    fn fallback<'a>() -> Option<&'a Self> {
        None
    }

    /// Convert a raw pointer from a dice callback to a Rust reference.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - ptr is a valid, properly aligned pointer to an initialized `Self`, or Null 
    /// - The pointed-to data remains valid for lifetime `'a`
    /// - No mutable references to the data exist for lifetime `'a`
    /// - No other thread is concurrently writing to the data (no data races)
    ///
    /// # Returns
    ///
    /// - `Some(&Self)` if `ptr` is valid or fallback is available
    /// - `None` if `ptr` is null and no fallback exists
    #[inline]
    unsafe fn from_raw<'a>(ptr: *const ()) -> Option<&'a Self> {
        if ptr.is_null() {
            Self::fallback()
        } else {
            let ptr = ptr as *const Self;
            debug_assert!(ptr.is_aligned(), "Event pointer must be properly aligned");

            /* SAFETY:
             * - not dangling: by the safety contract, ptr must be valid for lifetime 'a
             * - aligned: by the safety contract, ptr must be properly aligned (debug_assert above)
             * - no data race: by the safety contract, no concurrent writes may occur
             * - aliasing: we create a shared reference; caller guarantees no mutable refs exist
             * - initialized: by the safety contract, the pointed-to data is properly initialized
             */
            let reference = unsafe { &*ptr };
            Some(reference)
        }
    }
}

/// Metadata passed into a dice subscribe callback and constructed by publishers.
///
/// For example, metadata is used to access "internal" properties such as the
/// thread ID in capture chain callbacks.
///
/// # Safety Invariants
///
/// `Metadata` is deliberately designed to be **impossible to construct, clone, or
/// share** outside of dice's control. This design enables critical safety guarantees
/// for the entire library:
///
/// 1. **Uniqueness**: Each callback invocation receives exactly one `&mut Metadata`.
///    Since `Metadata` has private fields and no public constructor, users cannot
///    create additional instances. This guarantees that functions receiving
///    `&mut Metadata` have exclusive access.
///
/// 2. **Thread Locality**: `Metadata` is `!Send` and `!Sync` (via `PhantomData<*mut ()>`).
///    This prevents passing metadata between threads, which is essential because
///    dice's thread-local storage is keyed by metadata, and the metadata itself
///    contains thread-specific state.
///
/// 3. **No Aliasing**: Since `Metadata` is neither `Clone` nor `Copy`, and cannot
///    be constructed outside dice, the `&mut Metadata` passed to callbacks is
///    guaranteed to be the only reference. This allows functions like
///    [`TlsKey::get_mut`](thread::TlsKey::get_mut) to safely return `&'a mut T`
///    tied to the metadata's lifetime without runtime borrow checking.
///
/// 4. **Bounded Lifetime**: The `&mut Metadata` reference is only valid for the
///    duration of the callback. The [`subscribe_scoped!`] macro enforces this by
///    preventing capture of the metadata reference beyond the callback body.
///
/// These invariants collectively allow us to provide safe mutable access to
/// thread-local storage and other per-callback state without runtime overhead.
#[repr(C, align(8))]
#[derive(Debug)]
pub struct Metadata {
    drop_: bool,
    // This marker makes `Metadata` !Send and !Sync, enforcing thread locality.
    _marker: PhantomData<*mut ()>,
}

/// Allocator backed by dice's mempool allocator.
///
/// This type is intended to be used as a global allocator for crates that want
/// to allocate via dice's mempool instead of the default system allocator.
/// This is typically required when building dice plugins, as all allocations
/// must go through dice's tracked memory pool.
///
/// # Example
///
/// ```ignore
/// #[global_allocator]
/// static GLOBAL: dice_rs::MempoolAllocator = dice_rs::MempoolAllocator;
/// ```
pub struct MempoolAllocator;

/// # Safety
///
/// This implementation upholds the [`GlobalAlloc`] safety contract:
///
/// 1. **No unwinding**: Neither `alloc` nor `dealloc` will unwind. They delegate
///    to dice's C mempool functions which cannot panic. Unwinding from a global
///    allocator is undefined behavior.
///
/// 2. **Correct layout handling**: Layout parameters are forwarded directly to
///    the underlying mempool. The implementation does not perform any layout
///    calculations that could be incorrect.
///
/// 3. **No reliance on allocations occurring**: This allocator maintains no
///    internal state that depends on allocations actually happening. The
///    optimizer may eliminate or move allocations without affecting correctness.
unsafe impl GlobalAlloc for MempoolAllocator {
    /// Allocate memory with the specified layout.
    ///
    /// # Safety
    ///
    /// Per [`GlobalAlloc::alloc`], `layout` must have non-zero size.
    /// Allocating with a zero-sized layout is undefined behavior.
    ///
    /// # Returns
    ///
    /// A pointer to newly allocated memory, or null on failure.
    /// The returned memory is **uninitialized**.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        /* SAFETY: raw::mempool_aligned_alloc preconditions (from raw.rs):
         * - alignment must be power of two: Layout guarantees this
         * - size must be non-zero: GlobalAlloc contract requires layout.size() > 0
         */
        let allocation = unsafe { raw::mempool_aligned_alloc(layout.align(), layout.size()) };
        allocation as *mut u8
    }

    /// Deallocate memory previously allocated by this allocator.
    ///
    /// # Safety
    ///
    /// Per [`GlobalAlloc::dealloc`], the caller must ensure:
    /// - `ptr` is allocated by this allocator
    /// - `layout` is the same layout used to allocate `ptr`
    ///
    /// Note: While the dice mempool does not require the layout for freeing
    /// (it tracks size internally), callers must still satisfy the trait's
    /// contract by passing the correct layout.
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        /* SAFETY: raw::mempool_free preconditions (from raw.rs):
         * - ptr from mempool_alloc/aligned_alloc: caller guarantees via GlobalAlloc contract
         * - ptr not previously freed: caller guarantees via GlobalAlloc contract
         */
        unsafe { raw::mempool_free(ptr as *mut libc::c_void) };
    }
}

/// Helpers for thread-local storage and thread identification.
///
/// This module provides safe wrappers around dice's thread-local storage (TLS)
/// facilities, allowing subscribers to:
///
/// - Determine which dice thread they are running on via `self_id`
/// - Store per-thread state using `TlsKey` without standard library TLS
///
/// # Requirements
///
/// <div class="warning">
///
/// This module requires the `dice-self` feature and the `dice-self` C module
/// to be linked into the final binary. The `dice-self` module provides thread
/// tracking and TLS infrastructure that these functions depend on.
///
/// Without `dice-self`:
/// - `self_id` will return 0 or undefined values
/// - TLS operations may fail or cause undefined behavior
///
/// </div>
///
/// # Thread-Local Storage Design
///
/// Unlike standard library TLS (`thread_local!`), dice's TLS is:
/// - Keyed by arbitrary `uintptr_t` values (we use the address of the `TlsKey`)
/// - Managed by dice's memory pool (allocations survive across callback invocations)
/// - Accessible only through a valid [`Metadata`] reference
///
/// The safety of `TlsKey::get_mut` returning `&mut T` without runtime borrow checking
/// relies on the invariants documented on [`Metadata`].
#[cfg(feature = "dice-self")]
pub mod thread {
    use std::{marker::PhantomData, mem::MaybeUninit};

    use crate::{DiceThreadId, Metadata, raw};

    /// Get the current dice thread ID.
    ///
    /// Returns the unique identifier assigned to this thread by dice's `dice-self` module.
    ///
    /// # Returns
    ///
    /// The thread ID, or 0 if called outside a valid dice thread context.
    pub fn self_id(mt: &mut Metadata) -> DiceThreadId {
        /* SAFETY: raw::thread::self_id preconditions (from raw.rs):
         * - mt must be valid pointer to Metadata: mt is a reference, guaranteed valid by Rust
         * - must be called from dice-managed context: this module requires dice-self feature,
         *   and this function is only callable with &mut Metadata from a callback
         */
        unsafe { raw::thread::self_id(mt) }
    }

    /// Internal storage cell for TLS values.
    ///
    /// Tracks initialization state to support lazy initialization via `Default`.
    /// Note: `T` does not have to be `repr(C)` as we perform the size and alignment
    /// calculations on the Rust side.
    #[repr(C)]
    struct TlsCell<T> {
        initialized: bool,
        value: MaybeUninit<T>,
    }

    /// A key for accessing dice thread-local storage.
    ///
    /// Each `TlsKey<T>` provides access to a separate `T` value per thread.
    /// The key uses its own memory address as the TLS key, so each static
    /// `TlsKey` instance accesses a different TLS slot.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use dice_rs::thread::TlsKey;
    ///
    /// static COUNTER: TlsKey<u64> = TlsKey::new();
    ///
    /// // In a callback:
    /// let count = COUNTER.get_mut(metadata);
    /// *count += 1;
    /// ```
    pub struct TlsKey<T> {
        _marker: PhantomData<T>,
    }

    impl<T: Default> Default for TlsKey<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> TlsKey<T> {
        /// Create a new TLS key.
        ///
        /// This should typically be used to create `static` keys, as the key's
        /// memory address is used as the TLS slot identifier.
        pub const fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: Default> TlsKey<T> {
        /// Get or allocate the TLS cell for this key.
        ///
        /// # Safety
        ///
        /// This is an internal function. The returned pointer is valid only while
        /// `mt` is valid (i.e., during the current callback). The caller must ensure
        /// the pointer is not used after the callback returns.
        ///
        /// The safety of returning a raw pointer relies on:
        /// - `mt` being exclusive (Metadata invariants)
        /// - The TLS slot being unique per (thread, key) pair
        /// - Thread-locality prevents data races (only this thread accesses this slot)
        ///
        /// **Important**: For newly allocated cells, the returned memory is uninitialized.
        /// The caller (i.e., `get_mut`) must check `cell.initialized` before reading
        /// `cell.value` to avoid undefined behavior from reading uninitialized memory.
        #[inline(always)]
        unsafe fn cell_ptr(&self, mt: &mut Metadata) -> *mut TlsCell<T> {
            // Use this key's address as the TLS slot identifier
            let key = self as *const TlsKey<T> as libc::uintptr_t;

            /* SAFETY: raw::thread::self_tls_get preconditions (from raw.rs):
             * - mt must be valid pointer to Metadata: mt is a mutable reference, guaranteed valid
             * Returns null if key not set, otherwise a valid pointer we previously stored.
             */
            let raw = unsafe { raw::thread::self_tls_get(mt, key) as *mut TlsCell<T> };
            if !raw.is_null() {
                return raw;
            }

            // First access on this thread - allocate a new cell
            let layout = std::alloc::Layout::new::<TlsCell<T>>();
            /* SAFETY: raw::mempool_aligned_alloc preconditions (from raw.rs):
             * - alignment must be power of two: Layout::new guarantees valid alignment
             * - size must be non-zero: TlsCell<T> contains at least a bool, so size > 0
             * Note: returned memory is uninitialized; caller must check cell.initialized
             * before reading cell.value to avoid UB from reading uninitialized memory.
             */
            let raw = unsafe { raw::mempool_aligned_alloc(layout.align(), layout.size()) };
            let ptr = raw as *mut TlsCell<T>;
            debug_assert!(
                !raw.is_null() && raw.is_aligned(),
                "TLS allocation failed or misaligned"
            );

            /* SAFETY: raw::thread::self_tls_set preconditions (from raw.rs):
             * - mt must be valid pointer to Metadata: mt is a mutable reference, guaranteed valid
             * - key should be unique per TLS variable: we use address of static TlsKey as key
             * - value must remain valid until freed/overwritten: allocated from mempool, stays valid
             * - dtor.free must be valid if Some: we use TlsDestructor::default() which has None
             */
            unsafe {
                raw::thread::self_tls_set(
                    mt,
                    key,
                    ptr as *mut libc::c_void,
                    raw::thread::TlsDestructor::default(),
                )
            };
            ptr
        }

        /// Access the thread-local value, applying a function to it.
        ///
        /// This is a convenience wrapper around [`get_mut`](Self::get_mut) for
        /// cases where you want to limit the scope of the mutable borrow.
        #[inline]
        pub fn with<R>(&self, mt: &mut Metadata, f: impl FnOnce(&mut T) -> R) -> R {
            let t = self.get_mut(mt);
            f(t)
        }

        /// Get a mutable reference to this thread's value.
        ///
        /// On first access per thread, the value is initialized via `T::default()`.
        ///
        /// # Safety Explanation
        ///
        /// This function safely returns `&'a mut T` without runtime borrow checking because:
        ///
        /// 1. **Exclusivity via Metadata**: The `&'a mut Metadata` argument guarantees exclusive
        ///    access. Since Metadata cannot be cloned or constructed (see [`Metadata`]
        ///    docs), there can be no concurrent calls to `get_mut` with the same TLS key.
        ///
        /// 2. **Thread locality**: Metadata is `!Send` and `!Sync`, ensuring this reference
        ///    cannot escape to another thread where a different TLS value would be accessed.
        ///
        /// 3. **Lifetime bound**: The returned `&'a mut T` has the same lifetime as the
        ///    `&'a mut Metadata` borrow, preventing use after the callback returns.
        #[inline]
        pub fn get_mut<'a>(&self, mt: &'a mut Metadata) -> &'a mut T {
            /* SAFETY: cell_ptr preconditions (internal unsafe fn, see its doc):
             * - mt must be exclusive: we have &mut Metadata
             * - pointer valid while mt valid: Metadata lifetime bounds the cell lifetime
             * - thread-local: Metadata is !Send/!Sync, cannot escape thread
             */
            let cell = unsafe { self.cell_ptr(mt) };

            /* SAFETY: creating &mut from raw pointer:
             * - not dangling: cell_ptr returns valid pointer, mt still valid
             * - aligned: cell_ptr allocates with Layout::new::<TlsCell<T>>()
             * - no data race: thread-local storage, Metadata is !Send/!Sync
             * - no aliasing: Metadata exclusivity prevents concurrent get_mut calls
             * - initialized (struct): we only read .initialized field before full init
             */
            let cell = unsafe { &mut *cell };

            // Lazy initialization on first access
            if !cell.initialized {
                cell.value = MaybeUninit::new(T::default());
                cell.initialized = true;
            }

            // SAFETY: cell.initialized is true, so cell.value was written via MaybeUninit::new()
            unsafe { cell.value.assume_init_mut() }
        }
    }

    /// Create a static key for dice thread-local storage.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use dice_rs::{tls_key, thread::TlsKey};
    ///
    /// tls_key!(MY_COUNTER: u64);
    ///
    /// // In a callback:
    /// let count = MY_COUNTER.get_mut(metadata);
    /// *count += 1;
    /// ```
    #[macro_export]
    macro_rules! tls_key {
        ($name:ident : $ty:ty) => {
            static $name: TlsKey<$ty> = TlsKey::new();
        };
    }
}

/// Create a callback and subscribe it to a dice event chain.
///
/// This macro provides a safe way to register Rust callbacks with dice's C pubsub system.
/// It handles the FFI boundary, type safety, and lifetime management automatically.
///
/// # Syntax
///
/// ```ignore
/// subscribe_scoped!(chain, priority, |event: &EventType, metadata| {
///     // callback body
///     DiceResult::Ok
/// });
/// ```
///
/// # Safety Mechanism
///
/// The macro uses a **guard pattern** to enforce safety at compile time. The guard is a
/// type annotation that the callback body must satisfy:
///
/// ```ignore
/// let _guard: fn(&$t, &mut Metadata) -> DiceResult = |$e, $m| $body;
/// ```
///
/// This provides two critical guarantees:
///
/// 1. **No variable capture**: A `fn` pointer cannot capture local variables from the
///    enclosing scope. Closures that capture variables cannot coerce to `fn` pointers,
///    causing a compile-time error. This prevents use-after-free bugs if the callback
///    were to outlive captured data.
///
/// 2. **Correct types and lifetimes**: The body must use exactly `&$t` and `&mut Metadata`
///    with their natural (inferred) lifetimes. Without this guard, one could shadow these
///    bindings with incorrect types or artificially extended lifetimes (e.g., transmuting
///    to `&'static mut Metadata`), which would be undefined behavior.
///
/// The guard itself is never executed at runtime - it exists solely to trigger compile-time
/// type checking.
///
/// # Panics
///
/// Panics if `priority <= 4`. Priorities 1-4 are reserved for dice-internal use.
///
/// # Example
///
/// ```ignore
/// use dice_rs::{subscribe_scoped, Chain, DiceResult, events::MallocEvent};
///
/// let result = subscribe_scoped!(Chain::CaptureAfter, 10, |ev: &MallocEvent, _mt| {
///     println!("malloc({}) = {:?}", ev.size, ev.ret);
///     DiceResult::Ok
/// });
/// ```
#[macro_export]
macro_rules! subscribe_scoped {
    ($chain:expr, $prio:expr, |$e:ident: &$t:ty, $m:ident| $body:block) => {{
        // GUARD: This type annotation enforces compile-time safety:
        // - Prevents variable capture: fn pointers cannot capture locals, preventing use-after-free
        // - Enforces correct types/lifetimes: body must use exact types, preventing lifetime extension
        // The guard is never executed - it only triggers compile-time type checking.
        let _guard: fn(&$t, &mut $crate::Metadata) -> $crate::DiceResult =
            |$e: &$t, $m: &mut $crate::Metadata| $body;

        // Priorities 1-4 are reserved for dice internals
        assert!($prio > 4, "Priority must be greater than 4 (1-4 are reserved for dice internals)");

        extern "C" fn __trampoline(
            chain: $crate::Chain,
            _ty: $crate::TypeId,
            event: *const core::ffi::c_void,
            md: *mut $crate::Metadata,
        ) -> $crate::DiceResult {
            /* SAFETY: DiceEvent::from_raw preconditions (from trait definition):
             * - ptr valid/aligned or null with fallback: dice guarantees valid event ptr or null
             * - data valid for lifetime: dice guarantees event valid for callback duration
             * - no mutable refs exist: dice provides immutable event data to callbacks
             * - no concurrent writes: callback executes synchronously, dice doesn't modify during
             */
            let Some(ev_ref) = (unsafe { <$t as $crate::DiceEvent>::from_raw(event as *const ()) }) else {
                return $crate::DiceResult::Invalid;
            };

            let __chain = chain;

            let $e: &$t = ev_ref;
            /* SAFETY: creating &mut Metadata from raw pointer:
             * - not dangling: dice guarantees md valid for callback duration
             * - aligned: dice provides properly aligned Metadata pointer
             * - no data race: callback executes on single thread
             * - no aliasing: Metadata unique per callback (see Metadata invariants)
             * - initialized: dice initializes Metadata before invoking callback
             */
            let Some($m) = (unsafe { md.as_mut() }) else {
                return $crate::DiceResult::Invalid;
            };

            $body
        }

        /* SAFETY: raw::ps_subscribe preconditions (from raw.rs):
         * - chain must be valid Chain: $chain is type-checked as Chain enum by Rust
         * - ty must be valid event type ID: comes from DiceEvent::ID trait implementation
         * - cb must not unwind: __trampoline is extern "C" and does not panic
         * - prio > 4: enforced by assert above
         */
        unsafe {
            $crate::raw::ps_subscribe(
                $chain,
                <$t as $crate::DiceEvent>::ID,
                Some(__trampoline),
                $prio,
            )
        }
    }};
}

/// Create a callback and subscribe it to dice.
/// this is a convenience macro over `subscribe_scoped` which allows usage in global scope
/// and subscribes at startup of the program using a startup constructors.
/// `Once` ensures this only happens once, even in multithreaded applications.
#[macro_export]
macro_rules! subscribe {
    ($chain:expr, $slot:expr, |$e:ident: &$t:ty, $m:ident| $body:block) => {
        const _: () = {
            #[allow(non_snake_case)]
            #[::ctor::ctor]
            fn __dice_subscribe_ctor() {
                use std::sync::Once;
                static INIT: Once = Once::new();

                INIT.call_once(|| {
                    let _ = $crate::subscribe_scoped!($chain, $slot, |$e: &$t, $m| $body);
                });
            }
        };
    };
}

/// Helper to initialize logging
/// TODO: make it a struct
#[macro_export]
macro_rules! init_dice_state {
    (log_level: $level:expr) => {
        #[global_allocator]
        static GLOBAL: $crate::MempoolAllocator = $crate::MempoolAllocator;

        #[::ctor::ctor]
        fn __init_log() {
            $crate::log::init($level);
        }
    };
    () => { init_dice_state!(log_level: $crate::log::LogLevel::Debug); }
}
