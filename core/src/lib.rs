#![deny(
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use
)]

#[allow(dead_code)]
pub mod core;
pub mod rest_api;
pub mod strategies;

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

#[macro_export]
macro_rules! dashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let map = dashmap::DashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}

#[cfg(test)]
use parking_lot::ReentrantMutex;

#[cfg(test)]
pub static MOCK_MUTEX: once_cell::sync::Lazy<ReentrantMutex<()>> =
    once_cell::sync::Lazy::new(ReentrantMutex::default);
