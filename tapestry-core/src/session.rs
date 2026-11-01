//! Session state and transitions.
//!
//! A session is one prompt-driven unit of work carried from shell admission
//! through artefact presentation to human review. State transitions are
//! validated here so no client can invent an illegal path.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, SessionId};
use crate::{Result, TapestryError};

/// Playbook-visible gates (directive §7). Human review is a blocking gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Gate {
    RequestAccepted,
    PlanReady,
    GenerationStarted,
    ArtefactAvailable,
    ArtefactValidated,
    DisplayReady,
    HumanReviewRequired,
    HumanDecisionReceived,
    TaskComplete,
}

/// Lifecycle states of a session, ordered to match the gate sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Accepted,
    Planning,
    Generating,
    ArtefactReady,
    Transferring,
    Displaying,
    AwaitingReview,
    ReviewComplete,
    Complete,
    Failed,
    Cancelled,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl SessionState {
    /// The legal successor states. `Failed` and `Cancelled` are reachable
    /// from every non-terminal state.
    pub fn successors(self) -> &'static [SessionState] {
        use SessionState::*;
        match self {
            Accepted => &[Planning, Failed, Cancelled],
            Planning => &[Generating, Failed, Cancelled],
            Generating => &[ArtefactReady, Failed, Cancelled],
            ArtefactReady => &[Transferring, Displaying, Failed, Cancelled],
            Transferring => &[Displaying, Failed, Cancelled],
            Displaying => &[AwaitingReview, Failed, Cancelled],
            AwaitingReview => &[ReviewComplete, Failed, Cancelled],
            ReviewComplete => &[Complete, Generating, Failed, Cancelled],
            Complete | Failed | Cancelled => &[],
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            SessionState::Complete | SessionState::Failed | SessionState::Cancelled
        )
    }

    pub fn can_transition_to(self, next: SessionState) -> bool {
        self.successors().contains(&next)
    }
}

/// An append-only session event; replaying events reconstructs state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEvent {
    /// Monotonic per-session sequence; idempotency key for replay.
    pub seq: u64,
    pub session_id: SessionId,
    pub at: DateTime<Utc>,
    pub kind: SessionEventKind,
}

/// The payload of a session event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionEventKind {
    /// The session was accepted for processing.
    Accepted { prompt: String },
    /// A gate was reached.
    GateReached { gate: Gate },
    /// State advanced.
    StateChanged {
        from: SessionState,
        to: SessionState,
    },
    /// A device was bound to present this session.
    DeviceBound { device: DeviceId },
    /// A human review decision arrived (idempotent by `review` id).
    ReviewReceived { review: crate::ids::ReviewId },
}

/// A session record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: SessionId,
    pub state: SessionState,
    pub prompt: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seq: u64,
    pub device: Option<DeviceId>,
}

impl Session {
    pub fn new(prompt: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::generate(),
            state: SessionState::Accepted,
            prompt: prompt.into(),
            created_at: now,
            updated_at: now,
            last_seq: 0,
            device: None,
        }
    }

    /// Apply an event, validating ordering and transitions.
    ///
    /// Re-applying an already-applied `seq` is a no-op returning `Ok(false)`
    /// — idempotency, per directive §29 — while an out-of-order `seq` is an
    /// error.
    pub fn apply(&mut self, event: &SessionEvent) -> Result<bool> {
        if event.session_id != self.id {
            return Err(TapestryError::IllegalTransition(format!(
                "event for session {} applied to session {}",
                event.session_id, self.id
            )));
        }
        if event.seq <= self.last_seq {
            return Ok(false); // duplicate: already applied
        }
        if event.seq != self.last_seq + 1 {
            return Err(TapestryError::IllegalTransition(format!(
                "event seq {} follows {}",
                event.seq, self.last_seq
            )));
        }

        match &event.kind {
            SessionEventKind::StateChanged { to, .. } => {
                if !self.state.can_transition_to(*to) {
                    return Err(TapestryError::IllegalTransition(format!(
                        "{} -> {}",
                        self.state, to
                    )));
                }
                self.state = *to;
            }
            SessionEventKind::Accepted { .. } => {}
            SessionEventKind::GateReached { .. } => {}
            SessionEventKind::DeviceBound { device } => {
                self.device = Some(device.clone());
            }
            SessionEventKind::ReviewReceived { .. } => {}
        }

        self.last_seq = event.seq;
        self.updated_at = event.at;
        Ok(true)
    }

    /// Advance the session, producing the corresponding event.
    pub fn transition_to(&mut self, next: SessionState, at: DateTime<Utc>) -> Result<SessionEvent> {
        let event = SessionEvent {
            seq: self.last_seq + 1,
            session_id: self.id.clone(),
            at,
            kind: SessionEventKind::StateChanged {
                from: self.state,
                to: next,
            },
        };
        self.apply(&event)?;
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions() {
        let mut s = Session::new("hello");
        for next in [
            SessionState::Planning,
            SessionState::Generating,
            SessionState::ArtefactReady,
            SessionState::Transferring,
            SessionState::Displaying,
            SessionState::AwaitingReview,
            SessionState::ReviewComplete,
            SessionState::Complete,
        ] {
            s.transition_to(next, Utc::now()).unwrap();
        }
        assert!(s.state.is_terminal());
        assert_eq!(s.last_seq, 8);
    }

    #[test]
    fn illegal_transition_rejected() {
        let mut s = Session::new("hello");
        let err = s.transition_to(SessionState::Complete, Utc::now());
        assert!(matches!(err, Err(TapestryError::IllegalTransition(_))));
    }

    #[test]
    fn duplicate_event_is_noop_out_of_order_is_error() {
        let mut s = Session::new("hello");
        let event = s.transition_to(SessionState::Planning, Utc::now()).unwrap();
        assert!(!s.apply(&event).unwrap()); // duplicate seq
        let mut replay = event.clone();
        replay.seq += 5;
        assert!(s.apply(&replay).is_err()); // gap
    }

    #[test]
    fn foreign_session_event_rejected() {
        let mut s = Session::new("hello");
        let event = SessionEvent {
            seq: 1,
            session_id: SessionId::generate(),
            at: Utc::now(),
            kind: SessionEventKind::GateReached {
                gate: Gate::PlanReady,
            },
        };
        assert!(s.apply(&event).is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let mut s = Session::new("hello");
        s.transition_to(SessionState::Planning, Utc::now()).unwrap();
        let back: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
    }
}
