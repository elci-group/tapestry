//! End-to-end acceptance: `tapestry "prompt"` drives the complete loop —
//! auth → playbook drive → artefacts → loopback TAP transfer with digest
//! verification → human review → knowledge journal → structured completion.
//! Run against the built binary with an isolated TAPESTRY_DATA.

use std::io::Write;
use std::process::{Command, Stdio};

/// Locate the `tapestry` binary next to the test executable.
fn binary_path() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    while p.pop() {
        let candidate = p.join("tapestry");
        if candidate.exists() {
            return candidate;
        }
    }
    panic!("tapestry binary not found near test executable");
}

#[test]
fn full_loop_approve() {
    let bin = binary_path();
    let data = tempfile::tempdir().unwrap();
    let mut child = Command::new(&bin)
        .arg("Generate four visual concepts for the Tapestry home screen")
        .env("TAPESTRY_DATA", data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tapestry binary");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"APPROVE\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TAPESTRY COMPLETE"), "stdout: {stdout}");
    assert!(stdout.contains("decision    approved"));
    assert!(stdout.contains("session     tap_"));
    assert!(stdout.contains("device      lounge-loopback"));

    // Knowledge journal recorded session + artefacts + human decision.
    let journal = std::fs::read_to_string(data.path().join("knowledge/tapestry.jsonl")).unwrap();
    assert!(journal.contains("\"kind\":\"session\""));
    assert!(journal.contains("\"kind\":\"artefact\""));
    assert!(journal.contains("explicit_human_judgement"));

    // Playbook drive evidence exists.
    let drives = std::fs::read_dir(data.path().join("drives"))
        .unwrap()
        .count();
    assert!(drives >= 1);

    // Loopback transfer evidence: frames + acks.
    let transport_root = std::fs::read_dir(data.path().join("transport"))
        .unwrap()
        .count();
    assert!(transport_root >= 1);
}

#[test]
fn full_loop_reject_exits_nonzero() {
    let bin = binary_path();
    let data = tempfile::tempdir().unwrap();
    let mut child = Command::new(&bin)
        .arg("make it darker")
        .env("TAPESTRY_DATA", data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"REJECT\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "review rejection must exit 5"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("decision    rejected"));
}

#[test]
fn doctor_reports_healthy_after_run() {
    let bin = binary_path();
    let data = tempfile::tempdir().unwrap();
    // Prime the data dir with one run.
    let mut child = Command::new(&bin)
        .arg("prime")
        .env("TAPESTRY_DATA", data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"APPROVE\n")
        .unwrap();
    child.wait().unwrap();

    let out = Command::new(&bin)
        .arg("doctor")
        .env("TAPESTRY_DATA", data.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("vault"));
    assert!(stdout.contains("transport"));
    assert!(stdout.contains("knowledge"));
    assert!(stdout.contains("pressure"));
}
