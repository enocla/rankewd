//! Rankewd dataset analysis library.
//!
//! The crate separates CLI orchestration, dataset access, analyses, and shared
//! chart presentation. The source-record schema remains available as
//! [`schema`] for consumers of the original JSON Lines export.

mod analysis;
#[doc(hidden)]
pub mod cli;
pub mod schema;

mod dataset;
mod plot;
