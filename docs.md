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

- Seven CLI applications expose `--version`. `dclisync` accepts multiple values for sync, add, and remove operations; `dclistat` requires one or more `--stat` values.
- Three checked-in archives under [src/dcli-ffi/build](src/dcli-ffi/build) are historical binaries. They do not export `dcli_store_init_with_diagnostics` or `dcli_store_close`; rebuild the library from current source before using those current header APIs.
- The FFI header progress declarations describe existing Rust exports; this change did not add native ABI implementations.
- The Last Banner app maintains its own Rust library build and source pin. Changes in this repository do not update that consumer automatically.
- CI validation does not itself publish a release. Desktop release packaging runs only for an existing tag and requires `DESTINY_API_KEY`.

Created and maintained by Nori.
