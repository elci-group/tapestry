//! Resumable chunk bookkeeping for artefact transfer (directive §9).
//!
//! Tracks which byte ranges of an artefact have been acknowledged so a
//! reconnect resumes rather than restarts, and a replayed frame cannot
//! create duplicate effects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::ArtifactId;
use crate::{Result, TapestryError};

/// Receiver-side resume state for one artefact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferState {
    pub artifact: ArtifactId,
    pub total_len: u64,
    pub chunk_len: u64,
    /// Offset → chunk digest, for every acknowledged chunk.
    pub acknowledged: BTreeMap<u64, String>,
}

impl TransferState {
    /// New transfer state; `total_len` must be chunkable by `chunk_len`.
    pub fn new(artifact: ArtifactId, total_len: u64, chunk_len: u64) -> Result<Self> {
        if chunk_len == 0 || chunk_len > crate::protocol::frame::MAX_CHUNK_LEN {
            return Err(TapestryError::InvalidValue {
                field: "chunk_len",
                reason: format!("must be 1..={}", crate::protocol::frame::MAX_CHUNK_LEN),
            });
        }
        if total_len == 0 {
            return Err(TapestryError::InvalidValue {
                field: "total_len",
                reason: "artefact must not be empty".into(),
            });
        }
        Ok(Self {
            artifact,
            total_len,
            chunk_len,
            acknowledged: BTreeMap::new(),
        })
    }

    /// Next unacknowledged offset, or `None` when complete.
    pub fn next_offset(&self) -> Option<u64> {
        let mut expected = 0u64;
        for &offset in self.acknowledged.keys() {
            if offset == expected {
                expected += self.chunk_len;
            } else {
                break;
            }
        }
        (expected < self.total_len).then_some(expected)
    }

    /// Acknowledge a chunk at `offset` with `digest`. Re-acknowledging the
    /// same offset with the same digest is a no-op (idempotent); a
    /// conflicting digest is an error.
    pub fn acknowledge(&mut self, offset: u64, digest: impl Into<String>) -> Result<bool> {
        if offset >= self.total_len || offset % self.chunk_len != 0 {
            return Err(TapestryError::InvalidValue {
                field: "offset",
                reason: format!(
                    "not a valid chunk boundary for len {total}",
                    total = self.total_len
                ),
            });
        }
        let digest = digest.into();
        match self.acknowledged.get(&offset) {
            Some(existing) if *existing == digest => Ok(false),
            Some(_) => Err(TapestryError::InvalidValue {
                field: "digest",
                reason: "conflicting digest for already-acknowledged chunk".into(),
            }),
            None => {
                self.acknowledged.insert(offset, digest);
                Ok(true)
            }
        }
    }

    /// Whether every byte of the artefact is acknowledged.
    pub fn is_complete(&self) -> bool {
        self.next_offset().is_none()
    }

    /// Bytes acknowledged so far (clamped to `total_len`).
    pub fn received_bytes(&self) -> u64 {
        let chunks = self.acknowledged.len() as u64;
        chunks.saturating_mul(self.chunk_len).min(self.total_len)
    }
}

/// Acknowledgement frame payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkAck {
    pub artifact: ArtifactId,
    pub offset: u64,
    pub digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> TransferState {
        TransferState::new(ArtifactId::generate(), 10 * (1 << 20), 1 << 20).unwrap()
    }

    #[test]
    fn chunk_bounds_enforced() {
        assert!(TransferState::new(ArtifactId::generate(), 100, 0).is_err());
        assert!(TransferState::new(ArtifactId::generate(), 100, (2 << 20) as u64).is_err());
        assert!(TransferState::new(ArtifactId::generate(), 0, 1024).is_err());
    }

    #[test]
    fn resume_walks_offsets_in_order() {
        let mut s = state();
        for i in 0..5u64 {
            assert_eq!(s.next_offset(), Some(i * s.chunk_len));
            s.acknowledge(i * s.chunk_len, format!("d{i}")).unwrap();
        }
        assert_eq!(s.received_bytes(), 5 * s.chunk_len);
        assert!(!s.is_complete());
    }

    #[test]
    fn complete_when_all_chunks_acknowledged() {
        let mut s = state();
        let chunks = s.total_len.div_ceil(s.chunk_len);
        for i in 0..chunks {
            s.acknowledge(i * s.chunk_len, "d").unwrap();
        }
        assert!(s.is_complete());
        assert_eq!(s.next_offset(), None);
        assert_eq!(s.received_bytes(), s.total_len);
    }

    #[test]
    fn duplicate_ack_is_noop_conflict_is_error() {
        let mut s = state();
        assert!(s.acknowledge(0, "digest-a").unwrap());
        assert!(!s.acknowledge(0, "digest-a").unwrap());
        assert!(s.acknowledge(0, "digest-b").is_err());
    }

    #[test]
    fn misaligned_and_out_of_range_offsets_rejected() {
        let mut s = state();
        assert!(s.acknowledge(512, "d").is_err());
        assert!(s.acknowledge(s.total_len, "d").is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let mut s = state();
        s.acknowledge(0, "d").unwrap();
        let back: TransferState =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
    }
}
