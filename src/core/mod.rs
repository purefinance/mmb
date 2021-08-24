use chrono::Utc;

mod balance_manager;
mod balances;
pub mod connectivity;
pub mod exchanges;
pub mod infrastructure;
pub mod logger;
pub mod misc;
pub mod orders;
pub mod service_configuration;
pub mod statistic_service;
pub mod utils;

pub mod config;
pub mod disposition_execution;
pub(crate) mod events;
pub mod explanation;
pub(crate) mod internal_events_loop;
pub mod lifecycle;
pub mod math;
pub mod order_book;
pub(crate) mod services;
pub mod settings;
pub mod text;

pub type DateTime = chrono::DateTime<Utc>;

/// Just for marking explicitly: no action to do here and it is not forgotten execution branch
#[inline(always)]
pub fn nothing_to_do() {}
