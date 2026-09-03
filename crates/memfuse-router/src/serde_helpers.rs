//! Serde helpers for memfuse-router.

pub mod sorted_u64_set {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashSet;

    pub fn serialize<S: Serializer>(set: &HashSet<u64>, s: S) -> Result<S::Ok, S::Error> {
        let mut v: Vec<u64> = set.iter().copied().collect();
        v.sort_unstable();
        v.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<HashSet<u64>, D::Error> {
        Vec::<u64>::deserialize(d).map(|v| v.into_iter().collect())
    }
}
