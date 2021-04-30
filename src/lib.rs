#![feature(
    path_try_exists,
    with_options,
    associated_type_defaults,
    map_first_last,
    const_fn,
    debug_non_exhaustive,
    format_args_capture
)]
// TODO: Warn clippy::cargo
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod core;
pub mod ui;

#[cfg(test)]
pub use test_support;
