use std::{
    alloc::Layout, cell::Cell, ffi::CStr, hash::Hash, mem::MaybeUninit, ops::Deref, ptr::NonNull,
    str::Utf8Error,
};

// start of with a megabyte of storage, this should usualy be all that's needed
// for the entire program, and usually there shouldn't be any strings larger than
// a megabyte
const INITIAL_SIZE: usize = 1024 * 1024;

const ALIGN: usize = std::mem::align_of::<InternedStringHeader>();
const ALIGN_MASK: usize = !ALIGN.wrapping_sub(1);

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

        if !FIRST {
            ALLOC.with(|alloc| alloc.set(LeakyAllocHandle(ptr)));
        }

        ptr
    }
}

#[cold]
#[inline(never)]
fn large_alloc(size: usize) -> *mut u8 {
    // super large string, just give it a dedicated allocation

    let layout = Layout::from_size_align(size, ALIGN).unwrap();

    let ptr = unsafe { std::alloc::alloc(layout) };

    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout)
    }

    #[cfg(miri)]
    register_leaked(ptr.cast());

    ptr
}

fn alloc(size: usize) -> *mut u8 {
    let mut ptr = get_alloc();

    let mut start = unsafe { core::ptr::addr_of!((*ptr).data).cast::<u8>() };
    let mut header = unsafe { &mut *ptr };

    let remaining = unsafe { header.ptr.offset_from(start) as usize };

    debug_assert_eq!(remaining % ALIGN, 0);

    if remaining >= size {
        // already enough space
    } else if header.layout.size() >= size / 2 {
        // create a new leaky alloc, since it is guaranteed to be larger than the string

        ptr = LeakyAlloc::new();

        start = unsafe { core::ptr::addr_of!((*ptr).data).cast::<u8>() };
        header = unsafe { &mut *ptr };
    } else {
        // for a very large allocation, just create a new allocation dedicated to the string

        return large_alloc(size);
    }

    // if we have enough space in the current leaky alloc, cut off enough space for the string
    // this operates as a bump allocator where the allocator grows down the address space
    // https://fitzgeraldnick.com/2019/11/01/always-bump-downwards.html

    let current = unsafe { header.ptr.sub(size) };
    #[allow(clippy::transmutes_expressible_as_ptr_casts)]
    let current_addr = unsafe { core::mem::transmute::<_, usize>(current) };
    let addr = current_addr & ALIGN_MASK;
    let current = unsafe { current.sub(current_addr - addr) };
    header.ptr = current;

    debug_assert!(current as *const u8 >= start);
    debug_assert!(current as *const u8 <= unsafe { (ptr as *const u8).add(header.layout.size()) });

    current
}

#[repr(C)]
pub(crate) struct InternedStringHeader {
    hash: u64,
    len: usize,
    data: [u8; 0],
}

#[repr(C)]
pub(crate) struct InternedStringData<const N: usize> {
    hash: u64,
    len: usize,
    data: [u8; N],
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IBytes(NonNull<u8>);

impl Hash for IBytes {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.saved_hash().hash(state)
    }
}

unsafe impl Send for IBytes {}
unsafe impl Sync for IBytes {}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IStr(IBytes);

#[cfg(test)]
pub(crate) fn new(s: &str) -> IStr {
    with_hash(s, crate::hasher::hash(s.as_bytes()))
}

#[cfg(test)]
pub(crate) fn with_hash(s: &str, hash: u64) -> IStr {
    let bytes = with_hash_bytes(s.as_bytes(), hash);
    unsafe { IStr::from_utf8_unchecked(bytes) }
}

pub(crate) fn with_hash_bytes(s: &[u8], hash: u64) -> IBytes {
    if s.is_empty() {
        return IBytes::empty();
    }

    const HEADER_PLUS_NUL_TERM: usize =
        core::mem::size_of::<u64>() + core::mem::size_of::<usize>() + 1;
    let size = HEADER_PLUS_NUL_TERM
        .checked_add(s.len())
        .expect("Overflow while calculating layout");

    let ptr = alloc(size).cast::<InternedStringHeader>();

    unsafe {
        ptr.write(InternedStringHeader {
            hash,
            len: s.len(),
            data: [],
        });

        let ptr = core::ptr::addr_of_mut!((*ptr).data).cast::<u8>();

        ptr.copy_from_nonoverlapping(s.as_ptr(), s.len());

        // add a nul terminator, to ensure that every string is a valid cstr
        ptr.add(s.len()).write(0);

        IBytes(NonNull::new_unchecked(ptr))
    }
}

impl IBytes {
    #[inline]
    pub fn empty() -> Self {
        static EMPTY_BYTES: InternedStringData<1> = InternedStringData {
            hash: crate::hasher::EMPTY_HASH,
            len: 0,
            data: [0],
        };

        let x = core::ptr::addr_of!(EMPTY_BYTES.data[0]);

        IBytes(unsafe { NonNull::new_unchecked(x.cast_mut()) })
    }

    #[inline]
    fn header_ptr(self) -> *mut InternedStringHeader {
        let offset = unsafe {
            let data = MaybeUninit::<InternedStringHeader>::uninit();
            let end = core::ptr::addr_of!((*data.as_ptr()).data).cast::<u8>();
            end.offset_from(data.as_ptr().cast()) as usize
        };

        unsafe { self.0.as_ptr().sub(offset).cast() }
    }

    #[inline]
    pub fn to_bytes(self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.0.as_ptr(), self.len()) }
    }

    #[inline]
    pub fn len(self) -> usize {
        let ptr = self.header_ptr();
        unsafe { (*ptr).len }
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn saved_hash(self) -> u64 {
        let ptr = self.header_ptr();
        unsafe { (*ptr).hash }
    }

    #[inline]
    pub fn as_cstr_ptr(self) -> *const std::ffi::c_char {
        self.0.as_ptr().cast()
    }

    #[inline]
    pub fn as_cstr(self) -> &'static CStr {
        unsafe { CStr::from_ptr(self.as_cstr_ptr()) }
    }
}

impl Default for IBytes {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl Default for IStr {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl IStr {
    #[inline]
    pub fn empty() -> Self {
        unsafe { IStr::from_utf8_unchecked(IBytes::empty()) }
    }

    #[inline]
    pub fn from_utf8(bytes: IBytes) -> Result<Self, Utf8Error> {
        core::str::from_utf8(bytes.to_bytes())?;
        Ok(unsafe { Self::from_utf8_unchecked(bytes) })
    }

    /// # Safety
    ///
    /// The bytes must represent valid utf-8
    #[inline]
    pub unsafe fn from_utf8_unchecked(bytes: IBytes) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn to_str(self) -> &'static str {
        unsafe { core::str::from_utf8_unchecked(self.to_bytes()) }
    }

    #[inline]
    pub fn to_bytes(self) -> &'static [u8] {
        self.0.to_bytes()
    }

    #[inline]
    pub fn to_ibytes(self) -> IBytes {
        self.0
    }

    #[inline]
    pub fn len(self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn saved_hash(self) -> u64 {
        self.0.saved_hash()
    }

    #[inline]
    pub fn as_cstr_ptr(self) -> *const std::ffi::c_char {
        self.0.as_cstr_ptr()
    }

    #[inline]
    pub fn as_cstr(self) -> &'static CStr {
        self.0.as_cstr()
    }
}

impl From<IStr> for IBytes {
    #[inline]
    fn from(value: IStr) -> Self {
        value.0
    }
}

impl Deref for IStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.to_str()
    }
}

impl core::fmt::Debug for IBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_bytes().fmt(f)
    }
}

impl core::fmt::Pointer for IBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
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
