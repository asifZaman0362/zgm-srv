pub fn new_fast_hashmap<K, V>(cap: usize) -> ahash::HashMap<K, V> {
    ahash::HashMap::with_capacity_and_hasher(cap, ahash::RandomState::default())
}
