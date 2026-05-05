//! MemFuse Core — Types, traits, and error handling.
//!
//! This crate provides the foundational building blocks for the MemFuse
//! embedded hybrid-search library.

#![forbid(unsafe_code)]

pub mod error;
pub mod snapshot;
pub mod traits;
pub mod tx_buffer;
pub mod types;

pub use error::{MemFuseError, Result};
pub use snapshot::{SnapshotGuard, SnapshotRegistry};
pub use traits::*;
pub use tx_buffer::{IndexOp, TxBuffer};
pub use types::*;
