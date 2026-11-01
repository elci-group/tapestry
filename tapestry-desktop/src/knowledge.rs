//! Padagonia-shaped durable knowledge journal (Gate 5, directive §26/§27).
//!
//! Append-only, digest-chained JSONL: provenance and confidence are
//! preserved and never collapsed into one truth category. Not a consensus
//! database.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Device,
    User,
    Agent,
    Session,
    Prompt,
    Task,
    Artefact,
    Revision,
    Model,
    Provider,
    Review,
    Decision,
    Annotation,
    Capability,
    StorageVolume,
    Transfer,
    Render,
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Relationship {
    GeneratedBy,
    DerivedFrom,
    DisplayedOn,
    RequestedBy,
    ReviewedBy,
    SelectedOver,
    RejectedFor,
    SentTo,
    ReceivedFrom,
    HostedBy,
    ConnectedVia,
    Supersedes,
    BelongsToSession,
    HasCapability,
}

/// Where a piece of knowledge came from. These categories MUST NOT merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    ExplicitHumanJudgement,
    AgentJudgement,
    InferredPreference,
    HistoricalCorrelation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEdge {
    pub from: String,
    pub rel: Relationship,
    pub to: String,
    /// 0.0..1.0 — confidence is per-edge, per-origin.
    pub confidence: f32,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JournalEvent {
    Node(KnowledgeNode),
    Edge(KnowledgeEdge),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
    seq: u64,
    /// sha256(prev_digest || payload).
    digest: String,
    event: JournalEvent,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("torn or tampered journal at seq {0}")]
    Integrity(u64),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// File-backed knowledge journal.
#[derive(Debug, Clone)]
pub struct KnowledgeJournal {
    path: PathBuf,
    seq: u64,
    head_digest: String,
    nodes: Vec<KnowledgeNode>,
    edges: Vec<KnowledgeEdge>,
}

fn genesis_digest() -> String {
    hex::encode(Sha256::digest(b"tapestry-genesis"))
}

impl KnowledgeJournal {
    /// Open (or create) a journal, replaying and verifying the chain.
    /// Truncates at the first torn/tampered record — what survives is
    /// exactly what provably happened.
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        let mut journal = Self {
            path: path.to_owned(),
            seq: 0,
            head_digest: genesis_digest(),
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        if path.exists() {
            journal.replay()?;
            journal.persist_verified_prefix()?;
        }
        Ok(journal)
    }

    fn replay(&mut self) -> Result<(), JournalError> {
        let file = std::fs::File::open(&self.path)?;
        for line in std::io::BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: JournalRecord = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => break, // torn tail write
            };
            if record.seq != self.seq + 1 {
                break;
            }
            if !verify_chain(&self.head_digest, &record) {
                break;
            }
            self.seq = record.seq;
            self.head_digest = record.digest.clone();
            match &record.event {
                JournalEvent::Node(n) => self.nodes.push(n.clone()),
                JournalEvent::Edge(e) => self.edges.push(e.clone()),
            }
        }
        Ok(())
    }

    /// Rewrite the file with only the verified prefix (atomic via tmp+rename).
    fn persist_verified_prefix(&self) -> Result<(), JournalError> {
        // The original payloads must be preserved byte-exact, so filter lines.
        let file = std::fs::File::open(&self.path)?;
        let mut verified = String::new();
        let mut seq = 0u64;
        let mut head = genesis_digest();
        for line in std::io::BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let record: JournalRecord = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => break,
            };
            if record.seq != seq + 1 || !verify_chain(&head, &record) {
                break;
            }
            seq = record.seq;
            head = record.digest.clone();
            verified.push_str(&line);
            verified.push('\n');
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, verified)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn append_record(&mut self, event: JournalEvent) -> Result<(), JournalError> {
        let payload = serde_json::to_vec(&event)?;
        let mut hasher = Sha256::new();
        hasher.update(self.head_digest.as_bytes());
        hasher.update(&payload);
        let digest = hex::encode(hasher.finalize());
        let record = JournalRecord {
            seq: self.seq + 1,
            digest: digest.clone(),
            event,
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(&record)?)?;
        file.flush()?;
        self.seq += 1;
        self.head_digest = digest;
        // Keep the in-memory view consistent with what we just persisted.
        match record.event {
            JournalEvent::Node(n) => self.nodes.push(n),
            JournalEvent::Edge(e) => self.edges.push(e),
        }
        Ok(())
    }

    pub fn append_node(&mut self, node: KnowledgeNode) -> Result<(), JournalError> {
        self.append_record(JournalEvent::Node(node))
    }

    /// Append an edge; both endpoints must exist so the graph never
    /// references nothing.
    pub fn append_edge(&mut self, edge: KnowledgeEdge) -> Result<(), JournalError> {
        if !self.has_node(&edge.from) || !self.has_node(&edge.to) {
            return Err(JournalError::Integrity(self.seq));
        }
        self.append_record(JournalEvent::Edge(edge))
    }

    fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n.id == id)
    }

    /// Edges where a human explicitly judged (SELECTED_OVER etc.).
    pub fn human_edges(&self) -> impl Iterator<Item = &KnowledgeEdge> {
        self.edges
            .iter()
            .filter(|e| e.origin == Origin::ExplicitHumanJudgement)
    }

    pub fn nodes(&self) -> &[KnowledgeNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[KnowledgeEdge] {
        &self.edges
    }
}

fn verify_chain(prev_digest: &str, record: &JournalRecord) -> bool {
    let payload = match serde_json::to_vec(&record.event) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut hasher = Sha256::new();
    hasher.update(prev_digest.as_bytes());
    hasher.update(&payload);
    hex::encode(hasher.finalize()) == record.digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> KnowledgeNode {
        KnowledgeNode {
            id: id.into(),
            kind,
            label: id.into(),
        }
    }

    fn human_edge(from: &str, rel: Relationship, to: &str) -> KnowledgeEdge {
        KnowledgeEdge {
            from: from.into(),
            rel,
            to: to.into(),
            confidence: 1.0,
            origin: Origin::ExplicitHumanJudgement,
        }
    }

    #[test]
    fn append_and_replay_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.jsonl");
        {
            let mut j = KnowledgeJournal::open(&path).unwrap();
            j.append_node(node("art_C", NodeKind::Artefact)).unwrap();
            j.append_node(node("art_A", NodeKind::Artefact)).unwrap();
            j.append_node(node("human_1", NodeKind::User)).unwrap();
            j.append_edge(human_edge("art_C", Relationship::SelectedOver, "art_A"))
                .unwrap();
            j.append_edge(human_edge("art_C", Relationship::ReviewedBy, "human_1"))
                .unwrap();
        }
        let j = KnowledgeJournal::open(&path).unwrap();
        assert_eq!(j.nodes().len(), 3);
        assert_eq!(j.edges().len(), 2);
        assert_eq!(j.human_edges().count(), 2);
    }

    #[test]
    fn edge_to_missing_node_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.jsonl");
        let mut j = KnowledgeJournal::open(&path).unwrap();
        let err = j
            .append_edge(human_edge("ghost", Relationship::ReviewedBy, "human_1"))
            .unwrap_err();
        assert!(matches!(err, JournalError::Integrity(_)));
    }

    #[test]
    fn torn_tail_truncated_verified_prefix_survives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.jsonl");
        {
            let mut j = KnowledgeJournal::open(&path).unwrap();
            j.append_node(node("art_A", NodeKind::Artefact)).unwrap();
        }
        // Simulate a torn write: valid record + half-written line.
        let mut body = std::fs::read_to_string(&path).unwrap();
        body.push_str("{\"seq\":2,\"digest\":\"zz");
        std::fs::write(&path, body).unwrap();
        let j = KnowledgeJournal::open(&path).unwrap();
        assert_eq!(j.nodes().len(), 1);
        // And the torn tail is gone from disk.
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("\"digest\":\"zz"));
    }

    #[test]
    fn tampered_digest_stops_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k.jsonl");
        {
            let mut j = KnowledgeJournal::open(&path).unwrap();
            j.append_node(node("art_A", NodeKind::Artefact)).unwrap();
            j.append_node(node("art_B", NodeKind::Artefact)).unwrap();
        }
        // Tamper with the second record's digest.
        let contents = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = contents.lines().map(str::to_owned).collect();
        assert!(lines.len() >= 2, "fixture journal should have 2 records");
        let mut rec: JournalRecord = serde_json::from_str(&lines[1]).unwrap();
        rec.digest = "0".repeat(64);
        lines[1] = serde_json::to_string(&rec).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let j = KnowledgeJournal::open(&path).unwrap();
        assert_eq!(j.nodes().len(), 1, "tampered record must not survive");
    }

    #[test]
    fn origins_do_not_collapse() {
        let agent = KnowledgeEdge {
            from: "art_A".into(),
            rel: Relationship::GeneratedBy,
            to: "model_x".into(),
            confidence: 0.8,
            origin: Origin::AgentJudgement,
        };
        let human = KnowledgeEdge {
            origin: Origin::ExplicitHumanJudgement,
            ..agent.clone()
        };
        assert_ne!(agent.origin, human.origin);
    }
}
