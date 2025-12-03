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

pub mod thread {
    use crate::DiceThreadId;

    use super::*;
    #[link(name = "dice", kind = "static")]
    unsafe extern "C" {
        pub fn self_id(mt: *mut Metadata) -> DiceThreadId;
        pub fn self_retired(mt: *mut Metadata) -> bool;
        pub fn self_tls(mt: *mut Metadata, key: *const c_void, size: usize) -> *mut c_void;

    }
}

#[link(name = "shim", kind = "static")]
unsafe extern "C" {
    pub fn dice_log_write(level: i32, msg: *const std::os::raw::c_char);
}
