use std::hash::{BuildHasher, Hasher};

pub fn hash(value: &[u8]) -> u64 {
    let mut hasher = ahash::RandomState::with_seeds(0, 0, 0, 0).build_hasher();
    hasher.write(value);
    hasher.finish()
}
