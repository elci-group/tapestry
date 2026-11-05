use tapestry_core::ids::{ArtifactId, DeviceId, SessionId};
use tapestry_core::protocol::frame::Frame;
use tapestry_core::protocol::Channel;

/// Lifecycle for the host-provided WebOS channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Disconnected,
    Connected,
}

/// Result of validating one incoming artefact chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedChunk {
    pub artifact: ArtifactId,
    pub sequence: u64,
    pub offset: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    NotConnected,
    InvalidFrame(String),
    WrongSession,
    WrongChannel,
    MissingArtifact,
    MissingOffset,
    ChangedArtifact,
    WrongSequence { expected: u64, actual: u64 },
    WrongOffset { expected: u64, actual: u64 },
    LengthOverflow,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "transport is disconnected"),
            Self::InvalidFrame(reason) => write!(f, "invalid TAP frame: {reason}"),
            Self::WrongSession => write!(f, "frame belongs to another session"),
            Self::WrongChannel => write!(f, "only artefact chunk frames are accepted"),
            Self::MissingArtifact => write!(f, "chunk frame has no artefact id"),
            Self::MissingOffset => write!(f, "chunk frame has no byte offset"),
            Self::ChangedArtifact => write!(f, "transfer changed artefact before completion"),
            Self::WrongSequence { expected, actual } => {
                write!(f, "expected frame sequence {expected}, got {actual}")
            }
            Self::WrongOffset { expected, actual } => {
                write!(f, "expected byte offset {expected}, got {actual}")
            }
            Self::LengthOverflow => write!(f, "artefact byte offset overflow"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Validates TAP frames supplied by a host-owned WebOS channel.
///
/// This type does not create a socket, discover peers, or claim an
/// authenticated connection. The native application owns those operations.
#[derive(Debug, Clone)]
pub struct Transport {
    pub session_id: SessionId,
    pub device_id: DeviceId,
    state: TransportState,
    artifact: Option<ArtifactId>,
    next_sequence: u64,
    next_offset: u64,
}

impl Transport {
    pub fn new(session_id: SessionId, device_id: DeviceId) -> Self {
        Self {
            session_id,
            device_id,
            state: TransportState::Disconnected,
            artifact: None,
            next_sequence: 1,
            next_offset: 0,
        }
    }

    /// Mark connected only after the native host has established its channel.
    pub fn mark_connected(&mut self) {
        self.state = TransportState::Connected;
    }

    pub fn disconnect(&mut self) {
        self.state = TransportState::Disconnected;
    }

    pub fn state(&self) -> TransportState {
        self.state
    }

    /// Validate and accept one chunk. Accepted payload ownership returns to
    /// the caller; durable storage and rendering belong to later gates.
    pub fn accept_frame(&mut self, frame: Frame) -> Result<AcceptedChunk, TransportError> {
        if self.state != TransportState::Connected {
            return Err(TransportError::NotConnected);
        }
        frame
            .validate()
            .map_err(|e| TransportError::InvalidFrame(e.to_string()))?;
        if frame.session != self.session_id {
            return Err(TransportError::WrongSession);
        }
        if frame.channel != Channel::Artefact || frame.frame_type != "chunk" {
            return Err(TransportError::WrongChannel);
        }
        if frame.seq != self.next_sequence {
            return Err(TransportError::WrongSequence {
                expected: self.next_sequence,
                actual: frame.seq,
            });
        }
        let artifact = frame.artifact.ok_or(TransportError::MissingArtifact)?;
        if self
            .artifact
            .as_ref()
            .is_some_and(|active| active != &artifact)
        {
            return Err(TransportError::ChangedArtifact);
        }
        let offset = frame.offset.ok_or(TransportError::MissingOffset)?;
        if offset != self.next_offset {
            return Err(TransportError::WrongOffset {
                expected: self.next_offset,
                actual: offset,
            });
        }
        let next_offset = self
            .next_offset
            .checked_add(frame.len)
            .ok_or(TransportError::LengthOverflow)?;

        self.artifact = Some(artifact.clone());
        self.next_sequence += 1;
        self.next_offset = next_offset;
        Ok(AcceptedChunk {
            artifact,
            sequence: frame.seq,
            offset,
            payload: frame.payload,
        })
    }
}
