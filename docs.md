# Noridoc: dcli Repository

Path: @/

### Overview

- dcli is a Rust workspace of Destiny 2 command-line tools, a shared data/API library, and a C ABI library for native clients.
- The CLI tools and native library use the same core API and SQLite data layer.
- Rust 1.83.0 and the checked-in lockfile define the supported build inputs.

### How it fits into the larger codebase

- The workspace in [src/Cargo.toml](src/Cargo.toml) connects the core library, FFI crate, output crate, and CLI applications.
- CLI applications call the shared library in [src/dcli](src/dcli); the sync tool populates activity data that query tools read.
- [src/dcli-ffi](src/dcli-ffi) wraps the same library for native consumers using the C declarations in [src/dcli-ffi/dcli.h](src/dcli-ffi/dcli.h).
- GitHub Actions validates workspace tests, builds, CLI behavior, native ABI contracts, and native initialization diagnostics before platform packaging; see [.github/workflows/dcli.yaml](.github/workflows/dcli.yaml).
- `DESTINY_API_KEY` is required when compiling because the key is embedded by the build. Use a real key for actual binaries; CI's validation-only value is isolated to checks.

### Core Implementation

- The core library separates Bungie API calls, SQLite activity storage, and manifest access. CLI tools and FFI wrappers sit above these shared services.
- The workspace and platform builds use Rust 1.83.0 with `--locked`, keeping dependency resolution tied to the checked-in lockfile.
- [tests/README.md](tests/README.md) describes the repository-root test commands. The suite covers Rust data and parser behavior, command-line smoke behavior, script guards, and C header/link contracts.
- The FFI crate builds source for Apple and Android targets through [src/dcli-ffi/build-apple.sh](src/dcli-ffi/build-apple.sh) and [src/dcli-ffi/build-android.sh](src/dcli-ffi/build-android.sh); these scripts create and copy platform artifacts.

### Things to Know

- Initial history sync records a durable activity-ID boundary for each player, character, and mode in [src/dcli/src/activitystoreinterface/sync.rs](src/dcli/src/activitystoreinterface/sync.rs). The boundary remains frozen through an interrupted scan, while the auxiliary scan tables retain discovered IDs, queued reports, unavailable reports, and versioned raw-page resume anchors without changing the activity schema.
- History discovery overlaps page requests within a bounded global window and commits each page's queue rows and history IDs transactionally before report downloads; when a checkpoint advances, its anchors are written in that same transaction. On resume, the scanner bridges from the current head and searches near saved anchors and page hints; if it cannot establish enough ordered overlap, it continues contiguous paging from the head prefix already scanned. The newest 50 become ready after every stream has supplied its first frontier and crossed the selected time cutoff, and readiness waits for those reports to commit. Full completion still requires discovery to finish with no pending or unavailable reports.
- [src/dcli/src/apiclient.rs](src/dcli/src/apiclient.rs) owns a shared request budget: at most 25 in-flight requests, one request start per 1/100 second, and four concurrent response bodies. It accepts identity or gzip responses, caps encoded transfer at 16 MiB and decoded body at 8 MiB, and applies retry/cooldown handling. Gzip support uses the locked dependencies in [src/dcli/Cargo.toml](src/dcli/Cargo.toml) and [src/Cargo.lock](src/Cargo.lock); the workspace remains pinned to Rust 1.83.0.
- [src/dcli-ffi/src/sync.rs](src/dcli-ffi/src/sync.rs) exposes additive version-1 coverage and cancellation contracts. A successful C return means an outcome was produced; callers inspect the full-history flag to distinguish complete coverage. The callback event is valid only during the callback, cancellation has independent lifetime rules, and callers must not re-enter the store from the callback.
- The Last Banner consumer has separate Swift-side publication and UI-readiness behavior. Building this repository alone does not update that consumer's source pin or bundled archives.
- Three checked-in archives under [src/dcli-ffi/build](src/dcli-ffi/build) are historical binaries. They do not export `dcli_store_init_with_diagnostics` or `dcli_store_close`; rebuild the library from current source before using those current header APIs.
- CI validation does not itself publish a release. Desktop release packaging runs only for an existing tag and requires `DESTINY_API_KEY`.


Created and maintained by Nori.
