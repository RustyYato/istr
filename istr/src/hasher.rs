use std::hash::Hasher;

pub fn hash(value: &[u8]) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(value);
    hasher.finish()
}
