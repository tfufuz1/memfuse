pub mod dispatch;
pub mod outcome;
pub mod profile;
pub mod router;
pub mod serde_helpers;

#[cfg(test)]
mod tests;

pub use dispatch::dispatch_to_slm;
pub use outcome::{DecisionId, RoutingOutcome};
pub use profile::SlmProfile;
pub use router::{RouterEngine, RoutingDecision};
