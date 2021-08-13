use std::collections::HashMap;
use std::hash::Hash;

pub(crate) fn make_hash_map<K, V>(k: K, v: V) -> HashMap<K, V>
where
    K: Eq + Hash,
{
    Some((k, v)).into_iter().collect()
}
