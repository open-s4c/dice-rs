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
/// `ID` must match the event type ID used by dice.
pub trait DiceEvent: Sized {
    const ID: TypeId;

    /// Fallback reference used when dice does not provide an event payload.
    ///
    /// This is typically used for unit structs, where dice will pass a null
    /// pointer instead of an actual instance.
    fn fallback<'a>() -> Option<&'a Self> {
        None
    }

    /// Cast a raw pointer from a dice callback to a Rust reference.
    ///
    /// If the event does not carry data, dice will not provide a concrete type
    /// or struct and instead return a null pointer. The `#[dice_event(...)]`
    /// attribute macro handles this correctly by implementing [`fallback`].
    ///
    /// The fallback is a `&T`, which is a valid reference for unit structs
    /// (structs without fields).
    ///
    /// # Safety
    ///
    /// - `ptr` must either be:
    ///   - a valid pointer to a properly initialized `Self`, or
    ///   - a null pointer for types where [`fallback`] returns `Some`.
    /// - The pointed-to value must outlive the returned reference.
    #[inline]
    unsafe fn from_raw<'a>(ptr: *const ()) -> Option<&'a Self> {
        if ptr.is_null() {
            Self::fallback()
        } else {
            let ptr = ptr as *const Self;
            debug_assert!(ptr.is_aligned(), "Sanity Check");

            // SAFETY: we know ptr is not null because we tested for null above.
            // By the safety assumption of this function,
            // this means it is a valid pointer to a properly initialized Self,
            // and we can take a reference to it
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
/// Due to private fields, it is not possible to construct `Metadata` safely
/// outside of this crate. It is also:
///
/// - `!Send` and `!Sync` (via the `_marker: PhantomData<*mut ()>` field)
/// - neither `Copy` nor `Clone`
///
/// This ensures uniqueness and locality of metadata usage within callbacks.
#[repr(C, align(8))]
#[derive(Debug)]
pub struct Metadata {
    drop_: bool,
    // This marker makes `Metadata` !Send and !Sync.
    _marker: PhantomData<*mut ()>,
}

/// Allocator backed by dice's mempool allocator.
///
/// This type is intended to be used as a global allocator for crates that want
/// to allocate via dice's mempool instead of the default system allocator.
pub struct MempoolAllocator;

/// GlobalAlloc interface implementation for Dice Mempool Allocator
///
/// SAFETY:
/// - does not unwind
/// - allocation is at least as big as requested and is aligned
/// - this allocator does not rely on allocations actually happenin
///   or in other words, it does not conflict or require state that would conflict with optimizing allocations away.
unsafe impl GlobalAlloc for MempoolAllocator {
    /// Returns a pointer to a correctly sized memory
    /// or null to indiciate allocation failure
    /// # Safety
    /// - zero sized layouts are UB
    /// # Errors
    /// on memory exhaustion will return a null pointer
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: dice mempool allocates memory with all the requirements described above.
        unsafe { raw::mempool_aligned_alloc(layout.align(), layout.size()) as *mut u8 }
    }

    /// Deallocate a previously allocated memory.
    ///
    /// # Safety
    /// ptr must be allocated with the same allocator instance.
    /// the deallocation does not depend on the layout, it does not depend on the Layout
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // SAFETY: dice mempool keeps track of layout internally on allocation
        // the freeing does not require layout and is safe when the correct pointer is supplied
        unsafe { raw::mempool_free(ptr as *mut libc::c_void) };
    }
}

/// Helpers for thread-local storage and thread IDs.
///
/// This module allows subscribers to:
///
/// - determine the dice thread they are running on
/// - use dice-backed thread-local storage.
///
/// <div class="warning">
/// This requires the `dice-self` module to be linked.
/// Without the `dice-self` module, this will break.
/// </div>
#[cfg(feature = "dice-self")]
pub mod thread {
    use std::{marker::PhantomData, mem::MaybeUninit};

    use crate::{DiceThreadId, Metadata, raw};

    /// Get the current dice thread ID.
    pub fn self_id(mt: &mut Metadata) -> DiceThreadId {
        // SAFETY: Metadata cannot be constructed,
        // thus it is exclusive and will always yield a correct Self Id
        unsafe { raw::thread::self_id(mt) }
    }

    // Note: `T` does not have to be `repr(C)` as we perform the size
    // calculation on the Rust side.
    #[repr(C)]
    struct TlsCell<T> {
        initialized: bool,
        value: MaybeUninit<T>,
    }

    pub struct TlsKey<T> {
        _marker: PhantomData<T>,
    }

    impl<T: Default> Default for TlsKey<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> TlsKey<T> {
        pub const fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: Default> TlsKey<T> {
        /// get a potentially unatialized thread local storage pointer
        /// # Safety
        /// user must check for initialization and potentially do it themselves
        #[inline(always)]
        unsafe fn cell_ptr(&self, mt: &mut Metadata) -> *mut TlsCell<T> {
            let key = self as *const TlsKey<T> as libc::uintptr_t;

            // SAFETY: key and metadata are both references and thus valid here (metadata even exclusive)
            let raw = unsafe { raw::thread::self_tls_get(mt, key) as *mut TlsCell<T> };
            if !raw.is_null() {
                return raw;
            }

            let layout = std::alloc::Layout::new::<TlsCell<T>>();
            // SAFETY: correct size and alignment calculated
            let raw = unsafe { raw::mempool_aligned_alloc(layout.align(), layout.size()) };
            let ptr = raw as *mut TlsCell<T>;
            debug_assert!(!raw.is_null() && raw.is_aligned());
            // SAFETY: ptr not null and alignment correct (checked in debug)
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

        /// wrapper for get_mut make scoping easier
        #[inline]
        pub fn with<R>(&self, mt: &mut Metadata, f: impl FnOnce(&mut T) -> R) -> R {
            let t = self.get_mut(mt);
            f(t)
        }

        /// Get a Mutable reference to a Thread Local Storage Object
        #[inline]
        pub fn get_mut<'a>(&self, mt: &'a mut Metadata) -> &'a mut T {
            // SAFETY: cell gets initialized in this function if it is not already.
            let cell = unsafe { self.cell_ptr(mt) };
            // Safety: to take a mutable reference, cell must be unique. This is fulfilled because:
            // - &'a mut Metadata and &'a mut Cell will have the same lifetime and exclusiveness
            // - This is thread local and cannot leave the thread due to !Send and !Sync
            let cell = unsafe { &mut *cell };
            // TODO: consider using (#[cold] based) unlikely here as this only happens once
            if !cell.initialized {
                cell.value = MaybeUninit::new(T::default());
                cell.initialized = true;
            }

            // SAFETY: this value is definitely initialized now
            unsafe { cell.value.assume_init_mut() }
        }
    }

    /// Create a key for a dice thread local allocation with the same type.
    #[macro_export]
    macro_rules! tls_key {
        ($name:ident : $ty:ty) => {
            static $name: TlsKey<$ty> = TlsKey::new();
        };
    }
}

/// Create a callback and subscribe to dice.
/// this subscribe macro emulates a normal rust anonymous function structure.
/// it creates a c callback and subscribes it automatically
/// it is type, lifetime and capture guarded using the _guard
///
/// # Panics
///
/// Will panic if priority of <= 4 is used. These are reserverd dice internal priorities.
#[macro_export]
macro_rules! subscribe_scoped {
    ($chain:expr, $prio:expr, |$e:ident: &$t:ty, $m:ident| $body:block) => {{
        // this guard enforces:
        // - no capturing of outside scope variables (besides global statics/consts)
        // - enforces types and lifetimes within body $body
        // without this one could create a &'static mut Metadata or override the type within the body
        // or capture variables outside the scope, both of these cases are UB.
        let _guard: fn(&$t, &mut $crate::Metadata) -> $crate::DiceResult =
            |$e: &$t, $m: &mut $crate::Metadata| $body;

        // enforce priority > 4 to not conflict with dice internals
        assert!($prio > 4, "Priority must be greater than 4");

        extern "C" fn __trampoline(
            chain: $crate::Chain,
            _ty: $crate::TypeId,
            event: *const core::ffi::c_void,
            md: *mut $crate::Metadata,
        ) -> $crate::DiceResult {
            // SAFETY: the dice subscribe callback either gives a correctly typed pointer for the event
            // or it gives a null in case the Event struct is empty.
            // Rust allows to take references of unit types directly and treat them as instances (like &() as () is type fields)
            // as these have no fields, there is also no concern of possibility of mutating these (potentially shared) references
            let Some(ev_ref) = (unsafe { <$t as $crate::DiceEvent>::from_raw(event as *const ()) }) else {
                return $crate::DiceResult::Invalid;
            };

            let __chain = chain;

            let $e: &$t = ev_ref;
            let Some($m) = (unsafe { md.as_mut() }) else {
                return $crate::DiceResult::Invalid;
            };

            $body
        }

        // Safety: all inputs are well typed to not make invalid states possible
        // prio > 4 is checked, to not conflict with dice internals
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
                use ::std::sync::Once;
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
