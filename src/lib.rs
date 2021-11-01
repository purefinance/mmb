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

/// This macro needs to generate an ID for some structures like ClientOrder or ExchangeOrder.
/// All IDs must be unique, here we use AtomicU64 static variable that initialize with current UNIX time(get_atomic_current_secs() function)
/// "0" means "empty id"
/// # Example:
/// ```
/// use std::fmt;
/// use std::fmt::{Display, Formatter};
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// use once_cell::sync::Lazy;
///
/// use mmb_lib::impl_id;
/// use mmb_lib::core::infrastructure::WithExpect;
/// use mmb_lib::core::utils::get_atomic_current_secs;
///
/// struct Example{};
///
/// impl_id!(ExampleId);
/// ```
#[macro_export]
macro_rules! impl_id {
    ($type: ident) => {
        paste::paste! {
            static [<$type:snake:upper _ID>]: Lazy<AtomicU64> = Lazy::new(|| get_atomic_current_secs());
        }

        #[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize, Hash, Ord, PartialOrd)]
        #[serde(from = "&str")]
        pub struct $type(u64);

        impl $type {
            /// Generate unique ID
            pub fn generate() -> Self {
                let new_id = paste::paste! { [<$type:snake:upper _ID>] }.fetch_add(1, Ordering::AcqRel);
                $type(new_id)
            }

            /// Create an empty ID
            pub fn new() -> Self {
                $type(0)
            }

            pub fn is_empty(&self) -> bool {
                self.0 == 0
            }
        }

        impl Display for $type {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }


        impl From<u64> for $type {
            fn from(value: u64) -> Self {
                $type(value)
            }
        }

        impl From<&str> for $type {
            fn from(value: &str) -> Self {
                $type(value.parse::<u64>().with_expect(|| format!("Failed to convert '{}' into u64", value) ))
            }
        }
    }
}

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key, $val); )*
         map
    }}
}
