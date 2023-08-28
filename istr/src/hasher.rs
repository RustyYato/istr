use std::hash::{BuildHasher, Hasher};

pub fn hash(value: &[u8]) -> u64 {
    let mut hasher = ahash::RandomState::with_seeds(
        3609252661711376574,
        17522957641342131531,
        18364184400384450343,
        5674598519608203581,
    )
    .build_hasher();
    hasher.write(value);
    hasher.finish()
}

pub const EMPTY_HASH: u64 = 180362161520211164;

#[test]
fn test() {
    assert_eq!(hash(b""), EMPTY_HASH)
}
