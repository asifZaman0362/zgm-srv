use ahash::HashMap;
use std::hash::Hash;
pub mod actor;

struct MapPool<K: Hash, V> {
    free: HashMap<K, V>,
    reserved: HashMap<K, V>,
}

impl<K: Hash, V> MapPool<K, V> {
    fn new(capacity: usize) -> Self {
        let free: HashMap<K, V> = crate::utils::new_fast_hashmap(capacity);
        let reserved: HashMap<K, V> = crate::utils::new_fast_hashmap(capacity);
        Self { free, reserved }
    }
    fn get_free(&mut self) -> Option<V> {
        if self.free.len() > 0 {
            self.free.
        }
    }
}

pub struct RoomManager {}
