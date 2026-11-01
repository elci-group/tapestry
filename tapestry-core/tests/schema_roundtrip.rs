//! Every public contract type round-trips through serde JSON without loss,
//! and wire contracts reject unknown fields.

use tapestry_core::artifact::{Artifact, ArtifactKind};
use tapestry_core::capability::{Codec, CodecCapabilities};
use tapestry_core::identity::{
    AudioCharacteristics, CapabilitySet, DeviceIdentity, DisplayCharacteristics,
    InputCharacteristic, StorageCharacteristic, TapVersion,
};
use tapestry_core::ids::{ArtifactId, DeviceId, OperationId, ReviewId, SessionId};
use tapestry_core::review::{Annotation, ReviewDecision, ReviewRecord};
use tapestry_core::session::{Gate, Session, SessionEvent, SessionEventKind, SessionState};
use tapestry_core::telemetry::{Correlation, Timing, TimingEvent};

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) -> T {
    let json = serde_json::to_string(value).expect("serialise");
    serde_json::from_str(&json).expect("deserialise")
}

fn sample_device() -> DeviceIdentity {
    DeviceIdentity {
        device_id: DeviceId::generate(),
        friendly_name: "lounge-fire".into(),
        platform: "fire-tv".into(),
        tap_version: TapVersion::default(),
        capabilities: CapabilitySet {
            display: Some(DisplayCharacteristics {
                width: 3840,
                height: 2160,
                refresh_hz: 60,
                hdr: false,
            }),
            audio: Some(AudioCharacteristics {
                channels: 2,
                surround: false,
            }),
            input: vec![InputCharacteristic::Dpad],
            storage: Some(StorageCharacteristic {
                capacity_bytes: 8 << 30,
                available_bytes: 4 << 30,
                writable: true,
                removable: true,
            }),
            codecs: vec![Codec::H265],
        },
        available: true,
    }
}

fn sample_artifact() -> Artifact {
    Artifact::builder(SessionId::generate(), ArtifactKind::Video)
        .mime("video/mp4")
        .content(b"video-bytes")
        .duration_ms(90_000)
        .codec(Codec::H265)
        .comparison_group("animated-hero")
        .build()
}

#[test]
fn ids_roundtrip() {
    for id in [
        serde_json::to_value(SessionId::generate()).unwrap(),
        serde_json::to_value(ArtifactId::generate()).unwrap(),
        serde_json::to_value(DeviceId::generate()).unwrap(),
        serde_json::to_value(ReviewId::generate()).unwrap(),
        serde_json::to_value(OperationId::generate()).unwrap(),
    ] {
        let json = id.to_string();
        assert!(!json.is_empty());
    }
}

#[test]
fn device_identity_roundtrip() {
    let dev = sample_device();
    assert_eq!(roundtrip(&dev), dev);
}

#[test]
fn artifact_roundtrip() {
    let art = sample_artifact();
    assert_eq!(roundtrip(&art), art);
}

#[test]
fn review_record_roundtrip() {
    let mut r = ReviewRecord::new(SessionId::generate(), ReviewDecision::Select, "human");
    r.artifact = Some(ArtifactId::generate());
    r.selected_over = vec![ArtifactId::generate(), ArtifactId::generate()];
    r.annotation = Some(Annotation {
        text: "prefer the darker variant".into(),
        tag: Some("brightness".into()),
    });
    assert_eq!(roundtrip(&r), r);
}

#[test]
fn session_and_events_roundtrip() {
    let mut s = Session::new("Generate four visual concepts");
    let event = SessionEvent {
        seq: 1,
        session_id: s.id.clone(),
        at: chrono::Utc::now(),
        kind: SessionEventKind::GateReached {
            gate: Gate::RequestAccepted,
        },
    };
    s.apply(&event).unwrap();
    assert_eq!(roundtrip(&s), s);
    assert_eq!(roundtrip(&event), event);
}

#[test]
fn session_state_and_gate_names_are_stable() {
    // Wire names must not drift between releases.
    assert_eq!(
        serde_json::to_string(&SessionState::AwaitingReview).unwrap(),
        "\"awaiting_review\""
    );
    assert_eq!(
        serde_json::to_string(&Gate::HumanReviewRequired).unwrap(),
        "\"HUMAN_REVIEW_REQUIRED\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewDecision::RequestVariation).unwrap(),
        "\"REQUEST_VARIATION\""
    );
    assert_eq!(
        serde_json::to_string(&ArtifactKind::ImageSet).unwrap(),
        "\"IMAGE_SET\""
    );
}

#[test]
fn telemetry_roundtrip() {
    let c = Correlation {
        programme: "tapestry".into(),
        session: SessionId::generate(),
        drive: Some("drive-mvp".into()),
        task: Some("task-1".into()),
        operation: OperationId::generate(),
        artifact: Some(ArtifactId::generate()),
    };
    let t = Timing {
        event: TimingEvent::FirstFrame,
        elapsed_ms: 18_400,
        duration_ms: 42,
    };
    assert_eq!(roundtrip(&c), c);
    assert_eq!(roundtrip(&t), t);
}

#[test]
fn codec_capabilities_roundtrip() {
    let caps = CodecCapabilities::from_probe(vec![Codec::H264]);
    assert_eq!(roundtrip(&caps), caps);
}

#[test]
#[should_panic(expected = "unknown variant")]
fn future_session_state_is_rejected_not_silently_accepted() {
    let json = r#""warp_speed""#;
    let _parsed: SessionState = serde_json::from_str(json).unwrap();
}
