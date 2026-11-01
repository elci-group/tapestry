# tapestry-desktop

The desktop coordination plane of the Tapestry programme: the `tapestry`
CLI, the local daemon-side loopback transport, scoped credentials, the
Playbook drive adapter, Mesut-shaped task lanes, the knowledge journal,
resource visibility, share tokens and temperature policy.

**Status: Gates 1–8 (desktop plane), POC.** The presentation endpoint is
simulated by a loopback device (`lounge-loopback`) using the real
tapestry-core TAP/1 frame types over a file transport, so no television
hardware is required to exercise the full human-inspection loop. Swapping
the loopback for a socket changes transport only, not semantics.

## The loop

```sh
tapestry "Generate four visual concepts for the Tapestry home screen"
```

runs: session admission → DreamSequence-shaped scoped credentials →
Playbook drive (observed via its journal) → Mesut lane classification →
deterministic artefact generation → TAP frame transfer with device-side
digest verification and acks → blocking human review → knowledge journal →
structured completion:

```text
✓ TAPESTRY COMPLETE

session     tap_01M6...
device      lounge-loopback
decision    approved
artifact    art_01M6...
latency     0.8s
```

Exit codes are stable: 0 ok, 2 usage, 3 auth, 4 no device, 5 review
rejected, 6 timeout.

## Commands

`tapestry "prompt"`, `send`, `show`, `compare`, `target`, `devices`,
`status`, `sessions`, `storage`, `share`, `doctor`, plus
`tapestry --request request.json --machine` for agents (JSON only, no
terminal scraping).

## Module map

| Module | Contents |
|---|---|
| `cli` | Command surface and stable exit codes |
| `slice` | The vertical slice orchestrating the full loop |
| `auth` | Scoped credentials; denied-by-default scopes; POC file vault |
| `playbook` | Drive adapter: subprocess, journal parsing, gates, cancellation |
| `mesut` | Workload classification and priority lanes with backpressure |
| `knowledge` | Digest-chained JSONL journal (Padagonia-shaped, Gate 5) |
| `resources` | Storage accounting and pressure signals (PV/Bart-shaped) |
| `transport` | Loopback TAP: frames, device-side validation, acks |
| `share` | Ephemeral single-use tokens + deterministic QR matrix |
| `temppolicy` | TempCheq-derived priors with logged, clamped adjustments |

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
deliver --spec deliver.toml
```

Version transitions are owned by kaptaind; do not bump `VERSION` manually.
