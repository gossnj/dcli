# Noridoc: dcli FFI Source

Path: @/src/dcli-ffi/src

### Overview

- [lib.rs](lib.rs) implements the C ABI that adapts Rust dcli services for native callers.
- It manages opaque client and store handles, converts C values to Rust types, and runs asynchronous service calls through per-handle Tokio runtimes.
- Public declarations are duplicated in [../dcli.h](../dcli.h) and [../build/dcli.h](../build/dcli.h).

### How it fits into the larger codebase

- The wrappers call shared API, activity-store, and manifest services in [../../dcli](../../dcli), keeping native and CLI data behavior on the same implementation.
- C callers own the handle lifecycle and must free returned strings with the exported string-free function.
- The Swift consumer maintains a separately pinned Rust library and checked-in artifacts; source changes here require its own library rebuild to reach that app.
- Native contract and diagnostics checks under [../../../tests](../../../tests) load the built library and compile/link C against both header copies.

### Core Implementation

- Client and store constructors create opaque Rust-owned handles. Operations validate input pointers, convert strings and enum identifiers, and return C-compatible booleans, counts, structs, or allocated strings.
- The diagnostic store initializer returns a versioned JSON report for initialization events and cleanup status. The caller releases that buffer through the FFI string-free function.
- The consuming close operation takes ownership of a non-null store pointer and awaits both SQLx connection workers. Its result reports whether both connections closed; the pointer remains consumed if it returns false. The legacy free path does not wait for workers.
- The legacy progress callback remains available for existing callers. The additive version-1 sync API reports discovered, committed, pending, and unavailable counts with distinct discovery, commit, recent-ready, complete, cancelled, and incomplete phases. See [the core sync engine](../../dcli/src/activitystoreinterface/sync.rs) for the durable scan and readiness rules.

### Things to Know

- Errors generally become boolean or count results and platform log messages; the legacy last-error function remains a stub. Initialization has its separate diagnostic JSON path.
- The version-1 coverage callback receives a borrowed event valid only for the duration of the callback. Copy it before asynchronous dispatch and never re-enter the owning store from it. The cancellation control may be signalled concurrently, but must remain alive until sync and all cancellation callers return; output storage and callback context remain alive through the sync call. A true return reports that an outcome exists, not that full history completed; callers inspect the completion flag.
- The version-1 API is additive alongside the earlier progress API. Public declarations live in [dcli.h](../dcli.h) and the generated copy under [build](../build); keep both aligned with the Rust exports in [sync.rs](sync.rs).
- The checked-in static archives predate `dcli_store_init_with_diagnostics` and `dcli_store_close`. Rebuild from current source before linking callers that use either symbol; source changes here do not replace the separately pinned Last Banner archives.
- Rust 1.83.0, `--locked`, and a non-empty compile-time `DESTINY_API_KEY` are used by the platform build scripts. CI's validation-only key is confined to validation builds.

Created and maintained by Nori.
