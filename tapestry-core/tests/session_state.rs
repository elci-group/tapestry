//! Session state machine and review decision semantics.

use chrono::Utc;
use tapestry_core::ids::{ArtifactId, DeviceId, ReviewId, SessionId};
use tapestry_core::review::{ReviewDecision, ReviewRecord};
use tapestry_core::session::{Gate, Session, SessionEvent, SessionEventKind, SessionState};
use tapestry_core::TapestryError;

fn drive_to(session: &mut Session, target: SessionState) {
    // Walk the legal path from Accepted towards a displayable state.
    let path = [
        SessionState::Planning,
        SessionState::Generating,
        SessionState::ArtefactReady,
        SessionState::Transferring,
        SessionState::Displaying,
        SessionState::AwaitingReview,
        SessionState::ReviewComplete,
        SessionState::Complete,
    ];
    for next in path {
        if session.state == target {
            return;
        }
        if session.state.can_transition_to(next) {
            session.transition_to(next, Utc::now()).unwrap();
        }
        if session.state == target {
            return;
        }
    }
    assert_eq!(session.state, target, "could not reach {target:?}");
}

#[test]
fn full_review_loop_transitions() {
    let mut s = Session::new("Generate four visual concepts");
    drive_to(&mut s, SessionState::Complete);
    assert_eq!(s.last_seq, 8);
}

#[test]
fn shortcuts_are_illegal() {
    let mut s = Session::new("prompt");
    assert!(matches!(
        s.transition_to(SessionState::Displaying, Utc::now()),
        Err(TapestryError::IllegalTransition(_))
    ));
    // And from a mid-state you cannot skip the review gate.
    drive_to(&mut s, SessionState::Displaying);
    assert!(!s.state.can_transition_to(SessionState::Complete));
}

#[test]
fn regeneration_returns_to_generating_after_review() {
    let mut s = Session::new("make it darker");
    drive_to(&mut s, SessionState::ReviewComplete);
    s.transition_to(SessionState::Generating, Utc::now())
        .unwrap();
    assert_eq!(s.state, SessionState::Generating);
}

#[test]
fn terminal_states_have_no_successors() {
    for terminal in [
        SessionState::Complete,
        SessionState::Failed,
        SessionState::Cancelled,
    ] {
        assert!(terminal.is_terminal());
        assert!(terminal.successors().is_empty());
    }
}

#[test]
fn every_non_terminal_state_can_fail_or_cancel() {
    for state in [
        SessionState::Accepted,
        SessionState::Planning,
        SessionState::Generating,
        SessionState::ArtefactReady,
        SessionState::Transferring,
        SessionState::Displaying,
        SessionState::AwaitingReview,
        SessionState::ReviewComplete,
    ] {
        assert!(
            state.can_transition_to(SessionState::Failed),
            "{state:?} cannot fail"
        );
        assert!(
            state.can_transition_to(SessionState::Cancelled),
            "{state:?} cannot cancel"
        );
        assert!(!state.is_terminal());
    }
}

#[test]
fn device_binding_via_event() {
    let mut s = Session::new("prompt");
    let device = DeviceId::generate();
    let event = SessionEvent {
        seq: 1,
        session_id: s.id.clone(),
        at: Utc::now(),
        kind: SessionEventKind::DeviceBound {
            device: device.clone(),
        },
    };
    s.apply(&event).unwrap();
    assert_eq!(s.device, Some(device));
}

#[test]
fn review_event_idempotent_by_seq() {
    let mut s = Session::new("prompt");
    let event = SessionEvent {
        seq: 1,
        session_id: s.id.clone(),
        at: Utc::now(),
        kind: SessionEventKind::ReviewReceived {
            review: ReviewId::generate(),
        },
    };
    assert!(s.apply(&event).unwrap());
    // Network retry delivers the same event again: no duplicate effect.
    assert!(!s.apply(&event).unwrap());
    assert_eq!(s.last_seq, 1);
}

#[test]
fn review_records_reject_select_without_artifact() {
    let r = ReviewRecord::new(SessionId::generate(), ReviewDecision::Select, "human");
    assert!(r.validate().is_err());
}

#[test]
fn approve_record_validates_and_resolves_gate() {
    let mut r = ReviewRecord::new(SessionId::generate(), ReviewDecision::Approve, "human");
    r.artifact = Some(ArtifactId::generate());
    r.validate().unwrap();
    assert!(r.decision.is_blocking_resolution());
    // The gate sequence includes a blocking human review gate.
    let gates = [
        Gate::RequestAccepted,
        Gate::PlanReady,
        Gate::GenerationStarted,
        Gate::ArtefactAvailable,
        Gate::ArtefactValidated,
        Gate::DisplayReady,
        Gate::HumanReviewRequired,
        Gate::HumanDecisionReceived,
        Gate::TaskComplete,
    ];
    assert!(gates.contains(&Gate::HumanReviewRequired));
}
