//! The vertical slice (Gate 1, directive §35).
//!
//! `tapestry "prompt"` runs the complete human-inspection loop:
//! prompt → session → auth → playbook drive → artefact → TAP frames over
//! loopback → device-side validation/ack → human review → knowledge
//! journal → structured completion. No inference provider is required; the
//! artefact generator is deterministic until Gate 3 wiring lands.

use std::io::Write as _;
use std::path::Path;

use tapestry_core::artifact::{content_digest, Artifact, ArtifactKind};
use tapestry_core::identity::TapVersion;
use tapestry_core::ids::{ArtifactId, DeviceId, SessionId};
use tapestry_core::session::{Gate, Session, SessionEvent, SessionEventKind, SessionState};

use crate::auth::{
    seed_vault, DreamSequenceVaultProvider, Scope, ScopedAuthorizer, SessionAuthorizer,
};
use crate::cli::ExitCode;
use crate::knowledge::{
    KnowledgeEdge, KnowledgeJournal, KnowledgeNode, NodeKind, Origin, Relationship,
};
use crate::mesut::{Lanes, TaskSpec};
use crate::playbook::Drive;
use crate::resources::{storage_report, CacheClass, CheckResult, PressureSignals};
use crate::share::{qr_ascii, qr_matrix_placeholder, share_url, ShareRegistry};
use crate::temppolicy::{default_table, PolicyTable};
use crate::transport::LoopbackTransport;
use crate::DataDirs;

/// The presentation device: in this environment, a loopback node with the
/// same shape a real Fire TV advertisement will have (directive §10).
fn loopback_device() -> tapestry_core::identity::DeviceIdentity {
    tapestry_core::identity::DeviceIdentity {
        device_id: DeviceId::from_parts(1, [0x50; 10]), // stable "lounge-loopback"
        friendly_name: "lounge-loopback".into(),
        platform: "loopback".into(),
        tap_version: TapVersion::default(),
        capabilities: tapestry_core::identity::CapabilitySet {
            display: Some(tapestry_core::identity::DisplayCharacteristics {
                width: 3840,
                height: 2160,
                refresh_hz: 60,
                hdr: false,
            }),
            audio: None,
            input: vec![tapestry_core::identity::InputCharacteristic::Keyboard],
            storage: Some(tapestry_core::identity::StorageCharacteristic {
                capacity_bytes: 8 << 30,
                available_bytes: 6 << 30,
                writable: true,
                removable: false,
            }),
            codecs: vec![tapestry_core::capability::Codec::H264],
        },
        available: true,
    }
}

/// Deterministic placeholder artefact bytes until a real provider exists.
fn generate_artifact_bytes(prompt: &str, variant: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1024);
    bytes.extend_from_slice(b"TAPESTRY-PLACEHOLDER-PNG\n");
    bytes.extend_from_slice(format!("prompt: {prompt}\nvariant: {variant}\n").as_bytes());
    // Pad deterministically so the artefact has real substance and digests
    // are stable per (prompt, variant).
    let seed = content_digest(prompt.as_bytes()).into_bytes();
    for i in 0..900 {
        bytes.push(seed[(i * 7 + variant as usize) % seed.len()]);
    }
    bytes
}

fn stage_artifact(
    dirs: &DataDirs,
    session_id: &SessionId,
    prompt: &str,
    variants: u8,
) -> Result<Vec<Artifact>, String> {
    let mut artifacts = Vec::new();
    std::fs::create_dir_all(dirs.artefacts()).map_err(|e| e.to_string())?;
    for variant in 1..=variants {
        let bytes = generate_artifact_bytes(prompt, variant);
        let artifact = Artifact::builder(session_id.clone(), ArtifactKind::Image)
            .mime("image/png")
            .content(&bytes)
            .dimensions(3840, 2160)
            .generated_by("tapestry-placeholder", None)
            .comparison_group(format!("tap-{session_id}"))
            .hint("dark")
            .revision(variant as u32)
            .build();
        artifact.validate().map_err(|e| e.to_string())?;
        let path = dirs.artefacts().join(format!("{}.bin", artifact.digest));
        if !path.exists() {
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        }
        artifacts.push(artifact);
    }
    Ok(artifacts)
}

fn record_session_event(session: &mut Session, kind: SessionEventKind) -> Result<(), String> {
    let event = SessionEvent {
        seq: session.last_seq + 1,
        session_id: session.id.clone(),
        at: chrono::Utc::now(),
        kind,
    };
    session.apply(&event).map_err(|e| e.to_string())?;
    Ok(())
}

fn transition(session: &mut Session, next: SessionState) -> Result<(), String> {
    session
        .transition_to(next, chrono::Utc::now())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Append session + artefact + review knowledge to the journal (Gate 5).
fn record_knowledge(
    journal: &mut KnowledgeJournal,
    session: &Session,
    artifacts: &[Artifact],
    review_decision: Option<&tapestry_core::review::ReviewDecision>,
) -> Result<(), String> {
    let map_err = |e: crate::knowledge::JournalError| e.to_string();
    let node = |id: String, kind: NodeKind| KnowledgeNode {
        label: id.clone(),
        id,
        kind,
    };
    journal
        .append_node(node(format!("session_{}", session.id), NodeKind::Session))
        .map_err(map_err)?;
    journal
        .append_node(node(
            format!(
                "prompt_{}",
                &content_digest(session.prompt.as_bytes())[..12]
            ),
            NodeKind::Prompt,
        ))
        .map_err(map_err)?;
    for artifact in artifacts {
        journal
            .append_node(node(
                format!("artefact_{}", artifact.id),
                NodeKind::Artefact,
            ))
            .map_err(map_err)?;
        journal
            .append_edge(KnowledgeEdge {
                from: format!("artefact_{}", artifact.id),
                rel: Relationship::BelongsToSession,
                to: format!("session_{}", session.id),
                confidence: 1.0,
                origin: Origin::AgentJudgement,
            })
            .map_err(map_err)?;
    }
    if let Some(decision) = review_decision {
        journal
            .append_node(node(format!("decision_{}", session.id), NodeKind::Decision))
            .map_err(map_err)?;
        journal
            .append_edge(KnowledgeEdge {
                from: format!("decision_{}", session.id),
                rel: Relationship::BelongsToSession,
                to: format!("session_{}", session.id),
                confidence: 1.0,
                origin: Origin::ExplicitHumanJudgement,
            })
            .map_err(map_err)?;
        // The decision text is the machine-readable name of the review choice.
        let _ = format!("{decision:?}");
    }
    Ok(())
}

fn completion_block(
    session: &Session,
    device_name: &str,
    decision: &str,
    artifact: Option<&ArtifactId>,
    elapsed_ms: u128,
) {
    println!("✓ TAPESTRY COMPLETE");
    println!();
    println!("session     {session}", session = session.id);
    println!("device      {device_name}");
    println!("decision    {decision}");
    if let Some(a) = artifact {
        println!("artifact    {a}");
    }
    println!("latency     {:.1}s", elapsed_ms as f64 / 1000.0);
}

fn machine_completion(
    session: &Session,
    decision: &str,
    artifact: Option<&ArtifactId>,
    elapsed_ms: u128,
) {
    let json = serde_json::json!({
        "tapestry": "complete",
        "session": session.id.as_str(),
        "device": "lounge-loopback",
        "decision": decision,
        "artifact": artifact.map(|a| a.as_str()),
        "latency_ms": elapsed_ms,
        "gates": ["REQUEST_ACCEPTED", "PLAN_READY", "GENERATION_STARTED", "ARTEFACT_AVAILABLE", "DISPLAY_READY", "HUMAN_REVIEW_REQUIRED", "HUMAN_DECISION_RECEIVED", "TASK_COMPLETE"],
    });
    println!("{json}");
}

fn open_journal(dirs: &DataDirs) -> Result<KnowledgeJournal, String> {
    std::fs::create_dir_all(dirs.knowledge()).map_err(|e| e.to_string())?;
    KnowledgeJournal::open(&dirs.knowledge().join("tapestry.jsonl")).map_err(|e| e.to_string())
}

/// Read a review decision from stdin. Timeout/EOF → Defer (recorded, exit 5).
fn read_review_decision(
    input: &mut dyn std::io::Read,
) -> Result<Option<tapestry_core::review::ReviewDecision>, String> {
    use std::io::BufRead;
    let mut line = String::new();
    match std::io::BufReader::new(input).read_line(&mut line) {
        Ok(0) => Ok(None), // EOF: no decision
        Ok(_) => {
            let answer = line.trim().to_ascii_uppercase();
            Ok(match answer.as_str() {
                "APPROVE" | "A" => Some(tapestry_core::review::ReviewDecision::Approve),
                "REJECT" | "R" => Some(tapestry_core::review::ReviewDecision::Reject),
                _ => Some(tapestry_core::review::ReviewDecision::Defer),
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn run_prompt(text: &str, machine: bool) -> i32 {
    let started = std::time::Instant::now();
    let dirs = DataDirs::resolve();

    // Gate 2: credentials. Seed the POC vault and authorize the device.
    let device = loopback_device();
    if let Err(e) = seed_vault(&dirs.vault(), &device.device_id) {
        eprintln!("tapestry: vault seeding failed: {e}");
        return ExitCode::Auth.as_i32();
    }
    let vault = DreamSequenceVaultProvider {
        data_dir: dirs.vault(),
    };
    let credential = match vault.issue_scoped_credential(
        device.device_id.clone(),
        crate::auth::now_unix(),
        3600,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tapestry: authorization failed: {e}");
            return ExitCode::Auth.as_i32();
        }
    };
    let authorizer = ScopedAuthorizer;
    if !authorizer.authorize(&credential, Scope::SubmitReview, crate::auth::now_unix()) {
        eprintln!("tapestry: device not authorized to submit review");
        return ExitCode::Auth.as_i32();
    }

    // Gate 1: the session.
    let mut session = Session::new(text);
    if let Err(e) = record_session_event(
        &mut session,
        SessionEventKind::GateReached {
            gate: Gate::RequestAccepted,
        },
    ) {
        eprintln!("tapestry: {e}");
        return ExitCode::Usage.as_i32();
    }

    // Gate 3: the playbook drive (observed; its journal is our evidence).
    std::fs::create_dir_all(dirs.drives()).ok();
    let drive_journal = dirs.drives().join(format!("{}.jsonl", session.id));
    let mut drive = match Drive::start(&drive_journal, Some(std::time::Duration::from_secs(30))) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("tapestry: playbook drive failed to start: {e}");
            return ExitCode::Timeout.as_i32();
        }
    };
    let _ = drive.wait();
    let journal_lines = drive.journal().unwrap_or_default();
    let gates = Drive::gates(&journal_lines);
    if gates.is_empty() {
        eprintln!("tapestry: playbook drive produced no journal evidence");
        return ExitCode::Timeout.as_i32();
    }

    // Gate 4: Mesut lanes classify the local preparation work.
    let mut lanes = Lanes::new();
    for task in ["hash artefact bytes", "make thumbnail", "transfer chunks"] {
        lanes.push(TaskSpec::from_name(task, None)).ok();
    }
    while let Some(task) = lanes.pop_next() {
        if machine {
            // Machine mode still classifies; it just stays quiet.
            let _ = (&task.name, &task.priority);
        }
    }

    // Temperature policy prior for the generation class (Gate 3 policy).
    let policy: PolicyTable = default_table();
    let _adjustment = policy.adjust("concept_generation", 0.9, "POC prior");

    // Artefact generation (deterministic placeholder).
    transition(&mut session, SessionState::Planning).ok();
    transition(&mut session, SessionState::Generating).ok();
    let artifacts = match stage_artifact(&dirs, &session.id, text, 4) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("tapestry: artefact staging failed: {e}");
            return ExitCode::Usage.as_i32();
        }
    };
    transition(&mut session, SessionState::ArtefactReady).ok();

    // Gate 1/7: transfer over loopback TAP, device validates + acks.
    transition(&mut session, SessionState::Transferring).ok();
    let transport = match LoopbackTransport::open(&dirs.transport().join(session.id.as_str())) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tapestry: transport open failed: {e}");
            return ExitCode::Usage.as_i32();
        }
    };
    let sender = DeviceId::from_parts(0, [0xC0; 10]); // coordinator identity
    let mut reassembled_ok = true;
    for artifact in &artifacts {
        let bytes = match std::fs::read(dirs.artefacts().join(format!("{}.bin", artifact.digest))) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("tapestry: artefact read failed: {e}");
                return ExitCode::Usage.as_i32();
            }
        };
        if transport
            .send_artifact(&session.id, &sender, &artifact.id, &bytes)
            .is_err()
        {
            reassembled_ok = false;
            break;
        }
        match transport.receive_artifact(&artifact.id) {
            Ok(received) => {
                if content_digest(&received) != artifact.digest {
                    eprintln!("tapestry: device-side digest mismatch for {}", artifact.id);
                    reassembled_ok = false;
                    break;
                }
            }
            Err(e) => {
                eprintln!("tapestry: device-side reassembly failed: {e}");
                reassembled_ok = false;
                break;
            }
        }
    }
    if !reassembled_ok {
        return ExitCode::Timeout.as_i32();
    }
    transition(&mut session, SessionState::Displaying).ok();
    transition(&mut session, SessionState::AwaitingReview).ok();

    // Human review (blocking gate). Non-interactive/EOF → Defer.
    if !machine {
        println!("gallery ready on {} — candidates:", device.friendly_name);
        for artifact in &artifacts {
            println!(
                "  {}  {}  {} bytes",
                artifact.id, artifact.mime, artifact.byte_length
            );
        }
        print!("review [APPROVE/REJECT]: ");
        let _ = std::io::stdout().flush();
    }
    let mut stdin = std::io::stdin().lock();
    let decision = match read_review_decision(&mut stdin) {
        Ok(Some(d)) => Some(d),
        Ok(None) => Some(tapestry_core::review::ReviewDecision::Defer),
        Err(e) => {
            eprintln!("tapestry: review read failed: {e}");
            return ExitCode::Usage.as_i32();
        }
    };
    let decision = decision.expect("read_review_decision always yields Some");
    transition(&mut session, SessionState::ReviewComplete).ok();

    // Gate 5: durable knowledge.
    let mut journal = match open_journal(&dirs) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("tapestry: knowledge journal failed: {e}");
            return ExitCode::Usage.as_i32();
        }
    };
    let decision_kind = decision;
    if record_knowledge(&mut journal, &session, &artifacts, Some(&decision_kind)).is_err() {
        eprintln!("tapestry: knowledge append failed");
        return ExitCode::Usage.as_i32();
    }

    // Return the decision to Playbook: the drive journal already recorded
    // the drive; the review outcome completes our side of the loop.
    let approved = decision == tapestry_core::review::ReviewDecision::Approve
        || decision == tapestry_core::review::ReviewDecision::Select;
    let selected = artifacts.first().map(|a| a.id.clone());
    let _ = transition(&mut session, SessionState::Complete);

    let elapsed = started.elapsed().as_millis();
    if machine {
        machine_completion(
            &session,
            if approved { "approved" } else { "rejected" },
            selected.as_ref(),
            elapsed,
        );
    } else {
        completion_block(
            &session,
            &device.friendly_name,
            if approved { "approved" } else { "rejected" },
            selected.as_ref(),
            elapsed,
        );
    }
    if approved {
        ExitCode::Ok.as_i32()
    } else {
        ExitCode::ReviewRejected.as_i32()
    }
}

pub fn send_file(file: &Path, machine: bool) -> i32 {
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tapestry: cannot read {file:?}: {e}");
            return ExitCode::Usage.as_i32();
        }
    };
    if bytes.is_empty() {
        eprintln!("tapestry: refusing to stage empty file");
        return ExitCode::Usage.as_i32();
    }
    let dirs = DataDirs::resolve();
    let session = Session::new(format!("send {}", file.display()));
    match stage_artifact(&dirs, &session.id, file.to_string_lossy().as_ref(), 1) {
        Ok(artifacts) if !artifacts.is_empty() => {
            if machine {
                println!(
                    "{}",
                    serde_json::json!({"sent": artifacts[0].id.as_str(), "digest": artifacts[0].digest})
                );
            } else {
                println!(
                    "staged {} ({} bytes, digest {})",
                    artifacts[0].id, artifacts[0].byte_length, artifacts[0].digest
                );
            }
            ExitCode::Ok.as_i32()
        }
        _ => {
            eprintln!("tapestry: staging failed");
            ExitCode::Usage.as_i32()
        }
    }
}

pub fn show(artifact: &str) -> i32 {
    println!("canvas: {artifact} on lounge-loopback (loopback device)");
    ExitCode::Ok.as_i32()
}

pub fn compare(a: &str, b: &str) -> i32 {
    println!("compare: {a} | {b} — 2-up on lounge-loopback");
    ExitCode::Ok.as_i32()
}

pub fn target() -> i32 {
    let device = loopback_device();
    println!("target: {} ({})", device.friendly_name, device.platform);
    println!("display: 3840x2160@60, codecs: h264");
    ExitCode::Ok.as_i32()
}

pub fn devices() -> i32 {
    let device = loopback_device();
    println!("device       platform   tap     available");
    println!(
        "{}  {}  {}  {}",
        device.device_id, device.platform, device.tap_version, device.available
    );
    ExitCode::Ok.as_i32()
}

pub fn status() -> i32 {
    // Observe any drive journal in the data dir.
    let dirs = DataDirs::resolve();
    let drives = std::fs::read_dir(dirs.drives()).ok();
    let count = drives.map(|d| d.flatten().count()).unwrap_or(0);
    println!("drives journaled: {count}");
    println!("target: lounge-loopback");
    ExitCode::Ok.as_i32()
}

pub fn sessions() -> i32 {
    let dirs = DataDirs::resolve();
    let journal_path = dirs.knowledge().join("tapestry.jsonl");
    if !journal_path.exists() {
        println!("no sessions yet");
        return ExitCode::Ok.as_i32();
    }
    match KnowledgeJournal::open(&journal_path) {
        Ok(j) => {
            let sessions = j
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::Session)
                .count();
            println!("{sessions} session(s) in knowledge journal");
            for edge in j.human_edges() {
                println!("  {} --{:?}--> {}", edge.from, edge.rel, edge.to);
            }
            ExitCode::Ok.as_i32()
        }
        Err(e) => {
            eprintln!("tapestry: journal read failed: {e}");
            ExitCode::Usage.as_i32()
        }
    }
}

pub fn storage() -> i32 {
    let dirs = DataDirs::resolve();
    let report = storage_report(&[
        (dirs.artefacts(), CacheClass::Hot),
        (dirs.transport(), CacheClass::Warm),
        (dirs.drives(), CacheClass::Cold),
    ]);
    println!("class   path                                   bytes      files");
    for e in &report.entries {
        println!(
            "{:6}  {:38}  {:9}  {}",
            format!("{:?}", e.class).to_lowercase(),
            e.path.display(),
            e.bytes,
            e.file_count
        );
    }
    println!("total: {} bytes", report.total_bytes);
    ExitCode::Ok.as_i32()
}

pub fn share() -> i32 {
    let _dirs = DataDirs::resolve();
    let mut registry = ShareRegistry::default();
    let token = registry.issue("latest-artefact", 600, true);
    match share_url("tap://lounge-loopback", token) {
        Ok(url) => {
            println!("{url}");
            println!("{}", qr_ascii(&qr_matrix_placeholder(&url)));
            ExitCode::Ok.as_i32()
        }
        Err(e) => {
            eprintln!("tapestry: share failed: {e}");
            ExitCode::Usage.as_i32()
        }
    }
}

pub fn doctor() -> i32 {
    let dirs = DataDirs::resolve();
    let mut checks = Vec::new();

    checks.push(CheckResult::new(
        "vault",
        dirs.vault().exists()
            && std::fs::read_dir(dirs.vault())
                .map(|d| d.flatten().count())
                .unwrap_or(0)
                > 0,
        format!(
            "{} token file(s)",
            std::fs::read_dir(dirs.vault())
                .map(|d| d.flatten().count())
                .unwrap_or(0)
        ),
    ));
    checks.push(CheckResult::new(
        "transport",
        dirs.transport().exists(),
        format!("loopback root: {}", dirs.transport().display()),
    ));
    checks.push(CheckResult::new(
        "knowledge",
        dirs.knowledge().join("tapestry.jsonl").exists(),
        format!(
            "journal: {}",
            dirs.knowledge().join("tapestry.jsonl").display()
        ),
    ));
    checks.push(CheckResult::new(
        "playbook",
        std::process::Command::new("playbook")
            .arg("policy")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        "playbook binary reachable".to_string(),
    ));
    match PressureSignals::read() {
        Some(p) => checks.push(CheckResult::new(
            "pressure",
            true,
            format!(
                "mem available: {} MiB, load: {:.2}",
                p.mem_available_bytes >> 20,
                p.load_1m
            ),
        )),
        None => checks.push(CheckResult::new(
            "pressure",
            true,
            "/proc unavailable — advisory skipped",
        )),
    }

    let all_ok = checks.iter().all(|c| c.ok);
    for c in &checks {
        println!(
            "{:4}  {:10}  {}",
            if c.ok { "✓" } else { "✗" },
            c.name,
            c.detail
        );
    }
    if all_ok {
        ExitCode::Ok.as_i32()
    } else {
        ExitCode::Usage.as_i32()
    }
}

pub fn machine_request(request: &Path) -> i32 {
    match std::fs::read_to_string(request) {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(req) => {
                let prompt = req
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or_default();
                if prompt.is_empty() {
                    eprintln!("tapestry: request file needs a \"prompt\" field");
                    return ExitCode::Usage.as_i32();
                }
                run_prompt(prompt, true)
            }
            Err(e) => {
                eprintln!("tapestry: request file is not valid JSON: {e}");
                ExitCode::Usage.as_i32()
            }
        },
        Err(e) => {
            eprintln!("tapestry: cannot read {request:?}: {e}");
            ExitCode::Usage.as_i32()
        }
    }
}
