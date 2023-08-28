use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};

use hashbrown::raw;

use crate::{hasher, leaky_alloc, IBytes};

#[repr(align(128))]
struct CacheAlignedTable {
    table: Mutex<raw::RawTable<IBytes>>,
}

#[allow(clippy::declare_interior_mutable_const)]
const CACHE_ALIGN_TABLE_INIT: CacheAlignedTable = CacheAlignedTable {
    table: Mutex::new(raw::RawTable::new()),
};

// choose a decently large prime number to prevent cache collisions
static TABLES: [CacheAlignedTable; 64] = [CACHE_ALIGN_TABLE_INIT; 64];

// Constant for h2 function that grabing the top 7 bits of the hash.
const MIN_HASH_LEN: usize = if core::mem::size_of::<usize>() < core::mem::size_of::<u64>() {
    core::mem::size_of::<usize>()
} else {
    core::mem::size_of::<u64>()
};

fn table_for(hash: u64) -> MutexGuard<'static, raw::RawTable<IBytes>> {
    let index = (hash >> (MIN_HASH_LEN * 4)) % TABLES.len() as u64;

    TABLES[index as usize]
        .table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn with_local_table<O>(f: impl FnOnce(&mut raw::RawTable<IBytes>) -> O) -> O {
    thread_local! {
        static LOCAL_TABLE: RefCell<raw::RawTable<IBytes>> = const { RefCell::new(raw::RawTable::new()) };
    }

    #[cold]
    fn reentrant() -> ! {
        panic!("Invalied reentrant call to with_local_table detected")
    }

    #[allow(clippy::explicit_auto_deref)]
    LOCAL_TABLE.with(|table| f(&mut *table.try_borrow_mut().unwrap_or_else(|_| reentrant())))
}

pub fn clear_local_cache() {
    with_local_table(|table| *table = raw::RawTable::new())
}

pub fn local_cache_size() -> usize {
    with_local_table(|table| table.len())
}

pub fn tables() -> impl Iterator<Item = MutexGuard<'static, raw::RawTable<IBytes>>> {
    TABLES
        .iter()
        .map(|table| table.table.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn size() -> usize {
    tables().map(|table| table.len()).sum()
}

#[cfg(ISTR_GLOBAL_CACHE_CLEAR)]
// NOTE: This does not deallocate any existing string, however all new strings will
// never be equal to any existing string so comparing them is useless. This should only
// be used when running tests
// NOTE: This will not clear the thread-local cache, so you may still get existing strings
// if that cache is used.
pub fn clear_global_cache() {
    tables().for_each(|mut table| table.clear())
}

fn insert(table: &mut raw::RawTable<IBytes>, ibytes: IBytes, hash: u64) {
    table.insert(hash, ibytes, |ibytes| ibytes.saved_hash());
}

#[cold]
#[inline(never)]
fn create(table: &mut raw::RawTable<IBytes>, s: &[u8], hash: u64) -> IBytes {
    let ibytes = leaky_alloc::with_hash_bytes(s, hash);
    insert(table, ibytes, hash);
    ibytes
}

fn new_imp(s: &[u8], hash: u64) -> IBytes {
    let table = &mut *table_for(hash);

    let ibytes = if let Some(ibytes) = table.get(hash, |ibytes| ibytes.to_bytes() == s).copied() {
        ibytes
    } else {
        create(table, s, hash)
    };

    ibytes
}

#[cold]
fn new_imp_slow(s: &[u8], hash: u64, local_table: &mut raw::RawTable<IBytes>) -> IBytes {
    let ibytes = new_imp(s, hash);
    insert(local_table, ibytes, hash);
    ibytes
}

fn get_imp(s: &[u8], hash: u64) -> Option<IBytes> {
    let table = &mut *table_for(hash);

    table.get(hash, |ibytes| ibytes.to_bytes() == s).copied()
}

#[cold]
#[inline(never)]
fn get_imp_slow(s: &[u8], hash: u64, local_table: &mut raw::RawTable<IBytes>) -> Option<IBytes> {
    let table = &mut *table_for(hash);
    let ibytes = table.get(hash, |ibytes| ibytes.to_bytes() == s).copied()?;
    insert(local_table, ibytes, hash);
    Some(ibytes)
}

pub fn new_skip_local(s: &[u8]) -> IBytes {
    let hash = hasher::hash(s);
    new_imp(s, hash)
}

pub fn new(s: &[u8]) -> IBytes {
    let hash = hasher::hash(s);

    with_local_table(|local_table| {
        let ibytes = local_table
            .get(hash, |ibytes| ibytes.to_bytes() == s)
            .copied();

        if let Some(ibytes) = ibytes {
            return ibytes;
        }

        new_imp_slow(s, hash, local_table)
    })
}

pub fn get_skip_local(s: &[u8]) -> Option<IBytes> {
    let hash = hasher::hash(s);
    get_imp(s, hash)
}

pub fn get(s: &[u8]) -> Option<IBytes> {
    let hash = hasher::hash(s);

    with_local_table(|local_table| {
        let ibytes = local_table
            .get(hash, |ibytes| ibytes.to_bytes() == s)
            .copied();

        if let Some(ibytes) = ibytes {
            return Some(ibytes);
        }

        get_imp_slow(s, hash, local_table)
    })
}

#[test]
fn test_simple() {
    assert_eq!(new(b"hello"), new(b"hello"))
}

#[test]
fn test_many() {
    for _ in 0..1024 {
        new(b"hello world");
    }
}

#[test]
fn test_large_string() {
    let large = include_bytes!("../../fixtures/large_string.txt");
    assert_eq!(new(large), new(large))
}
