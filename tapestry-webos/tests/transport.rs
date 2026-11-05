use tapestry_core::protocol::frame::Frame;
use tapestry_webos::{
    ArtifactId, Channel, DeviceId, SessionId, Transport, TransportError, TransportState,
};

fn chunk(session: &SessionId, artifact: &ArtifactId, sequence: u64, offset: u64) -> Frame {
    let mut frame = Frame::new(
        session.clone(),
        Channel::Artefact,
        "chunk",
        sequence,
        DeviceId::generate(),
        b"payload".to_vec(),
    );
    frame.artifact = Some(artifact.clone());
    frame.offset = Some(offset);
    frame
}

#[test]
fn transport_requires_host_connection_and_accepts_ordered_valid_chunks() {
    let session = SessionId::generate();
    let artifact = ArtifactId::generate();
    let mut transport = Transport::new(session.clone(), DeviceId::generate());
    assert_eq!(transport.state(), TransportState::Disconnected);
    assert_eq!(
        transport.accept_frame(chunk(&session, &artifact, 1, 0)),
        Err(TransportError::NotConnected)
    );

    transport.mark_connected();
    let accepted = transport
        .accept_frame(chunk(&session, &artifact, 1, 0))
        .unwrap();
    assert_eq!(accepted.sequence, 1);
    assert_eq!(accepted.offset, 0);
    assert_eq!(accepted.payload, b"payload");

    let accepted = transport
        .accept_frame(chunk(&session, &artifact, 2, 7))
        .unwrap();
    assert_eq!(accepted.offset, 7);
    transport.disconnect();
    assert_eq!(transport.state(), TransportState::Disconnected);
}

#[test]
fn transport_rejects_corrupt_and_out_of_order_frames() {
    let session = SessionId::generate();
    let artifact = ArtifactId::generate();
    let mut transport = Transport::new(session.clone(), DeviceId::generate());
    transport.mark_connected();

    let mut corrupt = chunk(&session, &artifact, 1, 0);
    corrupt.payload[0] ^= 0xff;
    assert!(matches!(
        transport.accept_frame(corrupt),
        Err(TransportError::InvalidFrame(_))
    ));

    assert_eq!(
        transport.accept_frame(chunk(&session, &artifact, 2, 0)),
        Err(TransportError::WrongSequence {
            expected: 1,
            actual: 2
        })
    );
}

#[test]
fn transport_rejects_frames_for_another_session() {
    let session = SessionId::generate();
    let other_session = SessionId::generate();
    let artifact = ArtifactId::generate();
    let mut transport = Transport::new(session, DeviceId::generate());
    transport.mark_connected();
    assert_eq!(
        transport.accept_frame(chunk(&other_session, &artifact, 1, 0)),
        Err(TransportError::WrongSession)
    );
}
