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
- Sync progress is forwarded through a C callback with message, current and total values, and caller data. The callback runs as part of the calling FFI operation.

### Things to Know

- Errors generally become boolean or count results and platform log messages; the legacy last-error function remains a stub. Initialization has its separate diagnostic JSON path.
- The C ABI implementation is present in source, but the checked-in static archives predate `dcli_store_init_with_diagnostics` and `dcli_store_close`. Rebuild from current source before linking callers that use either symbol.
- Rust 1.83.0, `--locked`, and a non-empty compile-time `DESTINY_API_KEY` are used by the platform build scripts. CI's validation-only key is confined to validation builds.

Created and maintained by Nori.
