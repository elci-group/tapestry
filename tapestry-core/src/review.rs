//! Human review as first-class records (directive §25).
//!
//! Decisions return to Playbook and are persisted to Padagonia; the types
//! here are the wire contract for both directions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ArtifactId, DeviceId, ReviewId, SessionId};

/// The decision a human reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewDecision {
    Approve,
    Reject,
    Select,
    Compare,
    Annotate,
    RequestVariation,
    Defer,
}

impl ReviewDecision {
    /// Whether this decision closes the review gate.
    pub fn is_blocking_resolution(self) -> bool {
        matches!(
            self,
            ReviewDecision::Approve
                | ReviewDecision::Reject
                | ReviewDecision::Select
                | ReviewDecision::RequestVariation
        )
    }
}

/// Free-text or structured annotation accompanying a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Annotation {
    pub text: String,
    /// Optional machine-readable tag, e.g. "typography_density".
    pub tag: Option<String>,
}

/// A complete review record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecord {
    pub id: ReviewId,
    pub session: SessionId,
    /// The artefact under review; absent for session-level decisions.
    pub artifact: Option<ArtifactId>,
    /// The artefact this was chosen over, for SELECT decisions.
    pub selected_over: Vec<ArtifactId>,
    pub reviewer: String,
    pub device: Option<DeviceId>,
    pub decision: ReviewDecision,
    pub annotation: Option<Annotation>,
    pub timestamp: DateTime<Utc>,
}

impl ReviewRecord {
    pub fn new(session: SessionId, decision: ReviewDecision, reviewer: impl Into<String>) -> Self {
        Self {
            id: ReviewId::generate(),
            session,
            artifact: None,
            selected_over: Vec::new(),
            reviewer: reviewer.into(),
            device: None,
            decision,
            annotation: None,
            timestamp: Utc::now(),
        }
    }

    /// Structural validation: SELECT should name what it was chosen over.
    pub fn validate(&self) -> crate::Result<()> {
        if self.decision == ReviewDecision::Select && self.artifact.is_none() {
            return Err(crate::TapestryError::MissingField("artifact"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_requires_artifact() {
        let mut r = ReviewRecord::new(SessionId::generate(), ReviewDecision::Select, "human");
        assert!(r.validate().is_err());
        r.artifact = Some(ArtifactId::generate());
        r.selected_over = vec![ArtifactId::generate()];
        assert!(r.validate().is_ok());
    }

    #[test]
    fn approve_resolves_the_gate_reject_does_too_defer_does_not() {
        assert!(ReviewDecision::Approve.is_blocking_resolution());
        assert!(ReviewDecision::Reject.is_blocking_resolution());
        assert!(!ReviewDecision::Defer.is_blocking_resolution());
    }

    #[test]
    fn serde_roundtrip() {
        let mut r = ReviewRecord::new(SessionId::generate(), ReviewDecision::Approve, "human");
        r.artifact = Some(ArtifactId::generate());
        r.device = Some(DeviceId::generate());
        r.annotation = Some(Annotation {
            text: "too dense".into(),
            tag: Some("typography_density".into()),
        });
        let back: ReviewRecord = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(r, back);
    }
}
