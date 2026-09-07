//! Score and probability calibration module for MemFuse (Isotonic + Platt Scaling).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod isotonic;
pub mod platt;

#[cfg(feature = "physio-replicator-weights")]
pub mod replicator;

pub use isotonic::IsotonicCalibrator;
pub use memfuse_core::ConfigFingerprint;
pub use platt::PlattScaler;

#[cfg(feature = "physio-replicator-weights")]
pub use replicator::{record_retrieval_feedback, ReplicatorState};
