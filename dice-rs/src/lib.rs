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

            // SAFETY: we know ptr has the correct type and is not null.
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

const MIN_ALIGN: usize = 8;

/// Dice based memory allocator
///
/// base alignment is 8 bytes, and every object gets a header 8 bytes
/// this means we overallocate by at minimum 8 bytes.
/// the padding is then calculated by using the 8 byte discretization.
/// # Safety
///
/// Everything from [`std::alloc::GlobalAlloc`] applies.
unsafe impl GlobalAlloc for MempoolAllocator {
    /// Returns a pointer to a correctly sized memory region.
    /// # Safety
    ///
    /// Everything from [`std::alloc::GlobalAlloc::alloc`] applies.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        if align <= MIN_ALIGN {
            return unsafe { raw::mempool_alloc(size) as *mut u8 };
        }

        // size + alignment guarantees space for header and alignment
        let alloc_size = size + align;
        let raw_ptr = unsafe { raw::mempool_alloc(alloc_size) as *mut u8 };

        if raw_ptr.is_null() {
            return std::ptr::null_mut();
        }

        let raw_addr = raw_ptr as usize;
        let header_size = std::mem::size_of::<usize>();

        let mask = align - 1;
        let aligned_addr = (raw_addr + header_size + mask) & !mask;
        let aligned_ptr = aligned_addr as *mut u8;

        // Store the header immediately before the aligned pointer.
        let header_ptr = unsafe { (aligned_ptr as *mut usize).sub(1) };
        unsafe { *header_ptr = raw_addr };

        aligned_ptr
    }

    /// Deallocate a previously allocated memory region.
    ///
    /// # Safety
    ///
    /// Everything from [`std::alloc::GlobalAlloc::dealloc`] applies
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.align() <= MIN_ALIGN {
            unsafe { raw::mempool_free(ptr as *mut libc::c_void) };
        } else {
            // Read the header to find the original pointer
            let header_ptr = unsafe { (ptr as *mut usize).sub(1) };
            let raw_addr = unsafe { *header_ptr };
            unsafe { raw::mempool_free(raw_addr as *mut libc::c_void) };
        }
    }
}

/// Helpers for dice-aware thread-local storage and thread IDs.
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
        // Safety: if dice-self is linked this will return a valid id
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
        #[inline(always)]
        fn cell_ptr(&self, mt: &mut Metadata) -> *mut TlsCell<T> {
            let self_key = &self as *const _ as *const libc::c_void;
            // SAFETY: We assume dice correctly returns a pointer for TLS storage.
            // in debug build we do an additional sanity check that this holds.
            let raw = unsafe { raw::thread::self_tls(mt, self_key, size_of::<TlsCell<T>>()) };
            let ptr = raw as *mut TlsCell<T>;
            debug_assert!(ptr.is_aligned() && !ptr.is_null(), "Sanity Check");
            ptr
        }

        #[inline]
        pub fn with<R>(&self, mt: &mut Metadata, f: impl FnOnce(&mut T) -> R) -> R {
            // SAFETY: see the safety contract of `Self::get_mut`
            let t = unsafe { self.get_mut(mt) };
            f(t)
        }

        #[inline]
        /// # SAFETY
        /// this function requires a valid &'a mut Metdata, which forces unique and exclusive usage.
        /// Furthermore, as Metadata is !Send and !Sync, we also enforce that this thread local object will stay there.
        /// TODO: we could even consider `Pin` type.
        pub unsafe fn get_mut<'a>(&self, mt: &'a mut Metadata) -> &'a mut T {
            let cell = self.cell_ptr(mt);
            // SAFETY: &'a mut Metadata ensures they have the same lifetime and metadata can only be borrowed once
            // as metadata is unique per subscribe call and is !Send & !Sync, this is safe.
            let cell = unsafe { &mut *cell };
            // TODO: consider using (#[cold] based) unlikely here as this only happens once
            if !cell.initialized {
                cell.initialized = true;
                // SAFETY: external memory, so a volatile write
                unsafe { std::ptr::write_volatile((*cell).value.as_mut_ptr(), T::default()) };
            }
            // SAFETY: this value is initialized now
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
/// # Warning
/// The priority must be > 4 to not conflict with dice interals
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
            let Some(ev_ref) = (unsafe { <$t as $crate::DiceEvent>::from_raw(event as _) }) else {
                return $crate::DiceResult::Invalid;
            };

            let __chain = chain;

            let $e: &$t = ev_ref;
            let Some($m) = (unsafe { md.as_mut() }) else {
                return $crate::DiceResult::Invalid;
            };

            $body
        }

        // SAFETY: a valid c function is supplied, the chain is typed/variance safe
        // the prio must be > 4 is also confirmed. for Priority <= 4, conflicts with dice
        // internals could happen.
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
