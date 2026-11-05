# Tapestry WebOS MVP: technical directive

**Owner:** WebOS integration lead  
**Release posture:** host-testable endpoint adapter; native TV release remains gated  
**Protocol authority:** `tapestry-core` is the single source of TAP wire contracts

## 1. Objective

Deliver a reviewable WebOS endpoint adapter that can be compiled and tested on
the host, reuses the shared Tapestry identity and frame schemas, and rejects
invalid or out-of-order artefact chunks before handing payloads to a host
renderer.

This MVP is an integration seam, not a TV app. Do not claim that it discovers
devices, opens a socket, pairs users, persists artefacts, renders a canvas,
handles remote control, or can be installed on an LG television. Those
capabilities require the native webOS runtime and separate release evidence.

## 2. MVP contract

### Device identity

- Reuse `tapestry_core::identity::DeviceIdentity`; never fork the wire type.
- Accept only the `webos` platform label (case-insensitive).
- Derive `can_present` from probed display and input capabilities. An unknown
  or unavailable capability stays absent; do not infer support from a model.
- The native host owns the capability probe and must include probe provenance
  in release evidence.

### TAP input boundary

- Accept only TAP artefact `chunk` frames for the bound session.
- Call shared `Frame::validate()` before processing a frame. That validates
  schema version, payload size, digest, and chunk offset rules.
- Enforce sequence numbers beginning at 1 and increasing by one, a stable
  artefact id for the transfer, and contiguous byte offsets.
- Return payload ownership to the host only after every check succeeds.
- Keep transport state explicit. `mark_connected()` records that the native
  host established its channel; it does not open a socket or authenticate a
  peer itself.
- A disconnect does not prove transfer completion. Until TAP carries a
  manifest/final marker, the host must not mark a multi-chunk artefact
  complete based solely on connection close.

### Enterprise safety boundary

- No unauthenticated LAN listener is in scope. Native networking must require
  an approved pairing and authentication design, TLS policy, origin policy,
  and replay protection before product release.
- Enforce frame/chunk size and integrity before allocation or rendering.
- Do not persist credentials in this crate. Any future credential storage
  must use the platform keystore and a threat-reviewed rotation/revocation
  plan.
- Treat all received payloads as untrusted. Rendering must be isolated from
  control-plane parsing and use an allowlist of supported media types.
- Keep logs free of tokens, credentials, and payload contents. Record
  correlation ids, sizes, outcomes, and failure classes instead.

## 3. Swarm operating model (Luna-oriented)

Use one accountable integrator and three independent Luna workstreams. The
integrator owns scope, code changes, merge decisions, and final evidence.

| Workstream | Luna responsibility | Evidence returned |
|---|---|---|
| Platform boundary | Check webOS runtime assumptions and distinguish host adapter from TV behavior | Exact platform APIs still missing; no inferred capabilities |
| Protocol/security | Review identity reuse, frame validation, sequencing, and transfer limits | Threats, rejected cases, and core-contract references |
| Delivery/release | Review tests, Deliver gate, Playbook trace, and release claims | Reproducible command results and unresolved gates |

### Coordination rules

- Delegate bounded, non-overlapping reviews first. The integrator assigns
  implementation ownership after the findings converge.
- Only one worker edits a given file at a time. Acquire a narrow Locksmith
  lease before shared mutations; heartbeat during long work and release it at
  handoff. Do not rely on Git state as a lock.
- Require each worker to distinguish observed repository evidence from
  proposals. A proposed behavior is not accepted until code and gate output
  support it.
- Run formatting, compilation, tests, and Deliver after integration. A green
  Playbook drive is orchestration evidence, not proof that this app works.
- Record known gaps in the handoff. Do not turn a missing runtime or hardware
  check into a claimed pass.

## 4. Acceptance gates

### Gate W0 — contracts

- WebOS crate compiles against the current workspace and shared core types.
- No duplicate WebOS-specific identity or protocol schema exists.
- Public APIs and errors are documented.

### Gate W1 — host adapter MVP

- Tests cover accepted presentable identity, wrong platform, and missing
  display behavior.
- Tests cover disconnected input rejection, valid chunk acceptance, corrupt
  digest rejection, session mismatch, sequence gaps, and offset continuity.
- `cargo check -p tapestry-webos` passes.
- `cargo test -p tapestry-webos` passes.
- `cargo clippy -p tapestry-webos --all-targets -- -D warnings` passes.
- `cargo fmt -p tapestry-webos -- --check` passes.
- `deliver --spec tapestry-webos/deliver.toml --base . --strict
  --allow-commands` passes.

### Gate W2 — native TV MVP release (not yet met)

- A signed, installable webOS package is produced from the supported SDK.
- Real platform APIs populate identity and capability fields.
- Pairing, authentication, TLS, replay, and revocation requirements are
  reviewed and implemented.
- A real TV or supported emulator shows a received artefact and reports
  review/input events through the shared protocol.
- Release evidence includes device/runtime versions, package digest, test
  results, and a Playbook journal identifier.

Do not label W1 as W2. A host crate build does not demonstrate TV installation,
rendering, networking, or security readiness.

## 5. Deliver policy

`tapestry-webos/deliver.toml` is the acceptance contract, not a post-hoc
summary. It must:

1. Require the directive, README, source modules, and relevant tests.
2. Reject unfinished placeholders in production source.
3. Run package-scoped format, clippy, compile, and test commands.
4. Be schema-validated before execution and run in strict mode with command
   execution explicitly enabled for the trusted local spec.
5. Avoid requiring a clean repository because other workspace work may be
   present; report unrelated working-tree changes separately.

Run from the workspace root:

```sh
deliver validate tapestry-webos/deliver.toml
deliver --spec tapestry-webos/deliver.toml --base . --strict --allow-commands
```

A failed command is a failed gate. Fix the cause and rerun the whole gate; do
not waive a failure by editing the expected output or suppressing warnings.
Hardware checks are separate evidence and cannot be simulated by Deliver.

## 6. Playbook usage policy

Playbook is the deterministic drive/audit layer for sprint execution. It is
not an implementation verifier and its generic acceptance drive is not a
WebOS acceptance test.

At sprint start:

```sh
playbook status --basic
playbook policy --basic
```

Run and retain an acceptance-drive journal as a CI artifact or local `/tmp`
file:

```sh
playbook mvp --journal /tmp/tapestry-webos-mvp-drive.jsonl --basic
playbook explain --basic
```

Operating policy:

- Use `status` to observe the active drive and `explain` to understand why a
  gate or play was accepted or rejected.
- Use `recover <journal>` only after an interrupted drive; reconcile uncertain
  host side effects before continuing.
- Use `sim` only to replay an existing trace under another policy. It
  reclassifies recorded events; it does not generate alternative actions.
- Treat priors as outcome-derived evidence, not as agent self-reports.
- Attach the journal path/hash to sprint evidence. Never substitute a
  completed Playbook drive for the Deliver report or W2 hardware proof.

## 7. MVP exit report

The release handoff must state:

- which WebOS gates passed and link the Deliver report;
- whether a native SDK package and TV/emulator run exist;
- which platform capabilities were actually probed;
- the Playbook journal location and final status;
- open security, persistence, rendering, and transfer-completion gaps.

If native packaging or hardware evidence is absent, report **host adapter MVP**
and keep the native TV release gate open.
