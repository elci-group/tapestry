# Tapestry Android TV MVP: technical directive

**Accountable owner:** Android TV integration lead  
**Current product slice:** native Leanback launcher and D-pad local-review demonstration  
**Not yet delivered:** desktop pairing, TAP/1 receive, authenticated network transport, persistent decisions

**Execution status:** host checks pass; native APK build is blocked because
Android SDK platform 34 is absent; no emulator or TV smoke test is recorded.

## 1. Objective and honest release claim

Establish an installable native Android TV app shell with a usable remote-first
review flow. The current MVP lets a user launch the app from the TV home
screen, enter a local sample review, choose approve or reject with the D-pad,
and see a local confirmation.

This is an Android TV UI MVP, not a connected Tapestry product MVP. It contains
no network permission and exchanges no data with the desktop. Keep this
boundary visible in the app UI and release notes until the connected gates
below pass.

## 2. Product and platform contract

- Declare `android.software.leanback` as required and touchscreen as optional.
- Expose the main activity through `LEANBACK_LAUNCHER` and provide a TV banner.
- For Google Play Android TV submissions, target API 34 or higher under the
  requirement effective 2026-08-31. Recheck this policy at each release:
  [Google Play target API requirement](https://developer.android.com/google/play/requirements/target-sdk).
- Follow the platform launcher contract documented in [Create and run a TV
  app](https://developer.android.com/training/tv/get-started/create).
- Keep the Gradle wrapper version and published SHA-256 pinned. The current
  wrapper distribution checksum is verified against [Gradle's checksum
  reference](https://gradle.org/release-checksums/).
- Make every action reachable with DPAD Up/Down/Left/Right and Select; provide
  visible focus state through the platform theme.
- Do not request `INTERNET`, storage, microphone, or location permissions in
  this UI-only slice.
- Use the shared Tapestry design vocabulary, but do not claim core TAP/1
  compatibility before the Android receiver implementation exists.
- Keep the preview artefact synthetic and label it clearly as a placeholder.
  Local approve/reject decisions are ephemeral and never presented as desktop
  review records.

## 3. Luna swarm model

The integration lead owns scope, file ownership, code integration, gate
execution, and the final evidence report. Three independent Luna workstreams
review separate concerns:

| Workstream | Review boundary | Required finding |
|---|---|---|
| TV runtime | Launcher manifest, minimum SDK, remote navigation, emulator/device expectations | Platform claims supported by files or runtime evidence |
| TAP integration | Shared identity/frame contract, pairing, transport limits, replay defense | Exact compatibility gaps and threat cases |
| Delivery | Gradle wrapper, SDK gate, host tests, release evidence | Reproducible commands and blocked gates |

Coordination rules:

- Ask agents for bounded reviews before implementation; proposals are not
  evidence that behavior exists.
- Give one owner each mutable file. Acquire a narrow Locksmith lease for
  shared edits, heartbeat long work, and release at handoff.
- Integrate through one lead-owned patch and rerun all gates after any merge.
- Require reviewers to label each statement as observed, inferred, or
  proposed. Keep contradictions visible until resolved by code or evidence.
- Do not use parallelism to bypass build, security, or hardware acceptance.

## 4. Connected product gates

### A0 — native TV shell (current target)

- Leanback launcher entry and TV banner are present.
- Touchscreen is not required.
- D-pad navigates home → sample review → local result → home.
- Approve and reject cannot overwrite each other after a result is recorded.
- UI always identifies the sample and decision as local-only.
- Host state tests, manifest checks, and wrapper checks pass.
- Native APK assembles with pinned Gradle and Android SDK platform 34.

### A1 — connected endpoint (open)

- Use `tapestry-core` wire contracts through a reviewed Java/Kotlin binding or
  shared protocol module; do not hand-maintain divergent schemas.
- Add explicit discovery/pairing with short-lived credentials and user
  confirmation on the TV.
- Authenticate and encrypt the channel before accepting frames. No
  unauthenticated LAN listener is allowed.
- Validate TAP schema, frame length, digest, session, sequence, chunk offset,
  artifact id, and transfer completion before rendering.
- Add bounded storage, cancellation, reconnect behavior, and idempotent
  approval/rejection event persistence.
- Run interoperability tests against desktop using malformed, replayed,
  truncated, oversized, and out-of-order inputs.

### A2 — product release (open)

- Build/sign an installable APK from a controlled CI environment and attest
  the artifact digest.
- Run UI automation on an Android TV emulator and smoke-test on a supported
  physical device; retain runtime and platform versions.
- Complete security review for pairing, credential storage, transport,
  content rendering, logs, and update strategy.
- Demonstrate a real desktop-to-TV artifact review and durable decision return.
- Attach Deliver reports, test logs, APK digest, device evidence, and the
  Playbook journal identifier to the release record.

Never report A1/A2 as passed on the strength of the A0 local demo or a
successful Playbook drive.

## 5. Deliver gate policy

`deliver.toml` is the SDK-independent host gate. It verifies the required app
files, the local decision state machine, the Leanback manifest declarations,
and wrapper availability. `deliver.android.toml` is the native gate and must
run when the API 34 SDK is installed; it assembles and lints the APK.

Validate specs before use and execute trusted command checks in strict mode:

```sh
deliver validate deliver.toml
deliver --spec deliver.toml --base . --strict --allow-commands
deliver validate deliver.android.toml
deliver --spec deliver.android.toml --base . --strict --allow-commands
```

The host gate cannot substitute for the native gate. The native gate cannot
substitute for emulator/device testing. If the SDK is unavailable, report the
native gate as blocked with the missing platform package; do not weaken the
spec or call a source-only check an APK build.

## 6. Playbook usage policy

Use Playbook to record and inspect the deterministic engineering drive, not to
claim Android behavior:

```sh
playbook status --basic
playbook policy --basic
playbook mvp --journal /tmp/tapestry-android-tv-mvp-drive.jsonl --basic
playbook explain --basic
```

- `status` is the current tactical snapshot; `explain` gives the audit basis
  for accepted progress.
- Keep the journal as a CI artifact or local temporary record and reference
  its path/hash in the sprint report.
- Use `recover <journal>` only after interruption, then reconcile uncertain
  host side effects before continuing.
- `sim` reclassifies a recorded trace under another policy; it does not run a
  counterfactual implementation.
- Measured priors come from outcomes, never agent self-report.
- A completed generic drive does not prove that Gradle built an APK, that the
  app launches, or that a TV can communicate with desktop.

## 7. Sprint report

Report the A0/A1/A2 gate states, host test output, native build/lint output,
APK digest when available, emulator/device evidence, and Playbook journal
reference. State explicitly when connected transport or hardware validation
has not been demonstrated.
