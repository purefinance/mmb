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

/// This macros is needed to create function that will initialize a mock object and a locker for $ type
/// because mockall doesn't support multithreading.
/// Example:
/// ```
/// struct Example {}
/// #[cfg_attr(test, automock)]
/// impl Example {
///     fn foo(&self) -> String {
///         "test".into()
///     }
/// }
///
/// mmb_lib::impl_mock_initializer!(MockExample);
///
/// #[cfg(test)]
/// mod test {
///     use mockall_double::double;
///
///     #[double]
///     use super::Example;
///
///     #[test]
///     fn test_example() {
///         let (mut example_mock, _example_locker) = Example::init_mock();
///         // here you can use example_mock while example_locker is alive
///
///         example_mock.expect_foo().returning(|| "hello".into());
///
///         assert_eq!(example_mock.foo(), "hello");
///     }
/// }
/// ```
///
#[macro_export]
macro_rules! impl_mock_initializer {
    ($type: ident) => {
        paste::paste! {
            /// Needs for syncing mock objects https://docs.rs/mockall/0.10.2/mockall/#static-methods
            #[cfg(test)]
            static [<$type:snake:upper _LOCKER>]: once_cell::sync::Lazy<Mutex<()>> = once_cell::sync::Lazy::new(Mutex::default);
        }

        #[cfg(test)]
        impl $type {
            pub fn init_mock() -> ($type, MutexGuard<'static, ()>) {
                let locker = paste::paste! { [<$type:snake:upper _LOCKER>] }.lock();
                ($type::default(), locker)
            }
        }
    };
}
