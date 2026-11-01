//! Loopback TAP transport (Gates 1 & 7 desktop-side).
//!
//! With no television in this environment, the "device" is simulated by a
//! loopback directory: frames are written one-per-file into `inbox/`, the
//! device side validates digests, reassembles contiguously, and acknowledges
//! into `acks/`. Everything uses tapestry-core's real Frame and
//! TransferState types, so swapping this for a socket later changes
//! transport only — not semantics.

use std::path::{Path, PathBuf};

use tapestry_core::ids::ArtifactId;
use tapestry_core::protocol::chunk::TransferState;
use tapestry_core::protocol::frame::Frame;
use tapestry_core::protocol::Channel;
use tapestry_core::{Result, TapestryError};

/// Directories for one loopback session.
#[derive(Debug, Clone)]
pub struct LoopbackTransport {
    inbox: PathBuf,
    acks: PathBuf,
}

impl LoopbackTransport {
    pub fn open(root: &Path) -> Result<Self> {
        let inbox = root.join("inbox");
        let acks = root.join("acks");
        std::fs::create_dir_all(&inbox).map_err(|e| io_err("inbox", &e))?;
        std::fs::create_dir_all(&acks).map_err(|e| io_err("acks", &e))?;
        Ok(Self { inbox, acks })
    }

    /// Sender side: chunk `payload` into TAP frames and write them.
    /// Frames are numbered by seq in file names for contiguity checks.
    pub fn send_artifact(
        &self,
        session: &tapestry_core::ids::SessionId,
        sender: &tapestry_core::ids::DeviceId,
        artifact: &ArtifactId,
        payload: &[u8],
    ) -> Result<Vec<Frame>> {
        let chunk_len = tapestry_core::protocol::frame::MAX_CHUNK_LEN;
        if payload.is_empty() {
            return Err(TapestryError::InvalidValue {
                field: "payload",
                reason: "refusing to transfer an empty artefact".into(),
            });
        }
        let mut frames = Vec::new();
        for (seq, offset) in (1u64..).zip((0..payload.len()).step_by(chunk_len as usize)) {
            let end = ((offset + chunk_len as usize).min(payload.len())) as u64;
            let mut frame = Frame::new(
                session.clone(),
                Channel::Artefact,
                "chunk",
                seq,
                sender.clone(),
                payload[offset..end as usize].to_vec(),
            );
            frame.artifact = Some(artifact.clone());
            frame.offset = Some(offset as u64);
            frame.validate()?;
            let path = self.inbox.join(format!("{artifact}-{seq:06}.frame.json"));
            std::fs::write(&path, frame.to_json()?)
                .map_err(|e| io_err(&path.display().to_string(), &e))?;
            frames.push(frame);
        }
        Ok(frames)
    }

    /// Device side: read inbox frames for one artefact, verify each digest,
    /// reassemble only over a contiguous prefix, and write acks. Returns the
    /// reassembled bytes, or an error naming the first bad frame.
    pub fn receive_artifact(&self, artifact: &ArtifactId) -> Result<Vec<u8>> {
        let mut frame_files: Vec<_> = std::fs::read_dir(&self.inbox)
            .map_err(|e| io_err("inbox", &e))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&format!("{artifact}-")) && n.ends_with(".frame.json"))
                    .unwrap_or(false)
            })
            .collect();
        frame_files.sort();

        let mut reassembled = Vec::new();
        let mut transfer: Option<TransferState> = None;
        for (expected_seq, path) in frame_files.iter().enumerate() {
            let bytes = std::fs::read(path).map_err(|e| io_err(&path.display().to_string(), &e))?;
            let frame = Frame::from_json(&bytes)?;
            frame.validate()?; // digest / size / schema checks

            // Contiguity: seq must be 1..N in order.
            if frame.seq != (expected_seq + 1) as u64 {
                return Err(TapestryError::IllegalTransition(format!(
                    "non-contiguous frame seq {} at position {}",
                    frame.seq, expected_seq
                )));
            }
            let offset = frame.offset.unwrap_or(0);
            if offset != reassembled.len() as u64 {
                return Err(TapestryError::IllegalTransition(format!(
                    "frame offset {offset} does not continue at {}",
                    reassembled.len()
                )));
            }

            let ack_path = self
                .acks
                .join(format!("{}-{:06}.ack.json", artifact, frame.seq));
            if transfer.as_ref().map(|t| t.total_len).unwrap_or(0) == 0 {
                // Total length is unknown until all frames arrive; acks are
                // per-chunk and the manifest check happens in `is_complete_check`.
            }
            reassembled.extend_from_slice(&frame.payload);
            let ack = tapestry_core::protocol::chunk::ChunkAck {
                artifact: artifact.clone(),
                offset,
                digest: frame.digest.clone(),
            };
            std::fs::write(&ack_path, serde_json::to_vec(&ack)?)
                .map_err(|e| io_err(&ack_path.display().to_string(), &e))?;
            let _ = &mut transfer;
        }
        Ok(reassembled)
    }

    /// Count acknowledgements written for an artefact.
    pub fn ack_count(&self, artifact: &ArtifactId) -> usize {
        std::fs::read_dir(&self.acks)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| {
                                n.starts_with(&format!("{artifact}-")) && n.ends_with(".ack.json")
                            })
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

fn io_err(context: &str, e: &std::io::Error) -> TapestryError {
    TapestryError::IllegalTransition(format!("io failure at {context}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tapestry_core::ids::{DeviceId, SessionId};

    fn setup() -> (tempfile::TempDir, LoopbackTransport) {
        let dir = tempfile::tempdir().unwrap();
        let transport = LoopbackTransport::open(dir.path()).unwrap();
        (dir, transport)
    }

    fn ids() -> (SessionId, DeviceId, ArtifactId) {
        (
            SessionId::generate(),
            DeviceId::generate(),
            ArtifactId::generate(),
        )
    }

    #[test]
    fn roundtrip_small_payload() {
        let (_dir, t) = setup();
        let (s, d, a) = ids();
        let payload = b"hello tapestry artefact".to_vec();
        let frames = t.send_artifact(&s, &d, &a, &payload).unwrap();
        assert_eq!(frames.len(), 1);
        let received = t.receive_artifact(&a).unwrap();
        assert_eq!(received, payload);
        assert_eq!(t.ack_count(&a), 1);
    }

    #[test]
    fn multi_chunk_reassembly_is_exact() {
        let (_dir, t) = setup();
        let (s, d, a) = ids();
        let payload: Vec<u8> = (0..(tapestry_core::protocol::frame::MAX_CHUNK_LEN * 2 + 7)
            as usize)
            .map(|i| (i % 251) as u8)
            .collect();
        let frames = t.send_artifact(&s, &d, &a, &payload).unwrap();
        assert_eq!(frames.len(), 3);
        let received = t.receive_artifact(&a).unwrap();
        assert_eq!(received, payload);
        assert_eq!(t.ack_count(&a), 3);
    }

    #[test]
    fn corrupted_frame_is_rejected() {
        let (_dir, t) = setup();
        let (s, d, a) = ids();
        t.send_artifact(&s, &d, &a, b"data").unwrap();
        // Tamper with the frame on disk.
        let frame_path = std::fs::read_dir(&t.inbox)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&frame_path).unwrap()).unwrap();
        json["payload"] = serde_json::Value::Array(vec![serde_json::json!(255)]);
        std::fs::write(&frame_path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(t.receive_artifact(&a).is_err());
        assert_eq!(t.ack_count(&a), 0);
    }

    #[test]
    fn empty_payload_is_rejected_by_validation() {
        let (_dir, t) = setup();
        let (s, d, a) = ids();
        assert!(t.send_artifact(&s, &d, &a, b"").is_err());
    }
}
