use rand::Rng;
use std::mem::size_of;
use std::{collections::HashMap, fmt};
use xxhash_rust::xxh3::xxh3_64_with_seed;

#[derive(Eq, Hash, PartialEq, Clone, Copy, Ord, PartialOrd, Debug)]
pub struct VertexHash(u64);

impl VertexHash {
    pub fn new(x: u64) -> VertexHash {
        VertexHash(x)
    }
}

#[derive(Eq, PartialEq, Clone, Copy, Ord, PartialOrd, Debug)]
pub struct VertexIndex(u64);

impl VertexIndex {
    pub fn new(x: u64) -> VertexIndex {
        VertexIndex(x)
    }
    pub fn to_u64(self) -> u64 {
        self.0
    }
    pub fn to_usize(self) -> usize {
        self.0 as usize
    }
    pub fn mark_collision(&mut self) {
        *self = VertexIndex(self.0 | MSB64);
    }
    pub fn has_collision(&self) -> bool {
        self.0 & MSB64 == MSB64
    }
    pub fn pure(&self) -> VertexIndex {
        // without collision mark
        VertexIndex(self.0 & !MSB64)
    }
}

fn xxh3_hash(key: &[u8]) -> VertexHash {
    VertexHash::new(xxh3_64_with_seed(key, 0xdeadbeefdeadbeef))
}
const MSB64: u64 = 1u64 << 63;

#[derive(PartialEq)]
pub struct VertexKeyIndex {
    hasher: fn(&[u8]) -> VertexHash,

    index_to_hash: Vec<VertexHash>,

    // high bit indicates a collision
    hash_to_index: HashMap<VertexHash, VertexIndex>,

    exceptions: HashMap<Vec<u8>, VertexHash>,
}
impl fmt::Debug for VertexKeyIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VertexKeyIndex")
            .field("index_to_hash", &self.index_to_hash)
            .field("hash_to_index", &self.hash_to_index)
            .field("exceptions", &self.exceptions)
            .finish()
    }
}

impl Default for VertexKeyIndex {
    fn default() -> Self {
        Self {
            hasher: xxh3_hash,
            index_to_hash: vec![],
            hash_to_index: HashMap::new(),
            exceptions: HashMap::new(),
        }
    }
}
impl VertexKeyIndex {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from(
        index_to_hash: Vec<VertexHash>,
        hash_to_index: HashMap<VertexHash, VertexIndex>,
        exceptions: HashMap<Vec<u8>, VertexHash>,
    ) -> Self {
        Self {
            hasher: xxh3_hash,
            index_to_hash,
            hash_to_index,
            exceptions,
        }
    }

    pub fn add(&mut self, key: &[u8]) -> VertexIndex {
        // First detect a collision:
        let index = VertexIndex(self.index_to_hash.len() as u64);
        let hash = (self.hasher)(key);
        let mut actual = hash;
        if self.hash_to_index.contains_key(&hash) {
            // This is a collision, we create a random alternative
            // hash and report the collision:
            let mut rng = rand::thread_rng();
            loop {
                actual = VertexHash(rng.gen::<u64>());
                if !self.hash_to_index.contains_key(&actual) {
                    break;
                }
            }
            let oi = self.hash_to_index.get_mut(&hash).unwrap();
            oi.mark_collision();
            self.exceptions.insert(key.to_vec(), actual);
        }
        self.index_to_hash.push(actual);
        self.hash_to_index.insert(actual, index);
        index
    }

    pub fn count(&self) -> usize {
        self.index_to_hash.len()
    }

    pub fn get(&self, k: &[u8]) -> Option<VertexIndex> {
        let hash = (self.hasher)(k);
        match self.hash_to_index.get(&hash) {
            None => None,
            Some(index) => {
                if index.has_collision() {
                    match self.exceptions.get(k) {
                        Some(h) => self.hash_to_index.get(h).copied(),
                        None => Some(index.pure()),
                    }
                } else {
                    Some(*index)
                }
            }
        }
    }

    pub fn memory_in_bytes(&self) -> usize {
        let size_hash = size_of::<VertexHash>();
        let size_index = size_of::<VertexIndex>();

        self.count()
            * (
            // index_to_hash:
            size_hash
                // hash_to_index:
                + size_hash + size_index
        )
            // Heuristics for the (hopefully few) exceptions:
            + self.exceptions.len() * (48 + size_hash)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn length_hash(key: &[u8]) -> VertexHash {
        VertexHash::new(key.len() as u64)
    }

    #[test]
    fn adds_and_retrieves_hash() {
        let mut index = VertexKeyIndex {
            hasher: length_hash,
            ..Default::default()
        };

        let id = index.add(b"V/A");

        assert_eq!(index.get(b"V/A"), Some(id));
        assert_eq!(
            index,
            VertexKeyIndex {
                index_to_hash: vec![length_hash(b"V/A")],
                hash_to_index: HashMap::from([(length_hash(b"V/A"), VertexIndex::new(0))]),
                hasher: length_hash,
                ..Default::default()
            }
        );
    }

    #[test]
    fn gives_none_for_non_existing_key_retrieval() {
        let index = VertexKeyIndex::new();

        assert_eq!(index.get(b"V/B"), None);
    }

    mod hash_collisions {
        use super::*;

        #[test]
        fn handles_hash_collisions() {
            let mut index = VertexKeyIndex {
                hasher: length_hash,
                ..Default::default()
            };

            let some_index_value = index.add(b"V/A");
            let colliding_index_value = index.add(b"V/B");

            assert_eq!(index.get(b"V/A"), Some(some_index_value));
            assert_eq!(index.get(b"V/B"), Some(colliding_index_value));
            assert_eq!(index.exceptions.len(), 1);
        }

        #[test]
        fn retrieves_only_last_entry_from_collision_with_same_key() {
            let mut index = VertexKeyIndex {
                hasher: length_hash,
                ..Default::default()
            };

            index.add(b"V/A");
            let colliding_index_value = index.add(b"V/A");

            assert_eq!(index.get(b"V/A"), Some(colliding_index_value));
            assert_eq!(index.exceptions.len(), 1);
            assert_eq!(index.index_to_hash.len(), 2);
        }
    }
}
