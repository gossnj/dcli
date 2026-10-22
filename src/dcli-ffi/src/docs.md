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
- The additive `dcli_store_sync_member_slice_v1` entry point accepts a stable member ID and membership platform, so a native caller can resume a member refresh without constructing a full sync row. Its versioned options carry a millisecond work budget; its result includes newly discovered IDs plus the same coverage snapshot used by sync events. Existing version-1 full-sync and coverage entry points retain their prior contract.
- `dcli_sync_control_error_code` reads the last engine failure category from the opaque sync-control token after a full or slice sync returns. Each invocation clears the token first; zero means no mapped failure. Codes are fixed: 1 unknown, 2 transport, 3 timeout, 4 throttled, 5 HTTP status, 6 API status, 7 privacy, 8 response parsing, 9 database, 10 filesystem, 11 invalid input, 12 player unavailable, 13 response too large, and 14 response encoding. The accessor exposes categories only, never error text.

### Things to Know

- Errors generally become boolean or count results and platform log messages; the legacy last-error function remains a stub. Version-1 sync failures additionally set the category on the supplied opaque control token, without changing version-1 result/event layouts. Initialization has its separate diagnostic JSON path.
- The version-1 coverage callback receives a borrowed event valid only for the duration of the callback. Copy it before asynchronous dispatch and never re-enter the owning store from it. The cancellation control may be signalled concurrently, but must remain alive until sync and all cancellation callers return; output storage and callback context remain alive through the sync call. A true return reports that an outcome exists, not that full history completed; callers inspect the completion flag.
- The version-1 API is additive alongside the earlier progress API. Public declarations live in [dcli.h](../dcli.h) and the generated copy under [build](../build); keep both aligned with the Rust exports in [sync.rs](sync.rs).
- The error-code getter is additive and stores state on the existing control token; it does not extend version-1 structs. Read it while the token remains alive and after its sync call returns. A null token reads as zero.
- Slice disposition values are `0` deferred, `1` checkpointed, `2` full complete, `3` cancelled, and `4` failed. The C boolean says whether a result is valid; callers must check disposition `2` for full coverage. A checkpoint can be useful because it committed PGCRs or inserted new history IDs, while re-seeing existing history is not new progress. A zero budget returns deferred without API work. Budget expiration is separate from explicit cancellation. Engine errors return false with disposition `4`; argument or version validation can return false before producing a result.
- The checked-in static archives are historical and do not include the current native sync error-code getter. Current source and headers do not rebuild or replace the separately pinned Last Banner archives.
- Rust 1.83.0, `--locked`, and a non-empty compile-time `DESTINY_API_KEY` are used by the platform build scripts. CI's validation-only key is confined to validation builds.

Created and maintained by Nori.
