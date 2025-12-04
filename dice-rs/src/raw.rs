use crate::{Chain, DiceResult, Metadata, TypeId};
use libc::c_void;

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
    pub fn ps_subscribe(chain: Chain, ty: TypeId, cb: PsCallbackF, prio: i32) -> i32;
    pub fn ps_publish(chain: Chain, ty: TypeId, event: *const c_void, md: *mut Metadata) -> i32;
}

#[cfg(feature = "dice-self")]
pub mod thread {
    use crate::DiceThreadId;

    #[repr(C)]
    pub struct TlsDestructor {
        pub free: Option<extern "C" fn(arg: *mut core::ffi::c_void, ptr: *mut core::ffi::c_void)>,
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
        pub fn self_id(mt: *mut Metadata) -> DiceThreadId;
        pub fn self_retired(mt: *mut Metadata) -> bool;
        pub fn self_tls(mt: *mut Metadata, key: *const c_void, size: usize) -> *mut libc::c_void;
        pub fn self_tls_get(mt: *mut Metadata, key: libc::uintptr_t) -> *mut libc::c_void;
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
    pub fn mempool_aligned_alloc(alignment: libc::size_t, size: libc::size_t) -> *mut libc::c_void;
    pub fn mempool_alloc(size: libc::size_t) -> *mut libc::c_void;
    pub fn mempool_free(ptr: *mut libc::c_void);
}

#[link(name = "shim", kind = "static")]
unsafe extern "C" {
    pub fn dice_log_write(level: i32, msg: *const std::os::raw::c_char);
}
