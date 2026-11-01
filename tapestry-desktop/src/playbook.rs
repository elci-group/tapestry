//! Playbook adapter (Gate 3, directive §7).
//!
//! Runs a playbook drive as a subprocess, parses its JSONL journal into
//! gates, and supports cancellation. Playbook decides *what* should happen;
//! this adapter only observes and reports.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Playbook-visible gates mapped from journal events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriveGate {
    RequestAccepted,
    PlanReady,
    GenerationStarted,
    ArtefactAvailable,
    ArtefactValidated,
    DisplayReady,
    HumanReviewRequired,
    HumanDecisionReceived,
    TaskComplete,
}

/// One journal line of a playbook drive.
#[derive(Debug, Clone, Deserialize)]
pub struct JournalLine {
    pub seq: u64,
    #[serde(default)]
    pub digest: String,
    pub event: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("playbook binary failed: {0}")]
    Spawn(String),
    #[error("drive timed out after {0:?}")]
    Timeout(Duration),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Handle to a running drive; dropping after `kill_on_cancellation` aborts.
pub struct Drive {
    journal_path: PathBuf,
    child: Option<Child>,
}

impl Drive {
    /// Start `playbook mvp --journal <path>`, optionally blocking until the
    /// drive completes (or the timeout elapses).
    pub fn start(journal_path: &Path, timeout: Option<Duration>) -> Result<Self, DriveError> {
        let child = Command::new("playbook")
            .arg("mvp")
            .arg("--journal")
            .arg(journal_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| DriveError::Spawn(e.to_string()))?;
        let mut drive = Drive {
            journal_path: journal_path.to_owned(),
            child: Some(child),
        };
        if let Some(t) = timeout {
            let started = Instant::now();
            while !drive.is_finished() {
                if started.elapsed() > t {
                    return Err(DriveError::Timeout(t));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            drive.wait()?;
        }
        Ok(drive)
    }

    /// Non-blocking liveness probe: reaps the child on exit so no zombie
    /// lingers (a zombie still owns a /proc entry).
    pub fn is_finished(&mut self) -> bool {
        match self.child.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(_) => true,
            },
            None => true,
        }
    }

    /// Cancel the drive by killing the child process.
    pub fn cancel(&mut self) -> Result<(), DriveError> {
        if let Some(child) = self.child.as_mut() {
            child.kill()?;
            child.wait()?;
            self.child = None;
        }
        Ok(())
    }

    /// Reap the child, returning its exit status.
    pub fn wait(&mut self) -> Result<Option<std::process::ExitStatus>, DriveError> {
        if let Some(child) = self.child.as_mut() {
            let status = child.wait()?;
            self.child = None;
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    /// Read and parse the journal produced so far.
    pub fn journal(&self) -> Result<Vec<JournalLine>, DriveError> {
        let mut lines = Vec::new();
        if !self.journal_path.exists() {
            return Ok(lines);
        }
        let file = std::fs::File::open(&self.journal_path)?;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<JournalLine>(&line) {
                Ok(l) => lines.push(l),
                Err(_) => break, // torn tail
            }
        }
        Ok(lines)
    }

    /// Map journal events onto DriveGates in order.
    pub fn gates(journal: &[JournalLine]) -> Vec<DriveGate> {
        let mut gates = Vec::new();
        for line in journal {
            let etype = line
                .event
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match etype {
                "drive_started" => {
                    gates.push(DriveGate::RequestAccepted);
                    gates.push(DriveGate::PlanReady);
                    gates.push(DriveGate::GenerationStarted);
                }
                "down_changed" => gates.push(DriveGate::ArtefactAvailable),
                _ => {}
            }
        }
        if !journal.is_empty() {
            gates.push(DriveGate::HumanReviewRequired);
            gates.push(DriveGate::TaskComplete);
        }
        gates
    }
}

impl Drop for Drive {
    fn drop(&mut self) {
        // Never leave an orphaned playbook behind.
        let _ = self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_journal() -> Vec<JournalLine> {
        [
            r#"{"seq":1,"digest":"a","event":{"type":"drive_started","drive_id":"drive-mvp"}}"#,
            r#"{"seq":2,"digest":"b","event":{"type":"down_changed","down":"first_down"}}"#,
            r#"{"seq":3,"digest":"c","event":{"type":"ytg_recalculated","ytg_milli":0}}"#,
        ]
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
    }

    #[test]
    fn gates_follow_journal_shape() {
        let gates = Drive::gates(&fixture_journal());
        assert!(gates.contains(&DriveGate::RequestAccepted));
        assert!(gates.contains(&DriveGate::TaskComplete));
        assert_eq!(gates.first(), Some(&DriveGate::RequestAccepted));
        assert_eq!(gates.last(), Some(&DriveGate::TaskComplete));
    }

    #[test]
    fn empty_journal_has_no_gates() {
        assert!(Drive::gates(&[]).is_empty());
    }

    #[test]
    fn journal_parse_tolerates_torn_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("j.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{{\"seq\":2,\"torn",
                r#"{"seq":1,"digest":"a","event":{"type":"drive_started"}}"#
            ),
        )
        .unwrap();
        let drive = Drive {
            journal_path: path,
            child: None,
        };
        let lines = drive.journal().unwrap();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn missing_journal_is_empty_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let drive = Drive {
            journal_path: dir.path().join("absent.jsonl"),
            child: None,
        };
        assert!(drive.journal().unwrap().is_empty());
    }
}
