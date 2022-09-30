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

mod channels;
mod contract;
mod event_listener_fields;
mod exchange_client;
mod exchange_client_builder;
mod handlers;
mod interactive_brokers;
mod mutexes;
mod order_side;
mod order_status;
mod support;
