use chrono::Utc;

pub mod cancellation_token;
pub mod impl_id;
pub mod impl_mocks;
pub mod impl_table_types;
pub mod infrastructure;
pub mod logger;
pub mod time;
pub mod traits_ext;

/// Just for marking explicitly: no action to do here and it is not forgotten execution branch
#[inline(always)]
pub fn nothing_to_do() {}

pub static OPERATION_CANCELED_MSG: &str = "Operation cancelled";

pub type DateTime = chrono::DateTime<Utc>;
