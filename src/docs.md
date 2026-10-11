# Noridoc: Rust Workspace

Path: @/src

### Overview

- This Cargo workspace builds the shared dcli library, native FFI library, console output support, and Destiny 2 CLI applications.
- All workspace crates use the root [Cargo.toml](Cargo.toml) and [Cargo.lock](Cargo.lock); the toolchain is pinned in [rust-toolchain.toml](rust-toolchain.toml).
- The API key is read during compilation from `DESTINY_API_KEY` and embedded in the resulting binaries.

### How it fits into the larger codebase

- The core crate in [dcli](dcli) owns Bungie API access, activity storage, manifest access, and shared types.
- [dclisync](dclisync) writes activity data through the core store; the reporting applications query that same local database.
- [dcli-ffi](dcli-ffi) adapts core operations to a C ABI for native application clients, while [tell](tell) supplies shared console output behavior.
- Workspace validation is run from the repository root with `cargo +1.83.0 test --locked --workspace --manifest-path src/Cargo.toml` and the corresponding `build` command. `DESTINY_API_KEY` must be set for both.

### Core Implementation

- Cargo workspace membership and shared dependency/build settings are declared in [Cargo.toml](Cargo.toml); checked-in versions are resolved by [Cargo.lock](Cargo.lock).
- The application crates parse CLI inputs and call shared services instead of owning independent API or persistence implementations.
- The FFI crate owns C-compatible handles and wrappers around async core operations. Its native declarations and artifact rebuild guidance are documented in [dcli-ffi/docs.md](dcli-ffi/docs.md).
- Rust unit tests cover shared data-layer behavior and CLI parsing; repository-level native and executable checks are described in [../tests/README.md](../tests/README.md).

### Things to Know

- Rust 1.83.0 is the workspace toolchain. Builds and tests use `--locked` to keep the dependency graph stable.
- CI uses a validation-only API key for isolated compile and test jobs. That value is not a substitute for a real key when building binaries for use.
- Apple and Android helper scripts create and copy native build outputs. They require a non-empty `DESTINY_API_KEY` before doing build work; see [dcli-ffi/build-apple.sh](dcli-ffi/build-apple.sh) and [dcli-ffi/build-android.sh](dcli-ffi/build-android.sh).

Created and maintained by Nori.
