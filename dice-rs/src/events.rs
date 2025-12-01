use crate::{DiceEvent, TypeId};
use dice_derive::dice_event;
use libc::{c_char, c_int, c_long, off_t, c_void, pthread_t, pthread_mutex_t, pthread_attr_t, pthread_cond_t, pthread_rwlock_t, timespec};

// TODO: autogenerate this
pub mod raw {
    use crate::TypeId;

    pub const EVENT_THREAD_START: TypeId = 1;
    pub const EVENT_THREAD_EXIT: TypeId = 2;
    pub const EVENT_THREAD_CREATE: TypeId = 3;
    pub const EVENT_THREAD_JOIN: TypeId = 4;

    pub const EVENT_SELF_INIT: TypeId = 5;
    pub const EVENT_SELF_FINI: TypeId = 6;

    pub const EVENT_MUTEX_LOCK: TypeId = 12;
    pub const EVENT_MUTEX_TIMEDLOCK: TypeId = 13;
    pub const EVENT_MUTEX_TRYLOCK: TypeId = 14;
    pub const EVENT_MUTEX_UNLOCK: TypeId = 15;
    pub const EVENT_COND_WAIT: TypeId = 16;
    pub const EVENT_COND_TIMEDWAIT: TypeId = 17;
    pub const EVENT_COND_SIGNAL: TypeId = 18;
    pub const EVENT_COND_BROADCAST: TypeId = 19;
    pub const EVENT_RWLOCK_RDLOCK: TypeId = 20;
    pub const EVENT_RWLOCK_TIMEDRDLOCK: TypeId = 21;
    pub const EVENT_RWLOCK_TRYRDLOCK: TypeId = 22;
    pub const EVENT_RWLOCK_WRLOCK: TypeId = 23;
    pub const EVENT_RWLOCK_TIMEDWRLOCK: TypeId = 24;
    pub const EVENT_RWLOCK_TRYWRLOCK: TypeId = 25;
    pub const EVENT_RWLOCK_UNLOCK: TypeId = 26;

    pub const EVENT_MA_READ: TypeId = 30;
    pub const EVENT_MA_WRITE: TypeId = 31;
    pub const EVENT_MA_AREAD: TypeId = 32;
    pub const EVENT_MA_AWRITE: TypeId = 33;
    pub const EVENT_MA_RMW: TypeId = 34;
    pub const EVENT_MA_XCHG: TypeId = 35;
    pub const EVENT_MA_CMPXCHG: TypeId = 36;
    pub const EVENT_MA_CMPXCHG_WEAK: TypeId = 37;
    pub const EVENT_MA_FENCHE: TypeId = 38;

    // missing structs
    pub const EVENT_STACKTRACE_ENTER: TypeId = 40;
    pub const EVENT_STACKTRACE_EXIT: TypeId = 41;
    
    pub const EVENT_ANNOTATE_RWLOCK_CREATE: TypeId = 42;
    pub const EVENT_ANNOTATE_RWLOCK_DESTROY: TypeId = 43;
    pub const EVENT_ANNOTATE_RWLOCK_ACQ: TypeId = 44;
    pub const EVENT_ANNOTATE_RWLOCK_REL: TypeId = 45;

    pub const EVENT_MALLOC: TypeId = 50;
    pub const EVENT_CALLOC: TypeId = 51;
    pub const EVENT_REALLOC: TypeId = 52;
    pub const EVENT_FREE: TypeId = 53;
    pub const EVENT_POSIX_MEMALIGN: TypeId = 54;
    pub const EVENT_ALIGNED_ALLOC: TypeId = 55;

    pub const EVENT_CXA_GUARD_ACQUIRE: TypeId = 60;
    pub const EVENT_CXA_GUARD_RELEASE: TypeId = 61;
    pub const EVENT_CXA_GUARD_ABORT: TypeId = 62;

    // missing structs
    pub const EVENT_SEM_POST: TypeId = 70;
    pub const EVENT_SEM_WAIT: TypeId = 71;
    pub const EVENT_SEM_TRYWAIT: TypeId = 72;
    pub const EVENT_SEM_TIMEDWAIT: TypeId = 73;
    
    pub const EVENT_MMAP: TypeId = 80;
    pub const EVENT_MUNMAP: TypeId = 81;
    
    // missing structs
    pub const EVENT_DICE_INIT: TypeId = 99;    
    pub const EVENT_DICE_READY: TypeId = 98;

    pub const EVENT_MEMCPY: TypeId = 100;
    pub const EVENT_MEMMOVE: TypeId = 101;
    pub const EVENT_MEMSET: TypeId = 102;
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
#[dice_event(raw::EVENT_MUTEX_LOCK)]
pub struct MutexLockEvent {
    pub pc: *const c_void,
    pub mutex: *mut pthread_mutex_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MUTEX_TRYLOCK)]
pub struct MutexTrylockEvent {
    pub pc: *const c_void,
    pub mutex: *mut pthread_mutex_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MUTEX_UNLOCK)]
pub struct MutexUnlockEvent {
    pub pc: *const c_void,
    pub mutex: *mut pthread_mutex_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MUTEX_TIMEDLOCK)]
pub struct MutexTimedLockEvent {
    pub pc: *const c_void,
    pub mutex: *mut pthread_mutex_t,
    pub ret: c_int,
    pub timeout: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_COND_WAIT)]
pub struct CondWaitEvent {
    pub pc: *const c_void,
    pub cond: *mut pthread_cond_t,
    pub mutex: *mut pthread_mutex_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_COND_TIMEDWAIT)]
pub struct CondTimedWaitEvent {
    pub pc: *const c_void,
    pub cond: *mut pthread_cond_t,
    pub mutex: *mut pthread_mutex_t,
    pub abstime: *const timespec,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_COND_SIGNAL)]
pub struct CondSignalEvent {
    pub pc: *const c_void,
    pub cond: *mut pthread_cond_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_COND_BROADCAST)]
pub struct CondBroadcastEvent {
    pub pc: *const c_void,
    pub cond: *mut pthread_cond_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_RDLOCK)]
pub struct RWLockRDLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_TIMEDRDLOCK)]
pub struct RWLockTimedRDLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub abstime: *mut timespec,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_TRYRDLOCK)]
pub struct RWLockTryRDLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_WRLOCK)]
pub struct RWLockWRLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_TIMEDWRLOCK)]
pub struct RWLockTimedWRLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub abstime: *mut timespec,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_TRYWRLOCK)]
pub struct RWLockTryWRLockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_RWLOCK_UNLOCK)]
pub struct RWLockUnlockEvent {
    pub pc: *const c_void,
    pub lock: *mut pthread_rwlock_t,
    pub ret: c_int,
}


#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_CXA_GUARD_ACQUIRE)]
pub struct CxaGuardAcquireEvent {
    pub pc: *const c_void,
    pub addr: *mut c_void,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_CXA_GUARD_RELEASE)]
pub struct CxaGuardReleaseEvent {
    pub pc: *const c_void,
    pub addr: *mut c_void,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_CXA_GUARD_ABORT)]
pub struct CxaGuardAbortEvent {
    pub pc: *const c_void,
    pub addr: *mut c_void,
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
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_ANNOTATE_RWLOCK_CREATE)]
pub struct AnnotateRWLockCreateEvent {
    pub file: *const c_char,
    pub line: c_int,
    pub lock: *const c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_ANNOTATE_RWLOCK_DESTROY)]
pub struct AnnotateRWLockDestroyEvent {
    pub file: *const c_char,
    pub line: c_int,
    pub lock: *const c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_ANNOTATE_RWLOCK_ACQ)]
pub struct AnnotateRWLockAcquiredEvent {
    pub file: *const c_char,
    pub line: c_int,
    pub lock: *const c_void,
    pub is_w: c_long,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_ANNOTATE_RWLOCK_REL)]
pub struct AnnotateRWLockReleasedEvent {
    pub file: *const c_char,
    pub line: c_int,
    pub lock: *const c_void,
    pub is_w: c_long,
}


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
#[dice_event(raw::EVENT_CALLOC)]
pub struct CallocEvent {
    pub pc: *const c_void,
    pub number: usize,
    pub size: usize,
    pub ret: *const c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_REALLOC)]
pub struct ReallocEvent {
    pub pc: *const c_void,
    pub ptr: *const c_void,
    pub size: usize,
    pub ret: *const c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_FREE)]
pub struct FreeEvent {
    pub pc: *const c_void,
    pub ptr: *const c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_POSIX_MEMALIGN)]
pub struct PosixMemalignEvent {
    pub pc: *const c_void,
    pub ptr: *mut *const c_void,
    pub alignment: usize,
    pub size: usize,
    pub ret: c_int,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_ALIGNED_ALLOC)]
pub struct AlignedAllocEvent {
    pub pc: *const c_void,
    pub alignment: usize,
    pub size: usize,
    pub ret: *const c_void,
}


#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MMAP)]
pub struct MmapEvent {
    pub pc: *const c_void,
    pub addr: *mut c_void,
    pub length: usize,
    pub prot: c_int,
    pub flags: c_int,
    pub fd: c_int,
    pub offset: off_t,
    pub ret: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MUNMAP)]
pub struct MunmapEvent {
    pub pc: *const c_void,
    pub addr: *mut c_void,
    pub length: usize,
    pub ret: c_int,
}


#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MEMCPY)]
pub struct MemcpyEvent {
    pub pc: *const c_void,
    pub dest: *mut c_void,
    pub src: *const c_void,
    pub num: usize,
    pub ret: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MEMMOVE)]
pub struct MemmoveEvent {
    pub pc: *const c_void,
    pub dest: *mut c_void,
    pub src: *const c_void,
    pub count: usize,
    pub ret: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
#[dice_event(raw::EVENT_MEMSET)]
pub struct MemsetEvent {
    pub pc: *const c_void,
    pub ptr: *mut c_void,
    pub value: c_int,
    pub num: usize,
    pub ret: *mut c_void,
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
