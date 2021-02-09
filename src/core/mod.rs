use chrono::Utc;

pub mod connectivity;
pub mod exchanges;
pub mod logger;
pub mod orders;

pub mod lifecycle;
pub mod order_book;
pub mod settings;
pub mod text;

pub type DateTime = chrono::DateTime<Utc>;
