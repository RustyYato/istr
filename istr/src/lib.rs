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

impl IBytes {
    pub fn new(s: &[u8]) -> Self {
        cache::new(s)
    }

    pub fn new_skip_local(s: &[u8]) -> Self {
        cache::new_skip_local(s)
    }

    pub fn get(s: &[u8]) -> Option<Self> {
        cache::get(s)
    }

    pub fn get_skip_local(s: &[u8]) -> Option<Self> {
        cache::get_skip_local(s)
    }
}

impl IStr {
    pub fn new(s: &str) -> Self {
        unsafe { IStr::from_utf8_unchecked(IBytes::new(s.as_bytes())) }
    }

    pub fn new_skip_local(s: &str) -> Self {
        unsafe { IStr::from_utf8_unchecked(IBytes::new_skip_local(s.as_bytes())) }
    }

    pub fn get(s: &str) -> Option<Self> {
        Some(unsafe { IStr::from_utf8_unchecked(IBytes::get(s.as_bytes())?) })
    }

    pub fn get_skip_local(s: &str) -> Option<Self> {
        Some(unsafe { IStr::from_utf8_unchecked(IBytes::get_skip_local(s.as_bytes())?) })
    }
}
