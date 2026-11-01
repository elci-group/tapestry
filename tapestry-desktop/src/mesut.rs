//! Mesut-shaped local task classification and lanes (Gate 4, directive §8).
//!
//! Playbook issues work; Mesut classifies and lanes it. Struct only —
//! no threads, so the POC scheduler is deterministic and testable.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadClass {
    Io,
    Compute,
    Blocking,
    Async,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// Backpressure bound: one heavy media task must not starve navigation.
const MAX_LANE_LEN: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("lane full for priority {0:?}: backpressure engaged")]
    LaneFull(Priority),
}

/// A unit of local work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub name: String,
    pub class: WorkloadClass,
    pub priority: Priority,
    pub estimated_ms: u64,
}

/// Classify by task name and optional size hint (bytes).
pub fn classify(task_name: &str, size_hint: Option<u64>) -> (WorkloadClass, Priority) {
    let name = task_name.to_ascii_lowercase();
    let heavy = size_hint.unwrap_or(0) > 16 << 20; // 16 MiB
    match name.split_whitespace().next().unwrap_or(&name) {
        n if n.contains("hash")
            || n.contains("thumbnail")
            || n.contains("qr")
            || n.contains("transcode") =>
        {
            let class = WorkloadClass::Compute;
            let priority = if heavy { Priority::Low } else { Priority::High };
            (class, priority)
        }
        n if n.contains("read") || n.contains("probe") || n.contains("metadata") => {
            (WorkloadClass::Blocking, Priority::High)
        }
        n if n.contains("upload")
            || n.contains("download")
            || n.contains("transfer")
            || n.contains("network") =>
        {
            (WorkloadClass::Async, Priority::Normal)
        }
        n if n.contains("remote")
            || n.contains("review")
            || n.contains("cancel")
            || n.contains("auth") =>
        {
            (WorkloadClass::Critical, Priority::Critical)
        }
        _ if heavy => (WorkloadClass::Compute, Priority::Low),
        _ => (WorkloadClass::Io, Priority::Normal),
    }
}

impl TaskSpec {
    pub fn from_name(task_name: &str, size_hint: Option<u64>) -> Self {
        let (class, priority) = classify(task_name, size_hint);
        Self {
            name: task_name.to_owned(),
            class,
            priority,
            estimated_ms: 0,
        }
    }
}

/// Priority lanes with backpressure and drain-by-session cancellation.
#[derive(Debug, Default)]
pub struct Lanes {
    lanes: [VecDeque<TaskSpec>; 4],
}

impl Lanes {
    pub fn new() -> Self {
        Self::default()
    }

    fn index(p: Priority) -> usize {
        match p {
            Priority::Critical => 3,
            Priority::High => 2,
            Priority::Normal => 1,
            Priority::Low => 0,
        }
    }

    /// Enqueue with backpressure: non-critical lanes reject when full.
    pub fn push(&mut self, task: TaskSpec) -> Result<(), SchedulerError> {
        let idx = Self::index(task.priority);
        if self.lanes[idx].len() >= MAX_LANE_LEN && task.priority != Priority::Critical {
            return Err(SchedulerError::LaneFull(task.priority));
        }
        self.lanes[idx].push_back(task);
        Ok(())
    }

    /// Pop the most urgent task: Critical first, Low last.
    pub fn pop_next(&mut self) -> Option<TaskSpec> {
        (0..4).rev().find_map(|i| self.lanes[i].pop_front())
    }

    /// Drain queued work for an abandoned session (structured cancellation).
    pub fn drain_cancelled(&mut self, session_hint: &str) -> usize {
        let mut drained = 0;
        for lane in self.lanes.iter_mut() {
            let before = lane.len();
            lane.retain(|t| !t.name.contains(session_hint));
            drained += before - lane.len();
        }
        drained
    }

    pub fn len(&self) -> usize {
        self.lanes.iter().map(VecDeque::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_rules() {
        assert_eq!(
            classify("hash artefact bytes", None),
            (WorkloadClass::Compute, Priority::High)
        );
        assert_eq!(
            classify("probe metadata", None),
            (WorkloadClass::Blocking, Priority::High)
        );
        assert_eq!(
            classify("transfer chunks", None),
            (WorkloadClass::Async, Priority::Normal)
        );
        assert_eq!(
            classify("review submission", None),
            (WorkloadClass::Critical, Priority::Critical)
        );
    }

    #[test]
    fn heavy_compute_downgrades_to_low() {
        assert_eq!(
            classify("transcode", Some(64 << 20)),
            (WorkloadClass::Compute, Priority::Low)
        );
    }

    #[test]
    fn critical_pops_first() {
        let mut lanes = Lanes::new();
        lanes
            .push(TaskSpec::from_name("thumbnail generation", None))
            .unwrap();
        lanes
            .push(TaskSpec::from_name("cancel session", None))
            .unwrap();
        let next = lanes.pop_next().unwrap();
        assert_eq!(next.priority, Priority::Critical);
        assert_eq!(lanes.len(), 1);
    }

    #[test]
    fn backpressure_rejects_full_lanes_but_not_critical() {
        let mut lanes = Lanes::new();
        for i in 0..MAX_LANE_LEN {
            lanes
                .push(TaskSpec {
                    name: format!("normal {i}"),
                    class: WorkloadClass::Io,
                    priority: Priority::Normal,
                    estimated_ms: 1,
                })
                .unwrap();
        }
        let err = lanes
            .push(TaskSpec::from_name("normal overflow", None))
            .unwrap_err();
        assert!(matches!(err, SchedulerError::LaneFull(Priority::Normal)));
        // Critical admission is never refused.
        lanes.push(TaskSpec::from_name("auth", None)).unwrap();
        assert_eq!(lanes.pop_next().unwrap().priority, Priority::Critical);
    }

    #[test]
    fn drain_cancelled_removes_only_matching_session() {
        let mut lanes = Lanes::new();
        lanes
            .push(TaskSpec {
                name: "tap-01 thumbnail".into(),
                class: WorkloadClass::Compute,
                priority: Priority::High,
                estimated_ms: 5,
            })
            .unwrap();
        lanes
            .push(TaskSpec {
                name: "tap-02 transfer".into(),
                class: WorkloadClass::Async,
                priority: Priority::Normal,
                estimated_ms: 5,
            })
            .unwrap();
        assert_eq!(lanes.drain_cancelled("tap-01"), 1);
        assert_eq!(lanes.len(), 1);
    }

    #[test]
    fn empty_lanes_report_empty() {
        assert!(Lanes::new().is_empty());
    }
}
