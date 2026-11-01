//! TAP frame encode → decode → encode byte stability, chunk resume
//! behaviour, and validation guards.

use tapestry_core::capability::NegotiationMessage;
use tapestry_core::ids::{ArtifactId, DeviceId, SessionId};
use tapestry_core::protocol::chunk::{ChunkAck, TransferState};
use tapestry_core::protocol::frame::{Frame, MAX_CHUNK_LEN, MAX_FRAME_PAYLOAD};
use tapestry_core::protocol::negotiate::Handshake;
use tapestry_core::protocol::Channel;
use tapestry_core::{TapestryError, TAP_VERSION};

fn chunk_frame(seq: u64, offset: u64, payload: Vec<u8>) -> Frame {
    let mut f = Frame::new(
        SessionId::generate(),
        Channel::Artefact,
        "chunk",
        seq,
        DeviceId::generate(),
        payload,
    );
    f.artifact = Some(ArtifactId::generate());
    f.offset = Some(offset);
    f
}

#[test]
fn encode_decode_encode_is_byte_identical() {
    let f = chunk_frame(441, 16_777_216, vec![0xABu8; 4096]);
    let first = f.to_json().unwrap();
    let decoded = Frame::from_json(&first).unwrap();
    let second = decoded.to_json().unwrap();
    assert_eq!(first, second);
}

#[test]
fn valid_chunks_validate() {
    let f = chunk_frame(1, 0, vec![0u8; 1024]);
    assert!(f.validate().is_ok());
}

#[test]
fn corrupted_payload_fails_validation() {
    let mut f = chunk_frame(1, 0, vec![1, 2, 3]);
    f.payload[0] ^= 0xff;
    match f.validate() {
        Err(TapestryError::InvalidValue {
            field: "digest", ..
        }) => {}
        other => panic!("expected digest error, got {other:?}"),
    }
}

#[test]
fn declared_length_must_match_payload() {
    let mut f = chunk_frame(1, 0, vec![1, 2, 3]);
    f.len = 999;
    assert!(f.validate().is_err());
}

#[test]
fn payload_over_max_frame_rejected() {
    let mut f = chunk_frame(1, 0, vec![0u8; 16]);
    f.len = MAX_FRAME_PAYLOAD + 1;
    assert!(f.validate().is_err());
}

#[test]
fn chunk_over_max_chunk_len_rejected() {
    let mut f = chunk_frame(1, 0, vec![0u8; 16]);
    f.len = MAX_CHUNK_LEN + 1;
    assert!(f.validate().is_err());
}

#[test]
fn full_transfer_resume_cycle() {
    let artifact = ArtifactId::generate();
    let payload = vec![7u8; (MAX_CHUNK_LEN * 3 + 512) as usize];
    let chunk_len = MAX_CHUNK_LEN;
    let mut state = TransferState::new(artifact.clone(), payload.len() as u64, chunk_len).unwrap();

    let mut seq = 0u64;
    let mut sent: Vec<Frame> = Vec::new();
    while let Some(offset) = state.next_offset() {
        let end = ((offset + chunk_len) as usize).min(payload.len());
        let slice = payload[offset as usize..end].to_vec();
        let frame = chunk_frame(seq, offset, slice);
        frame.validate().unwrap();
        state.acknowledge(offset, &frame.digest).unwrap();
        sent.push(frame);
        seq += 1;
    }
    assert!(state.is_complete());
    assert_eq!(sent.len(), 4);

    // Simulate reconnect: last two acks "lost", replay from resume point.
    let mut resumed = TransferState::new(artifact, payload.len() as u64, chunk_len).unwrap();
    for f in &sent[..2] {
        resumed.acknowledge(f.offset.unwrap(), &f.digest).unwrap();
    }
    assert_eq!(resumed.next_offset(), sent[2].offset);

    // Replaying an already-applied ack is a no-op.
    assert!(!resumed
        .acknowledge(sent[1].offset.unwrap(), &sent[1].digest)
        .unwrap());
}

#[test]
fn ack_payload_roundtrip() {
    let ack = ChunkAck {
        artifact: ArtifactId::generate(),
        offset: 1 << 20,
        digest: "aa".repeat(32),
    };
    let back: ChunkAck = serde_json::from_str(&serde_json::to_string(&ack).unwrap()).unwrap();
    assert_eq!(ack, back);
}

#[test]
fn handshake_and_negotiation_roundtrip() {
    let hs = Handshake::Hello {
        tap_version: tapestry_core::identity::TapVersion::new(1, 0),
    };
    let back: Handshake = serde_json::from_str(&serde_json::to_string(&hs).unwrap()).unwrap();
    assert_eq!(hs, back);

    let neg = NegotiationMessage::Propose {
        codecs: vec![tapestry_core::capability::Codec::H265],
    };
    let back: NegotiationMessage =
        serde_json::from_str(&serde_json::to_string(&neg).unwrap()).unwrap();
    assert_eq!(neg, back);
}

#[test]
fn protocol_constant_matches_tap1() {
    assert_eq!(TAP_VERSION, "TAP/1");
    assert_eq!(tapestry_core::protocol::PROTOCOL_VERSION, 1);
}

#[test]
fn unknown_field_in_frame_rejected() {
    let f = chunk_frame(1, 0, vec![1]);
    let mut json = serde_json::to_value(&f).unwrap();
    json["extraneous"] = serde_json::json!(1);
    let bytes = serde_json::to_vec(&json).unwrap();
    assert!(Frame::from_json(&bytes).is_err());
}

#[test]
fn digest_error_is_displayable() {
    let err = TapestryError::InvalidValue {
        field: "digest",
        reason: "mismatch".into(),
    };
    assert!(err.to_string().contains("digest"));
}
