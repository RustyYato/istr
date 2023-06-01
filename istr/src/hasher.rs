use std::hash::{Hash, Hasher};

pub fn hash<T: ?Sized + Hash>(value: &T) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}
