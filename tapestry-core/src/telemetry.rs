//! Telemetry correlation hierarchy and timing-event names (directive §30).
//!
//! Only names and correlation ids live here. Emission, sampling and export
//! belong to the observability swarm.

use serde::{Deserialize, Serialize};

use crate::ids::{OperationId, SessionId};

/// Correlation hierarchy: programme > session > drive > task > operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correlation {
    pub programme: String,
    pub session: SessionId,
    /// Playbook drive identifier, when the session is playbook-governed.
    pub drive: Option<String>,
    pub task: Option<String>,
    pub operation: OperationId,
    /// artefact / transfer / review correlation, when applicable.
    pub artifact: Option<crate::ids::ArtifactId>,
}

/// The timed spans every task SHALL emit, in causal order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingEvent {
    PromptAdmission,
    PlaybookDecision,
    ProviderQueue,
    Inference,
    MesutQueue,
    MesutDispatch,
    LocalExecution,
    Transfer,
    Decode,
    FirstFrame,
    HumanReview,
    ReturnCallback,
}

impl TimingEvent {
    /// All events, in expected causal order (for schema validation of traces).
    pub const ALL: &'static [TimingEvent] = &[
        TimingEvent::PromptAdmission,
        TimingEvent::PlaybookDecision,
        TimingEvent::ProviderQueue,
        TimingEvent::Inference,
        TimingEvent::MesutQueue,
        TimingEvent::MesutDispatch,
        TimingEvent::LocalExecution,
        TimingEvent::Transfer,
        TimingEvent::Decode,
        TimingEvent::FirstFrame,
        TimingEvent::HumanReview,
        TimingEvent::ReturnCallback,
    ];
}

/// One measured span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub event: TimingEvent,
    /// Milliseconds since correlation start.
    pub elapsed_ms: u64,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_roundtrip() {
        let c = Correlation {
            programme: "tapestry".into(),
            session: SessionId::generate(),
            drive: Some("drive-1".into()),
            task: Some("task-1".into()),
            operation: OperationId::generate(),
            artifact: None,
        };
        let back: Correlation = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn timing_events_roundtrip_by_name() {
        for event in TimingEvent::ALL {
            let json = serde_json::to_string(event).unwrap();
            let back: TimingEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, &back);
        }
    }
}
