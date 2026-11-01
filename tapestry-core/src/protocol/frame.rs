//! TAP/1 frame schema.
//!
//! Every message on the wire is a frame: session-scoped, channel-tagged,
//! sequence-numbered, length-prefixed and digest-stamped. Frames are pure
//! data here; the byte encoding is the transport swarm's concern.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ids::{ArtifactId, DeviceId, SessionId};
use crate::protocol::Channel;
use crate::{Result, TapestryError};

/// Maximum frame payload accepted by a conforming receiver (guards against
/// decompression/memory attacks, §28).
pub const MAX_FRAME_PAYLOAD: u64 = 64 << 20; // 64 MiB

/// Maximum chunk size for artefact transfer.
pub const MAX_CHUNK_LEN: u64 = 1 << 20; // 1 MiB

/// A TAP frame. `payload` is opaque at this layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// Always `1` on the wire for TAP/1.
    pub schema_version: u32,
    pub session: SessionId,
    pub channel: Channel,
    /// Frame type within the channel, e.g. "chunk", "ack", "heartbeat".
    pub frame_type: String,
    /// Monotonic per-session, per-channel sequence. Idempotency key.
    pub seq: u64,
    /// Sender device identity.
    pub sender: DeviceId,
    /// Present for artefact/media/review frames.
    pub artifact: Option<ArtifactId>,
    /// Byte offset into the artefact, for chunked transfer.
    pub offset: Option<u64>,
    /// Payload length in bytes.
    pub len: u64,
    /// Hex SHA-256 of the payload.
    pub digest: String,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Construct a frame carrying `payload`, computing `len` and `digest`.
    pub fn new(
        session: SessionId,
        channel: Channel,
        frame_type: impl Into<String>,
        seq: u64,
        sender: DeviceId,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            schema_version: crate::protocol::PROTOCOL_VERSION,
            session,
            channel,
            frame_type: frame_type.into(),
            seq,
            sender,
            artifact: None,
            offset: None,
            len: payload.len() as u64,
            digest: hex::encode(Sha256::digest(&payload)),
            payload,
        }
    }

    /// Structural validation: schema version, size limits, digest match.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != crate::protocol::PROTOCOL_VERSION {
            return Err(TapestryError::InvalidValue {
                field: "schema_version",
                reason: format!("expected {}", crate::protocol::PROTOCOL_VERSION),
            });
        }
        if self.len > MAX_FRAME_PAYLOAD {
            return Err(TapestryError::InvalidValue {
                field: "len",
                reason: format!("exceeds MAX_FRAME_PAYLOAD ({MAX_FRAME_PAYLOAD})"),
            });
        }
        if self.len != self.payload.len() as u64 {
            return Err(TapestryError::InvalidValue {
                field: "len",
                reason: format!(
                    "declared {} but payload is {}",
                    self.len,
                    self.payload.len()
                ),
            });
        }
        if self.digest != hex::encode(Sha256::digest(&self.payload)) {
            return Err(TapestryError::InvalidValue {
                field: "digest",
                reason: "payload digest mismatch".into(),
            });
        }
        if let Some(offset) = self.offset {
            if self.frame_type != "chunk" {
                return Err(TapestryError::InvalidValue {
                    field: "offset",
                    reason: "only chunk frames carry an offset".into(),
                });
            }
            if self.len > MAX_CHUNK_LEN {
                return Err(TapestryError::InvalidValue {
                    field: "len",
                    reason: format!("chunk exceeds MAX_CHUNK_LEN ({MAX_CHUNK_LEN})"),
                });
            }
            let _ = offset;
        }
        Ok(())
    }

    /// Serialise to JSON and back, byte-for-byte stable.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(payload: Vec<u8>) -> Frame {
        Frame::new(
            SessionId::generate(),
            Channel::Artefact,
            "chunk",
            441,
            DeviceId::generate(),
            payload,
        )
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let f = sample(b"hello artefact".to_vec());
        let json = f.to_json().unwrap();
        let back = Frame::from_json(&json).unwrap();
        assert_eq!(f, back);
        assert_eq!(f.to_json().unwrap(), json);
    }

    #[test]
    fn digest_detects_corruption() {
        let mut f = sample(b"payload".to_vec());
        assert!(f.validate().is_ok());
        f.payload.push(b'!');
        assert!(f.validate().is_err());
    }

    #[test]
    fn oversized_frame_rejected() {
        let mut f = sample(vec![0u8; 64]);
        f.len = MAX_FRAME_PAYLOAD + 1;
        assert!(f.validate().is_err());
    }

    #[test]
    fn chunk_size_limit_enforced() {
        let mut f = sample(vec![0u8; 8]);
        f.frame_type = "chunk".into();
        f.offset = Some(0);
        f.len = MAX_CHUNK_LEN + 1;
        assert!(f.validate().is_err());
    }

    #[test]
    fn offset_only_on_chunks() {
        // Non-chunk frames must not carry an offset.
        let mut f = Frame::new(
            SessionId::generate(),
            Channel::Control,
            "heartbeat",
            1,
            DeviceId::generate(),
            vec![0u8; 8],
        );
        assert!(f.validate().is_ok());
        f.offset = Some(16);
        assert!(f.validate().is_err());
    }

    #[test]
    fn wrong_schema_version_rejected() {
        let mut f = sample(vec![0u8; 8]);
        f.schema_version = 2;
        assert!(f.validate().is_err());
    }
}
