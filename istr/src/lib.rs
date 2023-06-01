use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};

use hashbrown::raw;

pub use leaky_alloc::IStr;

impl nohash_hasher::IsEnabled for IStr {}
pub type IStrHasher = nohash_hasher::NoHashHasher<IStr>;
pub type IStrBuildHasher = nohash_hasher::BuildNoHashHasher<IStr>;
pub type IStrMap<V> = std::collections::HashMap<IStr, V, IStrBuildHasher>;
pub type IStrSet = std::collections::HashSet<IStr, IStrBuildHasher>;

mod hasher;
mod leaky_alloc;

#[repr(align(128))]
struct CacheAlignedTable {
    table: Mutex<raw::RawTable<IStr>>,
}
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

fn table_for(hash: u64) -> MutexGuard<'static, raw::RawTable<IStr>> {
    let index = (hash >> (MIN_HASH_LEN * 4)) % TABLES.len() as u64;

    TABLES[index as usize]
        .table
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn with_local_table<O>(f: impl FnOnce(&mut raw::RawTable<IStr>) -> O) -> O {
    thread_local! {
        static LOCAL_TABLE: RefCell<raw::RawTable<IStr>> = const { RefCell::new(raw::RawTable::new()) };
    }

    #[cold]
    fn reentrant() -> ! {
        panic!("Invalied reentrant call to with_local_table detected")
    }

    LOCAL_TABLE.with(|table| f(&mut *table.try_borrow_mut().unwrap_or_else(|_| reentrant())))
}

pub fn clear_local_cache() {
    with_local_table(|table| *table = raw::RawTable::new())
}

pub fn local_cache_size() -> usize {
    with_local_table(|table| table.len())
}

fn tables() -> impl Iterator<Item = MutexGuard<'static, raw::RawTable<IStr>>> {
    TABLES
        .iter()
        .map(|table| table.table.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn size() -> usize {
    tables().map(|table| table.len()).sum()
}

fn new_imp(s: &str, hash: u64) -> IStr {
    let table = &mut *table_for(hash);

    let istr = if let Some(istr) = table.get(hash, |istr| istr.to_str() == s).copied() {
        istr
    } else {
        let istr = leaky_alloc::with_hash(s, hash);

        table.insert(hash, istr, |istr| istr.saved_hash());

        istr
    };

    istr
}

#[cold]
#[inline(never)]
fn new_imp_slow(s: &str, hash: u64, local_table: &mut raw::RawTable<IStr>) -> IStr {
    let istr = new_imp(s, hash);

    local_table.insert(hash, istr, |istr| istr.saved_hash());

    istr
}

fn get_imp(s: &str, hash: u64) -> Option<IStr> {
    let table = &mut *table_for(hash);

    table.get(hash, |istr| istr.to_str() == s).copied()
}

#[cold]
#[inline(never)]
fn get_imp_slow(s: &str, hash: u64, local_table: &mut raw::RawTable<IStr>) -> Option<IStr> {
    let table = &mut *table_for(hash);

    let istr = table.get(hash, |istr| istr.to_str() == s).copied()?;

    local_table.insert(hash, istr, |istr| istr.saved_hash());

    Some(istr)
}

pub fn new_skip_local(s: &str) -> IStr {
    let hash = hasher::hash(s);
    new_imp(s, hash)
}

pub fn new(s: &str) -> IStr {
    let hash = hasher::hash(s);

    with_local_table(|local_table| {
        let istr = local_table.get(hash, |istr| istr.to_str() == s).copied();

        if let Some(istr) = istr {
            return istr;
        }

        new_imp_slow(s, hash, local_table)
    })
}

pub fn get_skip_local(s: &str) -> Option<IStr> {
    let hash = hasher::hash(s);
    get_imp(s, hash)
}

pub fn get(s: &str) -> Option<IStr> {
    let hash = hasher::hash(s);

    with_local_table(|local_table| {
        let istr = local_table.get(hash, |istr| istr.to_str() == s).copied();

        if let Some(istr) = istr {
            return Some(istr);
        }

        get_imp_slow(s, hash, local_table)
    })
}

#[test]
fn test_simple() {
    assert_eq!(new("hello"), new("hello"))
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
    assert_eq!(new(large), new(large))
}
