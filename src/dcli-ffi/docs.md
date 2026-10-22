# Noridoc: dcli FFI Layer

Path: @/src/dcli-ffi

### Overview

- This crate exposes selected dcli library operations through a C-compatible interface for native clients.
- The ABI is declared in the paired headers [dcli.h](dcli.h) and [build/dcli.h](build/dcli.h); the implementation is in [src/lib.rs](src/lib.rs).
- Apple and Android build scripts compile target libraries and copy them into consumer locations.

### How it fits into the larger codebase

- The FFI adapts the shared Rust services in [../dcli](../dcli); it does not implement a separate API or persistence layer.
- Native callers exchange opaque handles, C strings, primitive fields, and callbacks through the public header declarations.
- [build-apple.sh](build-apple.sh) and [build-android.sh](build-android.sh) build platform libraries with Rust 1.83.0 and locked dependencies. Run the relevant script from this directory with a real `DESTINY_API_KEY`; Apple builds also use Xcode tools and `lipo`, while Android builds use `cargo ndk` and the Android NDK.
- [../../tests/test_native_abi.py](../../tests/test_native_abi.py) checks C compilation and linking against both headers; [../../tests/native_init_diagnostics.py](../../tests/native_init_diagnostics.py) exercises the built shared library.
- The Last Banner app consumes its own Rust library build and source pin. Updating this repository does not refresh that app's checked-in library artifacts.

### Core Implementation

- Each opaque API client or activity store owns the Rust state and Tokio runtime needed to bridge synchronous C calls to async dcli operations.
- Store initialization can return a versioned JSON diagnostic report. The consuming close operation waits for activity and manifest connection workers; legacy free remains best-effort. See [src/lib.rs](src/lib.rs) and the ownership notes in [src/docs.md](src/docs.md).
- Progress callbacks pass a temporary message, current and total values, and caller data through the C ABI. Both public headers declare the callback type and progress-aware sync functions, which are implemented in Rust.
- The version-1 full and slice sync calls use the existing opaque sync-control token to expose a fixed error category through `dcli_sync_control_error_code` after the call returns. Each invocation clears the category first; the getter returns only a code, never raw error text. The version-1 result and event layouts remain unchanged. See [the sync implementation](src/sync.rs) for the mapping and [the source Noridoc](src/docs.md) for the category list and token lifetime.
- The tracked archives in [build](build) are historical and do not include the current sync error-code getter or other source-only additions; updating headers or Rust source does not rebuild them.

### Things to Know

- The `dcli.h` copies must remain aligned so native source and build consumers see the same declarations.
- The API key is a compile-time input, and the build scripts stop before invoking toolchains or creating output when it is unset or empty.
- A non-null store passed to the consuming close call is consumed even when closure reports failure; callers must not reuse the pointer.
- The Apple script builds macOS, iOS device, and iOS Simulator static libraries; its output is placed in this crate's `build` directory. VisionOS target entries are currently commented because the script notes they require nightly Rust.
- The Android script builds arm64-v8a, armeabi-v7a, x86_64, and x86 shared libraries and copies them to the consumer `jniLibs` directory configured by the script. These scripts create real artifacts, so use a real API key for consumer builds.

Created and maintained by Nori.
