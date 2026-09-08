//! Score and probability calibration module for MemFuse (Isotonic + Platt Scaling + Adaptive PID Pool-Size Control).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod isotonic;
pub mod pid;
pub mod platt;

#[cfg(feature = "replicator-dynamics-weights")]
pub mod replicator;

pub use isotonic::IsotonicCalibrator;
pub use memfuse_core::ConfigFingerprint;
pub use pid::PidController;
pub use platt::PlattScaler;

#[cfg(feature = "replicator-dynamics-weights")]
pub use replicator::{record_retrieval_feedback, ReplicatorState};
