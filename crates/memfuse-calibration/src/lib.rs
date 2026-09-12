//! Score and probability calibration module for MemFuse (Isotonic + Platt Scaling + Adaptive PID Pool-Size Control).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod isotonic;
pub mod pid;
pub mod platt;

pub use isotonic::IsotonicCalibrator;
pub use memfuse_core::ConfigFingerprint;
pub use pid::PidController;
pub use platt::PlattScaler;
