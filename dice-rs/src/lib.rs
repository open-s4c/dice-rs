pub mod log;
pub mod raw;

pub type ChainId = u16;
pub type TypeId = u16;
pub type DiceThreadId = u64;
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

pub trait DiceEvent: Sized {
    const ID: TypeId;

    fn fallback<'a>() -> Option<&'a Self> {
        None
    }

    /// This function is intended to be used for casting a void pointer from a dice callback to a Rust reference.
    /// If the Event does not require any data, dice will not provide a type/struct for this and return a nullpointer
    /// The `#[dice_event(...)]` attribute macro correctly handles this and implements a fallback.
    /// The fallback is a &T which is a valid reference for unit structs (structs without a body)
    /// # Safety
    /// usage of a valid pointer to a Self or a nullpointer
    /// if the type doesn't implement a fallback and the pointer is null, it will return None
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

/// Metadata is given in a dice subscribe callback and is constructed in publishers
/// For example: it is used to access "internal" properties like thread id in Capture chain callbacks.
/// due to private fields it is not possible to construct Metdata (safely)
/// it is also !Send and !Sync (from the _marker: PhatnomData<*mut ()>) and no Copy nor Clone.
/// this is to ensure uniqueness and locality of metadata usage within callbacks.
#[repr(C, align(8))]
#[derive(Debug)]
pub struct Metadata {
    drop_: bool,
    // This marker makes Metadata !Send and !Sync
    _marker: PhantomData<*mut ()>,
}

/// Allocator backed by dice's Allocator
pub struct MempoolAllocator;

/// # Safety
/// 
/// * the current implementation does unwind, which is UB by definition here. 
///   This is so we can know if dice is wrong. 
///   There are no good alternatives, because this means the core C dependency itself is broken.
/// 
/// * Dice needs to be patched to use 8 byte alignment. Then less than equal 8 byte alignment works.
///   everything else from [`std::alloc::GlobalAlloc`] applies 
///   TODO: update this once dice allows arbitrary alignment or #106 being merged.
///   we can also consider doing alignment ourselves 
/// 
/// * the allocator is from rust's perspective stateless and makes no assumptions about allocations happening.
///   the rust optimizer is free to not allocate or overallocate.
unsafe impl GlobalAlloc for MempoolAllocator {
    
    /// returns a pointer to correctly sized memory.
    /// 
    /// # Errors
    /// currently the alignment is ignored. but 8 byte alignment can be configured in dice
    /// 
    /// # Safety
    /// to get stable 8 byte alignment, dice needs to be patched: #106 Memory Alignment Patch
    /// 8 byte alignment seems to be enough for now.
    /// We do a check here to see if this contract holds, so we can know if we run into requirements we currently cannot guarantee
    /// this will panic and unwind, which breaks the usual GlobalAlloc contract, 
    /// but if we abort, we would not get the information we want.
    /// 
    /// everything else from [`std::alloc::GlobalAlloc::alloc`] applies
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { raw::mempool_alloc(layout.size()) as *mut u8 };
        assert!(
            !ptr.is_null() && ptr as usize % layout.align() == 0,
            "Requested alignment {} but got {}", layout.align(), 2 ^ (ptr as usize).trailing_zeros()
        );
        ptr
    }

    /// # Safety
    /// the current implementation does ignore the layout, but this is a TODO to be changed.
    /// everything else from [`std::alloc::GlobalAlloc::dealloc`] applies
    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let ptr = ptr as *mut _;
        unsafe { raw::mempool_free(ptr) };
    }
}

#[cfg(feature = "dice-self")]
pub mod thread {
    use std::{marker::PhantomData, mem::MaybeUninit};

    use crate::{DiceThreadId, Metadata, raw};

    /// get the thread id
    /// if this is called outside a valid thread it will return 0
    /// SAFETY:
    /// requires dice-self
    pub fn self_id(mt: &mut Metadata) -> DiceThreadId {
        // SAFETY: dice will return a number >= 1 if it is a thread, and 0 if it is not within a thread
        // it requires dice-self
        unsafe { raw::thread::self_id(mt) }
    }

    // Note: T here does not have to be repr(C) as we do the size calculation on the rust side
    #[repr(C)]
    struct TlsCell<T> {
        initialized: bool,
        value: MaybeUninit<T>,
    }

    pub struct TlsKey<T> {
        _marker: PhantomData<T>,
    }

    // this is kind of useless, but clippy wants this.
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
            // SAFETY: We assume dice correctly returns a pointer for TLS storage.
            // in debug build we do an additional sanity check that this holds.
            let raw = unsafe {
                raw::thread::self_tls(
                    mt,
                    self as *const _ as *const _,
                    size_of::<TlsCell<T>>(),
                )
            };
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
// this subscribe macro emulates a normal rust anonymous function structure.
// it creates a c callback and subscribes it automatically
// it is type, lifetime and capture guarded using the _guard
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

        extern "C" fn __trampoline(
            chain: $crate::Chain,
            _ty: $crate::TypeId,
            event: *const core::ffi::c_void,
            md: *mut $crate::Metadata,
        ) -> $crate::DiceResult {
            // SAFETY: the dice subscribe callback either gives a correctly typed pointer for the event
            // or it gives a null in case the Event struct is empty.
            // in this case an empty Fallback is used.
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

        // SAFETY
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
