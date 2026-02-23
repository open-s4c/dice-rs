use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicUsize},
};

use dice_rs::{
    Chain, DiceEvent, DiceResult, DiceThreadId, Metadata, TypeId, events::*, init_dice_state,
    subscribe, thread::*, tls_key,
};
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub struct Counter(AtomicUsize);

#[allow(unused)]
impl Counter {
    pub const fn new(init: usize) -> Self {
        Self(AtomicUsize::new(init))
    }

    #[inline]
    pub fn fetch_add(&self, n: usize, order: Ordering) -> usize {
        self.0.fetch_add(n, order)
    }

    #[inline]
    pub fn fetch_sub(&self, n: usize, order: Ordering) -> usize {
        self.0.fetch_sub(n, order)
    }

    #[inline]
    pub fn load(&self, order: Ordering) -> usize {
        self.0.load(order)
    }

    #[inline]
    pub fn store(&self, val: usize, order: Ordering) {
        self.0.store(val, order)
    }
}

#[derive(Debug, Default)]
struct Recorder {
    initd: bool,
    thread_id: Option<DiceThreadId>,
    order: Vec<usize>,
    ids: Vec<TypeId>,
}

init_dice_state! {
    log_level: log::LevelFilter::Debug
}

tls_key!(RECORDER: Recorder);

static GLOBAL_COUNTER: OnceLock<Counter> = OnceLock::new();

pub fn counter() -> &'static Counter {
    GLOBAL_COUNTER.get_or_init(|| Counter::new(0))
}

static LOCKED: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn release_lock(lock: &'static AtomicBool) {
    lock.store(false, Ordering::Release);
}

#[inline]
pub fn acquire_lock(lock: &'static AtomicBool) {
    loop {
        let lock = lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed);
        if lock == Ok(false) {
            break;
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct GlobalAtomicRecord {
    pub event: TypeId,
    pub global_index: usize,
}

impl GlobalAtomicRecord {
    pub fn new<T: DiceEvent>() -> Self {
        let event = T::ID;
        let global_index = counter().fetch_add(1, Ordering::Relaxed);
        Self {
            event,
            global_index,
        }
    }
}

impl Recorder {
    pub fn initialize(&mut self, thread_id: DiceThreadId) {
        if self.initd {
            return;
        }
        self.initd = true;
        self.thread_id = Some(thread_id);
    }

    pub fn end(&mut self) {
        if !self.initd {
            return;
        }
        if let Some(tid) = self.thread_id {
            let dir_path = "records";

            if let Err(e) = std::fs::create_dir_all(dir_path) {
                log::error!("failed to create directory {dir_path}: {e}");
                return;
            }

            let filename = format!("{dir_path}/record_{tid}.txt");
            let contents = format!("{:?}", &self.order[..]);
            if let Err(e) = std::fs::write(&filename, contents) {
                log::error!("failed to write {filename}: {e}");
            }

            let filename2 = format!("{dir_path}/types_{tid}.txt");
            let contents2 = format!("{:?}", &self.ids[..]);
            if let Err(e) = std::fs::write(&filename2, contents2) {
                log::error!("failed to write {filename2}: {e}");
            }

            log::debug!("wrote files: {filename}, {filename2}");
        }
    }

    pub fn record_event<T: DiceEvent>(&mut self) {
        assert!(self.initd);
        let record = GlobalAtomicRecord::new::<T>();
        self.order.push(record.global_index);
        self.ids.push(record.event);
    }
}

subscribe!(Chain::CaptureEvent, 9999, |_event: &SelfInitEvent, meta| {
    let thread_id = self_id(meta);

    if thread_id != 1 {
        release_lock(&LOCKED);
    }

    RECORDER.with(meta, |rec| {
        rec.initialize(thread_id);
    });

    DiceResult::Ok
});

subscribe!(Chain::CaptureEvent, 9999, |_event: &SelfFiniEvent, meta| {
    let thread_id = self_id(meta);
    RECORDER.with(meta, |rec| {
        rec.end();
    });
    DiceResult::Ok
});

fn generic_record<T: DiceEvent>(meta: &mut Metadata) {
    RECORDER.with(meta, |rec| {
        rec.record_event::<T>();
    });
}

subscribe!(
    Chain::CaptureBefore,
    9999,
    |_event: &PthreadCreateEvent, meta| {
        acquire_lock(&LOCKED);
        generic_record::<PthreadCreateEvent>(meta);
        DiceResult::Ok
    }
);

subscribe!(Chain::CaptureBefore, 9999, |_event: &MaAreadEvent, meta| {
    acquire_lock(&LOCKED);
    generic_record::<MaAreadEvent>(meta);
    DiceResult::Ok
});

subscribe!(Chain::CaptureAfter, 9999, |_event: &MaAreadEvent, meta| {
    release_lock(&LOCKED);
    DiceResult::Ok
});

subscribe!(
    Chain::CaptureBefore,
    9999,
    |_event: &MaAwriteEvent, meta| {
        acquire_lock(&LOCKED);
        generic_record::<MaAwriteEvent>(meta);
        DiceResult::Ok
    }
);

subscribe!(Chain::CaptureAfter, 9999, |_event: &MaAwriteEvent, meta| {
    release_lock(&LOCKED);
    DiceResult::Ok
});

subscribe!(
    Chain::CaptureBefore,
    9999,
    |_event: &MaCmpxchgEvent, meta| {
        acquire_lock(&LOCKED);
        generic_record::<MaCmpxchgEvent>(meta);
        DiceResult::Ok
    }
);

subscribe!(
    Chain::CaptureAfter,
    9999,
    |_event: &MaCmpxchgEvent, meta| {
        release_lock(&LOCKED);
        DiceResult::Ok
    }
);

subscribe!(Chain::CaptureBefore, 9999, |_event: &MallocEvent, meta| {
    acquire_lock(&LOCKED);
    generic_record::<MallocEvent>(meta);
    DiceResult::Ok
});

subscribe!(Chain::CaptureAfter, 9999, |_event: &MallocEvent, meta| {
    release_lock(&LOCKED);
    DiceResult::Ok
});
