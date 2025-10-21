use crate::{DiceEvent, TypeId};
use dice_derive::dice_event;
use libc::{c_char, c_int, c_void};

// TODO: autogenerate this
pub mod raw {
    use crate::TypeId;

    pub const EVENT_THREAD_START: TypeId = 1;
    pub const EVENT_THREAD_EXIT: TypeId = 2;
    pub const EVENT_THREAD_CREATE: TypeId = 3;
    pub const EVENT_THREAD_JOIN: TypeId = 4;

    pub const EVENT_SELF_INIT: TypeId = 5;
    pub const EVENT_SELF_FINI: TypeId = 6;

    pub const EVENT_MA_READ: TypeId = 30;
    pub const EVENT_MA_WRITE: TypeId = 31;
    pub const EVENT_MA_AREAD: TypeId = 32;
    pub const EVENT_MA_AWRITE: TypeId = 33;
    pub const EVENT_MA_RMW: TypeId = 34;
    pub const EVENT_MA_XCHG: TypeId = 35;
    pub const EVENT_MA_CMPXCHG: TypeId = 36;
    pub const EVENT_MA_CMPXCHG_WEAK: TypeId = 37;
    pub const EVENT_MA_FENCHE: TypeId = 38;

    pub const EVENT_MALLOC: TypeId = 50;
    pub const EVENT_CALLOC: TypeId = 51;
    pub const EVENT_REALLOC: TypeId = 52;
    pub const EVENT_FREE: TypeId = 53;
    pub const EVENT_POSIX_MEMALIGN: TypeId = 54;
    pub const EVENT_ALIGNED_ALLOC: TypeId = 55;
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_THREAD_START)]
pub struct ThreadStartEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_THREAD_EXIT)]
pub struct ThreadExitEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_THREAD_CREATE)]
pub struct ThreadCreateEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_THREAD_JOIN)]
pub struct ThreadJoinEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_SELF_INIT)]
pub struct SelfInitEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_SELF_FINI)]
pub struct SelfFiniEvent;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MALLOC)]
pub struct MallocEvent {
    pub pc: *const (),
    pub size: usize,
    pub ret: *const (),
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_READ)]
pub struct ReadEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_WRITE)]
pub struct WriteEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_AREAD)]
pub struct AtomicReadEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_AWRITE)]
pub struct AtomicWriteEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_XCHG)]
pub struct AtomicXCHGEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
    pub old: ma_val,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_RMW)]
pub struct AtomicRMWEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
    pub old: ma_val,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_CMPXCHG)]
pub struct AtomicCMPEXCHGEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
    pub cmp: ma_val,
    pub old: ma_val,
    pub ok: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_CMPXCHG_WEAK)]
pub struct AtomicCMPEXCHGWeakEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub addr: *mut c_void,
    pub size: usize,
    pub mo: c_int,
    pub val: ma_val,
    pub cmp: ma_val,
    pub old: ma_val,
    pub ok: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MA_FENCHE)]
pub struct FenceEvent {
    pub pc: *const c_void,
    pub func: *const c_char,
    pub mo: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union ma_val {
    pub u8_: u8,
    pub u16_: u16,
    pub u32_: u32,
    pub u64_: u64,
}

impl core::fmt::Debug for ma_val {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let v = unsafe { self.u64_ };
        write!(f, "ma_val({:#x} / {})", v, v)
    }
}
