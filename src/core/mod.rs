use chrono::Utc;

pub mod connectivity;
pub mod exchanges;
pub mod logger;
pub mod orders;

pub mod order_book;
pub mod settings;

pub type DateTime = chrono::DateTime<Utc>;
