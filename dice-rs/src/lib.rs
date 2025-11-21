pub mod events;
pub mod log;
pub mod raw;

pub type ChainId = u16;
pub type TypeId = u16;
pub type DiceThreadId = u64;
use std::alloc::{GlobalAlloc, Layout};

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

    #[inline]
    unsafe fn from_raw<'a>(ptr: *const ()) -> Option<&'a Self> {
        if ptr.is_null() {
            Self::fallback()
        } else {
            Some(unsafe { &*(ptr as *const Self) })
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

#[repr(C, align(8))]
#[derive(Copy, Clone, Debug)]
pub struct Metadata {
    pub drop_: bool,
}

pub struct MempoolAllocator;

unsafe impl GlobalAlloc for MempoolAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { raw::mempool_alloc(layout.size()) as *mut u8 }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { raw::mempool_free(ptr as *mut _) };
    }
}

pub mod thread {
    use std::{marker::PhantomData, mem::MaybeUninit};

    use crate::{DiceThreadId, Metadata, raw};

    pub fn self_id(mt: &mut Metadata) -> DiceThreadId {
        let id = unsafe { raw::thread::self_id(mt) };
        id
    }

    // Note: T here does not have to be repr(C) as we do the size calculation on the rust side
    #[repr(C)]
    struct TlsCell<T> {
        initialized: bool,
        value: MaybeUninit<T>,
    }

    pub struct TlsKey<T: Default> {
        _marker: PhantomData<T>,
    }

    impl<T: Default> TlsKey<T> {
        pub const fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }

        #[inline(always)]
        fn cell_ptr(&self, mt: &mut Metadata) -> *mut TlsCell<T> {
            unsafe {
                let raw = raw::thread::self_tls(
                    mt,
                    self as *const _ as *const _,
                    size_of::<TlsCell<T>>(),
                );
                raw as *mut TlsCell<T>
            }
        }

        #[inline]
        pub fn with<R>(&self, mt: &mut Metadata, f: impl FnOnce(&mut T) -> R) -> R {
            let t = unsafe { self.get_mut(mt) };
            f(t)
        }

        #[inline]
        pub unsafe fn get_mut<'a>(&self, mt: &'a mut Metadata) -> &'a mut T {
            let cell = self.cell_ptr(mt);
            if unsafe { !(*cell).initialized } {
                unsafe { std::ptr::write((*cell).value.as_mut_ptr(), T::default()) };
                unsafe { (*cell).initialized = true };
            }
            unsafe { &mut *(*cell).value.as_mut_ptr() }
        }
    }

    #[macro_export]
    macro_rules! tls_key {
        ($name:ident : $ty:ty) => {
            static $name: TlsKey<$ty> = TlsKey::new();
        };
    }
}

#[macro_export]
macro_rules! subscribe_scoped {
    ($chain:expr, $prio:expr, |$e:ident: &$t:ty, $m:ident| $body:block) => {{
        // no capture guard
        let _guard: fn(&$t, &mut $crate::Metadata) -> $crate::DiceResult =
            |$e: &$t, $m: &mut $crate::Metadata| $body;

        extern "C" fn __trampoline(
            chain: $crate::Chain,
            _ty: $crate::TypeId,
            event: *const core::ffi::c_void,
            md: *mut $crate::Metadata,
        ) -> $crate::DiceResult {
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

#[macro_export]
macro_rules! init_dice_state {
    () => {
        #[global_allocator]
        static GLOBAL: $crate::MempoolAllocator = $crate::MempoolAllocator;

        #[::ctor::ctor]
        fn __init_log() {
            $crate::log::init(log::LevelFilter::Debug);
        }
    };
}
