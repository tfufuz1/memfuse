//! Score and probability calibration module for MemFuse (Isotonic + Platt Scaling).

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod isotonic;
pub mod platt;

pub use isotonic::IsotonicCalibrator;
pub use memfuse_core::ConfigFingerprint;
pub use platt::PlattScaler;
