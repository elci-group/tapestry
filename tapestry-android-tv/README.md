# Tapestry Android TV

**State: native Android TV local-review MVP source. APK build is blocked in
this environment because SDK platform 34 is missing. Desktop pairing and TAP
network transport are not implemented.**

This is the first native Android TV application surface for Tapestry. It
declares the Leanback launcher contract, runs without a touchscreen, and
provides a D-pad controlled local review demonstration. The demo deliberately
labels its sample artefact and decisions as local-only; it does not pretend to
receive desktop content or persist a decision.

## Requirements

- JDK 17 or newer
- The checked-in Gradle wrapper (Gradle 8.7)
- Android Gradle Plugin 8.5.2
- Android SDK platform 34 and matching build tools
- Android TV emulator or device for runtime sign-off

## Build and run

```sh
./gradlew :app:assembleDebug
./gradlew :app:installDebug
```

After building and installing on a compatible Android TV device or emulator,
open **Tapestry** from the launcher, choose **Start local review demo** with
the remote, then approve or reject the sample. The result is only shown on
that device.

## Current execution status

The host Deliver gate passes. The native Gradle build is blocked because
`/usr/lib/android-sdk` has no API 34 platform package; this environment also
has no verified TV/emulator runtime run. The source therefore remains an
unverified APK until those gates are available.

## Delivery gates

The host gate validates the decision state machine, manifest contract, and
wrapper availability without requiring an Android SDK:

```sh
deliver --spec deliver.toml --base . --strict --allow-commands
```

The native gate builds and lints the APK when API 34 is installed:

```sh
deliver --spec deliver.android.toml --base . --strict --allow-commands
```

See [MVP_DIRECTIVE.md](MVP_DIRECTIVE.md) for the Luna swarm plan, Playbook
policy, security boundaries, and the remaining connected-TV release gates.
