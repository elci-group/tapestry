//! PV/Bart-shaped resource and storage visibility (Gate 6, directives §15/§16).
//!
//! Advisory signals only: pressure informs scheduling, it does not gate it
//! unless a safety policy says otherwise. Bart measures; it does not own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheClass {
    Hot,
    Warm,
    Cold,
    External,
}

/// Size accounting for one directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirUsage {
    pub path: PathBuf,
    pub bytes: u64,
    pub file_count: u64,
    pub class: CacheClass,
}

/// Unified storage report (§16): internal cache, staging, external volumes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageReport {
    pub entries: Vec<DirUsage>,
    pub total_bytes: u64,
}

fn walk_usage(path: &Path, class: CacheClass) -> DirUsage {
    let mut usage = DirUsage {
        path: path.to_owned(),
        bytes: 0,
        file_count: 0,
        class,
    };
    let mut stack = vec![path.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                usage.bytes += meta.len();
                usage.file_count += 1;
            }
        }
    }
    usage
}

/// Measure a set of paths with cache-class labels.
pub fn storage_report(entries: &[(PathBuf, CacheClass)]) -> StorageReport {
    let mut report = StorageReport::default();
    for (path, class) in entries {
        if path.exists() {
            let usage = walk_usage(path, *class);
            report.total_bytes += usage.bytes;
            report.entries.push(usage);
        }
    }
    report
}

/// Host pressure signals (PV-shaped), read from /proc. Advisory only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureSignals {
    pub mem_available_bytes: u64,
    pub load_1m: f32,
}

impl PressureSignals {
    /// None when /proc is unavailable (non-Linux) or unreadable.
    pub fn read() -> Option<Self> {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let available_kb = meminfo
            .lines()
            .find(|l| l.starts_with("MemAvailable:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())?;
        let loadavg = std::fs::read_to_string("/proc/loadavg").ok()?;
        let load_1m = loadavg
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0);
        Some(Self {
            mem_available_bytes: available_kb * 1024,
            load_1m,
        })
    }

    /// Advisory judgement: is the host under memory pressure?
    pub fn memory_pressure(&self) -> bool {
        self.mem_available_bytes < 1 << 30 // < 1 GiB
    }
}

/// One named check's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

impl CheckResult {
    pub fn new(name: impl Into<String>, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ok,
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_report_measures_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 1000]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b.bin"), vec![0u8; 500]).unwrap();
        let report = storage_report(&[(dir.path().into(), CacheClass::Hot)]);
        assert_eq!(report.total_bytes, 1500);
        assert_eq!(report.entries[0].file_count, 2);
        assert_eq!(report.entries[0].class, CacheClass::Hot);
    }

    #[test]
    fn missing_paths_are_skipped() {
        let report = storage_report(&[(
            PathBuf::from("/nonexistent/tapestry-path"),
            CacheClass::Cold,
        )]);
        assert!(report.entries.is_empty());
        assert_eq!(report.total_bytes, 0);
    }

    #[test]
    fn pressure_signals_read_or_none() {
        // Either parses /proc on Linux or returns None elsewhere; both are
        // acceptable — the type just must not panic.
        let _ = PressureSignals::read();
    }

    #[test]
    fn memory_pressure_threshold() {
        let low = PressureSignals {
            mem_available_bytes: 512 << 20,
            load_1m: 0.5,
        };
        let ok = PressureSignals {
            mem_available_bytes: 8 << 30,
            load_1m: 0.5,
        };
        assert!(low.memory_pressure());
        assert!(!ok.memory_pressure());
    }
}
