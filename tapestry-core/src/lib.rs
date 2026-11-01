//! # tapestry-core
//!
//! Shared contracts for the Tapestry programme: identity, sessions,
//! artefacts, reviews, telemetry and the TAP/1 protocol schema.
//!
//! This crate is deliberately free of I/O. It defines the data that moves;
//! the moving is someone else's job.
//!
//! Wire compatibility: every public type here is a contract consumed by
//! `tapestry-desktop` and `tapestry-android`. Changes require a versioned,
//! compatibility-checked schema change — no consumer redefines these types.

pub mod artifact;
pub mod capability;
pub mod identity;
pub mod ids;
pub mod protocol;
pub mod review;
pub mod session;
pub mod telemetry;

/// Programme-wide error type for contract-level validation failures.
#[derive(Debug, thiserror::Error)]
pub enum TapestryError {
    /// A required field was empty or absent.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// A value violated a structural invariant (bad id, bad digest, ...).
    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },

    /// A state transition or protocol step was attempted out of order.
    #[error("illegal transition: {0}")]
    IllegalTransition(String),

    /// Serialisation or deserialisation of a contract failed.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, TapestryError>;

/// The TAP protocol version this crate speaks.
pub const TAP_VERSION: &str = "TAP/1";
