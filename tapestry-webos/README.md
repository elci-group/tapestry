# Tapestry WebOS endpoint adapter

**State: host-testable MVP adapter. This is not yet an installable LG webOS TV application.**

This crate defines the Rust boundary a future webOS app can use to advertise
the shared Tapestry device identity and validate incoming TAP artefact chunks.
It uses `tapestry-core` wire types so the platform client cannot drift from the
desktop protocol.

## MVP behavior

- `WebOsEndpoint` accepts only a shared identity labeled `webos` and reports
  presentation capability from platform-probed display/input fields.
- `Transport` binds one session and device, validates TAP frame schema,
  payload digest, size, channel, sequence, artefact identity, and byte offset.
- Accepted chunk payloads return to the caller; this crate does not persist,
  render, discover peers, or create a network connection.
- The native host must establish an authenticated channel before calling
  `mark_connected()` and must obtain capabilities from webOS APIs rather than
  inferring them from model names.

## Build and gates

From the workspace root:

```sh
cargo check -p tapestry-webos
cargo test -p tapestry-webos
cargo clippy -p tapestry-webos --all-targets -- -D warnings
cargo fmt -p tapestry-webos -- --check
deliver --spec tapestry-webos/deliver.toml --base . --strict --allow-commands
```

The delivery gate covers host compilation and tests. It does not certify an
LG package or TV runtime. MVP product sign-off additionally requires a
packaged webOS build, pairing/authentication review, and a real device or
supported emulator presentation run with evidence attached to the release.

See [MVP_DIRECTIVE.md](MVP_DIRECTIVE.md) for the swarm policy, acceptance
criteria, delivery gates, and Playbook operating rules.
