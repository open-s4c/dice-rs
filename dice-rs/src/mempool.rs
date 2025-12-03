use std::ffi::c_void;
use std::ptr;
use std::sync::Mutex;
use std::mem::{self, size_of};
use libc::size_t;

const SIZES: [usize; 9] = [
    32, 128, 512, 1024, 2048, 8192, 1024 * 1024, 4 * 1024 * 1024, 8 * 1024 * 1024,
];
const MEMPOOL_SIZE: usize = 1024 * 1024 * 200;
const MEMPOOL_ALIGNMENT: usize = 8;

#[repr(C)]
struct Entry {
    next: *mut Entry,
    size: usize,
    data: [c_void; 0],
}

pub struct Mempool {
    lock: Mutex<Pool>,
}

pub struct Pool {
    allocated: usize,
    stack: [*mut Entry; SIZES.len()],
    capacity: usize,
    next: usize,
    memory: *mut u8,
}

// SAFETY: Pool is expected to be allocated statically
unsafe impl Send for Pool {}

static MEMPOOL: Mempool = Mempool {
    lock: Mutex::new(Pool {
        allocated: 0,
        stack: [ptr::null_mut(); SIZES.len()],
        capacity: MEMPOOL_SIZE,
        next: 0,
        memory: ptr::null_mut(),
    }),
};

fn bucketize(size: usize) -> Option<usize> {
    SIZES.iter()
        .enumerate()
        .find(|(_, bucket_size)| size <= **bucket_size)
        .map(|(bucket_index, _)| bucket_index)
}

impl Entry {
    fn as_raw_data_ptr(&mut self) -> *mut c_void {
        self.data.as_mut_ptr()
    }

    fn from_raw_data_ptr(ptr: *mut c_void) -> &'static mut Self {
        // SAFETY: as long as ptr is obtained from to_raw_data_ptr(), it is safe
        unsafe { &mut *(ptr as *mut Entry).sub(1) }
    }
}

impl Pool {
    fn ensure_initialized(&mut self) {
        if self.memory.is_null() {
            self.init(MEMPOOL_SIZE);
        }
    }

    fn init(&mut self, cap: usize) {
        // SAFETY: as safe as calling dlsym()
        let real_malloc_ptr = unsafe {
            libc::dlsym(
                libc::RTLD_NEXT,
                c"malloc".as_ptr() as *const std::os::raw::c_char,
            )
        };
        if real_malloc_ptr.is_null() {
            panic!("Could not find real malloc");
        }
        // SAFETY: non-null real_malloc_ptr returned by dlsym() is a valid C function pointer
        let real_malloc: extern "C" fn(size_t) -> *mut c_void = unsafe {
            mem::transmute(real_malloc_ptr)
        };
        let memory = real_malloc(cap + MEMPOOL_ALIGNMENT - 1);
        assert!(!memory.is_null());
        let memory_uintptr = memory as usize;
        let aligned_memory_uintptr = memory_uintptr + MEMPOOL_ALIGNMENT - 1 - ((memory_uintptr + MEMPOOL_ALIGNMENT - 1 + size_of::<Entry>()) % MEMPOOL_ALIGNMENT);
        self.memory = aligned_memory_uintptr as *mut u8;
    }
}

impl Mempool {
    fn alloc(&self, n: size_t) -> *mut c_void {
        let size = n + size_of::<Entry>();
        let bucket = bucketize(size).expect("the maximum bucket size should be sufficient");
        let bucket_size = SIZES[bucket];
        let mut pool = self.lock.lock().unwrap();
        let stack = &mut pool.stack[bucket];

        if !(*stack).is_null() {
            let entry_ptr = *stack;
            // SAFETY: stack entry pointers must be valid
            let entry_ref: &mut Entry = unsafe { &mut (*entry_ptr) };
            *stack = entry_ref.next;
            entry_ref.next = ptr::null_mut();
            pool.allocated += bucket_size;
            return entry_ref.as_raw_data_ptr();
        }

        pool.ensure_initialized();

        if pool.capacity >= pool.next + bucket_size {
            // SAFETY: safe due to size check in the condition
            // and the fact that all contents of Entry are overwritten
            let entry_ptr = unsafe { &mut *(pool.memory.add(pool.next) as *mut Entry) };
            entry_ptr.next = ptr::null_mut();
            entry_ptr.size = n;
            pool.next += bucket_size;
            pool.allocated += bucket_size;
            return (*entry_ptr).as_raw_data_ptr();
        }

        ptr::null_mut()
    }

    fn free(&self, ptr: *mut c_void) {
        assert!(!ptr.is_null());
        let entry = Entry::from_raw_data_ptr(ptr);

        let size = entry.size + size_of::<Entry>();
        let bucket = bucketize(size).expect("previously allocated bucket size must exist");
        let mut pool = self.lock.lock().expect("there should be no panics inside mempool");
        pool.allocated -= size;
        let stack = &mut pool.stack[bucket];
        entry.next = *stack;
        *stack = entry;
    }

    fn realloc(&self, ptr: *mut c_void, new_size: size_t) -> *mut c_void {
        let new_ptr = self.alloc(new_size);
        if new_ptr.is_null() || ptr.is_null() {
            return new_ptr;
        }
        let entry = Entry::from_raw_data_ptr(ptr);
        let copy_size = entry.size.min(new_size);
        // SAFETY: as long as ptr comes from mempool, (copy_nonoverlapping) should succeed
        unsafe {
            ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);
        }
        self.free(ptr);
        new_ptr
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mempool_alloc(n: size_t) -> *mut c_void {
    MEMPOOL.alloc(n)
}

#[unsafe(no_mangle)]
pub extern "C" fn mempool_free(ptr: *mut c_void) {
    MEMPOOL.free(ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn mempool_realloc(ptr: *mut c_void, new_size: size_t) -> *mut c_void {
    MEMPOOL.realloc(ptr, new_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc() {
        let size = 64;
        let ptr = mempool_alloc(size);

        assert!(!ptr.is_null());

        let entry = unsafe { &mut *(ptr as *mut Entry).sub(1) };

        unsafe {
            ptr::write_bytes(ptr, 0, size);
        }

        assert_eq!(entry.size, size);
        assert_eq!(ptr as usize % 8, 0);
    }

    #[test]
    fn test_free() {
        let size = 64;
        let ptr = mempool_alloc(size);

        assert!(!ptr.is_null());

        mempool_free(ptr);

        let ptr2 = mempool_alloc(size);
        assert!(!ptr2.is_null());
        assert_eq!(ptr, ptr2);
    }

    #[test]
    fn test_realloc_grows() {
        let initial_size = 64;
        let ptr = mempool_alloc(initial_size);
        assert!(!ptr.is_null());

        let new_size = 128;
        let new_ptr = mempool_realloc(ptr, new_size);
        assert!(!new_ptr.is_null());

        let entry = unsafe { &mut *(new_ptr as *mut Entry).sub(1) };
        assert_eq!(entry.size, new_size);
    }

    #[test]
    fn test_realloc_shrinks() {
        let initial_size = 128;
        let ptr = mempool_alloc(initial_size);
        assert!(!ptr.is_null());

        let new_size = 64;
        let new_ptr = mempool_realloc(ptr, new_size);
        assert!(!new_ptr.is_null());

        let entry = unsafe { &mut *(new_ptr as *mut Entry).sub(1) };
        assert_eq!(entry.size, new_size);
    }

    #[test]
    fn test_realloc_preserves_data() {
        let size = 64;
        let ptr = mempool_alloc(size);
        assert!(!ptr.is_null());

        let data = unsafe { &mut *(ptr as *mut u8) };
        *data = 42;

        let new_size = 128;
        let new_ptr = mempool_realloc(ptr, new_size);
        assert!(!new_ptr.is_null());

        let new_data = unsafe { &mut *(new_ptr as *mut u8) };
        assert_eq!(*new_data, 42);
    }

    #[test]
    fn test_bucketize_function() {
        assert_eq!(bucketize(32), Some(0));
        assert_eq!(bucketize(128), Some(1));
        assert_eq!(bucketize(500), Some(2));
        assert_eq!(bucketize(1024), Some(3));
        assert_eq!(bucketize(2048), Some(4));
        assert_eq!(bucketize(8192), Some(5));
        assert_eq!(bucketize(1024 * 1024), Some(6));
        assert_eq!(bucketize(4 * 1024 * 1024), Some(7));
        assert_eq!(bucketize(8 * 1024 * 1024), Some(8));
        assert_eq!(bucketize(8 * 1024 * 1024 + 1), None);
    }

    #[test]
    fn test_memory_initialization() {
        let mut pool = Pool {
            allocated: 0,
            stack: [ptr::null_mut(); SIZES.len()],
            capacity: MEMPOOL_SIZE,
            next: 0,
            memory: ptr::null_mut(),
        };

        pool.ensure_initialized();
        assert!(!pool.memory.is_null());
    }

    #[test]
    fn test_as_raw_data_ptr() {
        let mut entry = Entry {
            next: ptr::null_mut(),
            size: 0xDEADBEEF,
            data: [],
        };

        let reconstructed_entry = Entry::from_raw_data_ptr(entry.as_raw_data_ptr());

        assert_eq!(reconstructed_entry.size, entry.size);
    }
}
