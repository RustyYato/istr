pub use leaky_alloc::{IBytes, IStr};

impl nohash_hasher::IsEnabled for IStr {}
pub type IStrHasher = nohash_hasher::NoHashHasher<IStr>;
pub type IStrBuildHasher = nohash_hasher::BuildNoHashHasher<IStr>;
pub type IStrMap<V> = std::collections::HashMap<IStr, V, IStrBuildHasher>;
pub type IStrSet = std::collections::HashSet<IStr, IStrBuildHasher>;

impl nohash_hasher::IsEnabled for IBytes {}
pub type IBytesHasher = nohash_hasher::NoHashHasher<IBytes>;
pub type IBytesBuildHasher = nohash_hasher::BuildNoHashHasher<IBytes>;
pub type IBytesMap<V> = std::collections::HashMap<IBytes, V, IBytesBuildHasher>;
pub type IBytesSet = std::collections::HashSet<IBytes, IBytesBuildHasher>;

mod hasher;
mod leaky_alloc;

mod cache;

pub use cache::{clear_local_cache, local_cache_size, size};

#[cfg(ISTR_GLOBAL_CACHE_CLEAR)]
pub use cache::clear_global_cache;

pub fn items() -> impl Iterator<Item = IBytes> {
    use hashbrown::raw;

    pub struct TableIterator {
        _guard: std::sync::MutexGuard<'static, raw::RawTable<IBytes>>,
        iter: raw::RawIter<IBytes>,
    }

    impl Iterator for TableIterator {
        type Item = IBytes;

        fn next(&mut self) -> Option<Self::Item> {
            self.iter.next().map(|bucket| unsafe { *bucket.as_ref() })
        }
    }

    cache::tables().flat_map(|table| unsafe {
        TableIterator {
            iter: table.iter(),
            _guard: table,
        }
    })
}

impl IBytes {
    #[inline]
    pub fn new(s: &[u8]) -> Self {
        cache::new(s)
    }

    #[inline]
    pub fn new_skip_local(s: &[u8]) -> Self {
        cache::new_skip_local(s)
    }

    #[inline]
    pub fn get(s: &[u8]) -> Option<Self> {
        cache::get(s)
    }

    #[inline]
    pub fn get_skip_local(s: &[u8]) -> Option<Self> {
        cache::get_skip_local(s)
    }
}

impl IStr {
    #[inline]
    pub fn new(s: &str) -> Self {
        unsafe { IStr::from_utf8_unchecked(IBytes::new(s.as_bytes())) }
    }

    #[inline]
    pub fn new_skip_local(s: &str) -> Self {
        unsafe { IStr::from_utf8_unchecked(IBytes::new_skip_local(s.as_bytes())) }
    }

    #[inline]
    pub fn get(s: &str) -> Option<Self> {
        Some(unsafe { IStr::from_utf8_unchecked(IBytes::get(s.as_bytes())?) })
    }

    #[inline]
    pub fn get_skip_local(s: &str) -> Option<Self> {
        Some(unsafe { IStr::from_utf8_unchecked(IBytes::get_skip_local(s.as_bytes())?) })
    }
}
