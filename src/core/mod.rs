use chrono::Utc;

pub mod exchanges;
pub mod settings;
pub mod connectivity;
pub mod logger;
pub mod orders;


pub type DateTime = chrono::DateTime<Utc>;
