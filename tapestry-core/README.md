# tapestry-core

Shared contracts for the Tapestry programme: the versioned schemas and
protocol types every Tapestry client consumes. No client may independently
redefine core wire types.

**Status: Gate 0 (Foundations).** This crate is contracts only — no I/O, no
async runtime, no provider calls. Transport behaviour (framing on the wire,
crypto, reconnect) belongs to later gates and to the transport swarm.

## Module map

| Module | Contents |
|---|---|
| `ids` | Typed, time-sortable identifiers (`SessionId`, `ArtifactId`, `DeviceId`, `ReviewId`) |
| `identity` | `DeviceIdentity` and `CapabilitySet` advertised during discovery |
| `session` | `Session`, `SessionState`, legal state transitions, `SessionEvent` |
| `artifact` | The `Artifact` envelope and `ArtifactKind`; content-addressed digests |
| `review` | `ReviewDecision` and `ReviewRecord` — human judgement as first-class data |
| `telemetry` | Correlation hierarchy and timing-event names |
| `capability` | Codec descriptors and H.265/H.264 negotiation (probe-first, never assumed) |
| `protocol` | TAP/1: frame schema, channels, resumable chunks, capability negotiation |

## Design rules

- Every public type is a serde data contract; wire formats evolve only
  through a versioned, compatibility-checked change.
- Artefact identity is stable across transport: identity lives in the
  envelope, never in a transport address.
- Codec selection requires explicit capability probing; H.265 is never
  assumed from platform identity.
- Idempotency keys and monotonic sequence numbers are mandatory in the
  protocol types so repeated events cannot create duplicate effects.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
deliver --spec deliver.toml
```

Version transitions are owned by kaptaind; do not bump `VERSION` manually.
