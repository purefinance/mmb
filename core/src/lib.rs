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
    unused_must_use,
    clippy::unwrap_used
)]

pub(crate) mod balance_changes;
pub mod balance_manager;
mod balances;
pub mod connectivity;
pub mod exchanges;
pub mod infrastructure;
pub mod misc;
pub mod orders;
pub mod rpc;
pub mod service_configuration;
pub mod statistic_service;
pub mod strategies;

pub mod config;
pub mod disposition_execution;
pub mod explanation;
pub mod lifecycle;
pub mod math;
pub mod order_book;
pub(crate) mod services;
pub mod settings;
pub mod text;

#[cfg(test)]
use parking_lot::ReentrantMutex;

#[cfg(test)]
pub static MOCK_MUTEX: once_cell::sync::Lazy<ReentrantMutex<()>> =
    once_cell::sync::Lazy::new(ReentrantMutex::default);
