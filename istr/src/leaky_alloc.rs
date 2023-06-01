use std::{alloc::Layout, cell::Cell, ops::Deref, ptr::NonNull};

// start of with a megabyte of storage, this should usualy be all that's needed
// for the entire program, and usually there shouldn't be any strings larger than
// a megabyte
const INITIAL_SIZE: usize = 1024 * 1024;

#[cfg(miri)]
use std::sync::{Mutex, PoisonError};

#[cfg(miri)]
static LEAKED_MEMORY: Mutex<Vec<FrozenLeakyAllocPtr>> = Mutex::new(Vec::new());

struct FrozenLeakyAllocPtr(*mut ());

unsafe impl Send for FrozenLeakyAllocPtr {}
unsafe impl Sync for FrozenLeakyAllocPtr {}

#[derive(Clone)]
struct LeakyAllocHandle(*mut LeakyAlloc);

thread_local! {
    static ALLOC: Cell<LeakyAllocHandle> = Cell::new(LeakyAllocHandle(LeakyAlloc::new_::<true>()))
}

#[cfg(miri)]
fn register_leaked(ptr: *mut ()) {
    let guard = &mut *LEAKED_MEMORY.lock().unwrap_or_else(PoisonError::into_inner);

    // this is done to get around miri's leak check
    guard.push(FrozenLeakyAllocPtr(ptr));
}

#[cfg(miri)]
impl Drop for LeakyAllocHandle {
    fn drop(&mut self) {
        register_leaked(self.0.cast())
    }
}

#[repr(C)]
pub struct LeakyAlloc {
    // this is only used to prevent sanitizers from detecting the leaked memory
    ptr: *mut u8,
    layout: Layout,
    prev: *mut LeakyAlloc,
    data: [u8; 0],
}

fn get_alloc() -> *mut LeakyAlloc {
    ALLOC.with(|alloc| unsafe { (*alloc.as_ptr()).0 })
}

impl LeakyAlloc {
    #[cold]
    fn new() -> *mut LeakyAlloc {
        Self::new_::<false>()
    }

    #[cold]
    fn new_<const FIRST: bool>() -> *mut LeakyAlloc {
        let (layout, prev) = if FIRST {
            (
                Layout::from_size_align(INITIAL_SIZE, 16).unwrap(),
                core::ptr::null_mut(),
            )
        } else {
            let prev_ptr = get_alloc();
            let prev = unsafe { &*prev_ptr };
            (prev.layout.extend(prev.layout).unwrap().0, prev_ptr)
        };

        let ptr = unsafe { std::alloc::alloc(layout) };

        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout)
        }

        let end = unsafe { ptr.add(layout.size()) };
        let ptr = ptr.cast::<LeakyAlloc>();

        unsafe {
            ptr.write(LeakyAlloc {
                prev,
                ptr: end,
                layout,
                data: [],
            })
        }

        ptr
    }
}

#[cold]
#[inline(never)]
pub fn alloc(size: usize) -> *mut u8 {
    const ALIGN: usize = std::mem::align_of::<InternedStringHeader>();
    const ALIGN_MASK: usize = !ALIGN.wrapping_sub(1);

    let mut ptr = get_alloc();

    let mut start = unsafe { core::ptr::addr_of!((*ptr).data).cast::<u8>() };
    let mut header = unsafe { &mut *ptr };

    let remaining = unsafe { header.ptr.offset_from(start) as usize };

    debug_assert_eq!(remaining % ALIGN, 0);

    if remaining >= size {
        // already enough space
    } else if header.layout.size() >= size / 2 {
        // create a new leaky alloc, since it is guarnateed to be larger than the string

        ptr = LeakyAlloc::new();
        ALLOC.set(LeakyAllocHandle(ptr));

        start = unsafe { core::ptr::addr_of!((*ptr).data).cast::<u8>() };
        header = unsafe { &mut *ptr };
    } else {
        // super large string, just give it a dedicated allocation

        let layout = Layout::from_size_align(size, ALIGN).unwrap();

        let ptr = unsafe { std::alloc::alloc(layout) };

        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout)
        }

        #[cfg(miri)]
        register_leaked(ptr.cast());

        return ptr;
    }

    #[allow(unused_variables)]
    let remaining = ();

    let current = unsafe { header.ptr.sub(size) };
    let addr = current.addr() & ALIGN_MASK;
    let current = current.with_addr(addr);
    header.ptr = current;

    debug_assert!(current as *const u8 >= start);
    debug_assert!(current as *const u8 <= unsafe { (ptr as *const u8).add(header.layout.size()) });

    current
}

type USIZE = u32;

#[repr(C)]
pub(crate) struct InternedStringHeader {
    hash: u64,
    len: USIZE,
    data: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IStr(NonNull<InternedStringHeader>);

unsafe impl Send for IStr {}
unsafe impl Sync for IStr {}

#[cfg(test)]
pub(crate) fn new(s: &str) -> IStr {
    with_hash(s, crate::hasher::hash(s))
}

pub(crate) fn with_hash(s: &str, hash: u64) -> IStr {
    let size = core::mem::size_of::<u64>() + core::mem::size_of::<USIZE>() + s.len() + 1;
    let ptr = alloc(size).cast::<InternedStringHeader>();

    unsafe {
        ptr.write(InternedStringHeader {
            hash,
            len: s.len() as USIZE,
            data: [],
        });

        let ptr = core::ptr::addr_of_mut!((*ptr).data).cast::<u8>();

        ptr.copy_from_nonoverlapping(s.as_ptr(), s.len());

        ptr.add(s.len()).write(0);
    }

    IStr(unsafe { NonNull::new_unchecked(ptr) })
}

impl IStr {
    pub fn to_str(self) -> &'static str {
        let ptr = self.0.as_ptr();
        let ptr = unsafe { core::ptr::addr_of!((*ptr).data).cast::<u8>() };
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, self.len())) }
    }

    pub fn len(self) -> usize {
        let ptr = self.0.as_ptr();
        unsafe { (*ptr).len as usize }
    }

    pub fn saved_hash(self) -> u64 {
        let ptr = self.0.as_ptr();
        unsafe { (*ptr).hash }
    }
}

impl Deref for IStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.to_str()
    }
}

impl core::fmt::Debug for IStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_str().fmt(f)
    }
}

impl core::fmt::Display for IStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_str().fmt(f)
    }
}

impl core::fmt::Pointer for IStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[test]
fn test_simple_alloc() {
    assert_eq!(new("hello").to_str(), new("hello").to_str())
}

#[test]
fn test_many() {
    for _ in 0..1024 {
        new("hello world");
    }
}

#[test]
fn test_large_string() {
    let large = include_str!("../../fixtures/large_string.txt");
    assert_eq!(new(large).to_str(), new(large).to_str())
}
