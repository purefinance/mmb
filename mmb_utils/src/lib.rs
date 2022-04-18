#![deny(
    non_ascii_idents,
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
    unused_must_use,
    clippy::unwrap_used
)]

pub mod cancellation_token;
pub mod decimal_inverse_sign;
pub mod impl_id;
pub mod impl_mocks;
pub mod impl_table_types;
pub mod infrastructure;
pub mod logger;
pub mod panic;
pub mod send_expected;
pub mod time;
pub mod value_to_decimal;

use chrono::Utc;

/// Just for marking explicitly: no action to do here and it is not forgotten execution branch
#[inline(always)]
pub fn nothing_to_do() {}

pub static OPERATION_CANCELED_MSG: &str = "Operation cancelled";

pub type DateTime = chrono::DateTime<Utc>;

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
