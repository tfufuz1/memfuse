//! Core domain, budget, filter, and search query data types.

/// Memory and token resource budget management types.
pub mod budget;
/// Fundamental domain models (`DocId`, `TxId`, `Entity`, `DistanceMetric`, etc.).
pub mod domain;
/// Structured metadata expression filter types.
pub mod filter;
/// Unified 4-signal search query and context types.
pub mod saos;

pub use budget::*;
pub use domain::*;
pub use filter::*;
pub use saos::*;
