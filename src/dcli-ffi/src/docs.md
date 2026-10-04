# Noridoc: dcli FFI Source

Path: @/dcli/src/dcli-ffi/src

### Overview

Contains the single-file implementation of C FFI bindings exposing Rust dcli functionality to Swift/Kotlin applications on iOS and Android.

### How it fits into the larger codebase

This is the implementation layer called by mobile apps through C interop. The source here wraps @/dcli/src/dcli library calls, manages tokio runtimes for blocking FFI compatibility, and handles all C type conversions (pointers, structs, strings). Changes here directly affect the Swift API surface at @/Last Banner/Services/DcliDatabaseService.swift. Platform-specific logging uses OSLog on Apple platforms and android_logger on Android.

### Core Implementation

**Single File Architecture** (@/dcli/src/dcli-ffi/src/lib.rs): ~988 lines in one file, organized by functional area with MARK comments.

**Sections**:
1. **Platform Logging**: Conditional compilation for iOS (OSLog), Android (android_logger), or no-op. Filters sqlx query spam to Warn level.
2. **API Client Functions**: Player search and stats retrieval from Bungie API
3. **Activity Store Functions**: Database operations for syncing and querying activities, including a diagnostic initializer and a consuming close call that waits for both SQLite connection workers
4. **Scoreboard Backfill Functions**: `dcli_store_backfill_scoreboard_values()` and `dcli_store_backfill_scoreboard_values_with_progress()` re-fetch PGCRs to populate scoreboard data for activities since Aug 2025 that were synced before scoreboardValues storage was implemented. Delegates to `ActivityStoreInterface::backfill_scoreboard_values()`. The no-progress variant delegates to the with-progress variant with a null callback.
5. **Manifest Management**: Manifest download and update checking with 2-minute timeout
6. **Utility Functions**: String memory management and error retrieval

**Memory Management Pattern**:
```rust
// Creation: Rust owns via Box, gives raw pointer to Swift
Box::into_raw(Box::new(DcliApiClient { client, runtime }))

// Destruction: Swift returns pointer, Rust takes ownership and drops
drop(Box::from_raw(client))
```

**Async-to-Sync Bridging**:
```rust
let result = runtime.block_on(async {
    store_ref.sync_player(&player_name).await
})
```
Each opaque handle struct contains its own `tokio::runtime::Runtime` to execute async dcli library calls synchronously.

**Progress Callback Type**:
```rust
pub type ProgressCallback = extern "C" fn(*const c_char, u32, u32, *mut std::ffi::c_void);
```
Receives formatted message, current count, total count, and user data pointer.

### Things to Know

**Granular Progress Reporting**: `dcli_store_sync_player_with_progress()` converts `SyncProgress` enum variants to human-readable messages for mobile UI:
- "Starting sync..." / "Fetching history for WARLOCK (2/3)" / "Downloading activities (150/500)" / "Saving activities (150/500)" / "Sync complete! 500 activities synced"
- Progress callback invoked on the same thread that called FFI; Swift/Kotlin must not block.

**General FFI Error Reporting**: Most functions return bool/i32 for success or counts. Error details are logged through the platform logger and are not propagated to callers. `dcli_get_last_error()` remains a stub returning null; store initialization has the separate diagnostic result described below.

**Store Initialization Diagnostics**: `dcli_store_init_with_diagnostics()` is an additive C entry point alongside `dcli_store_init()`. It returns the same opaque store handle and optionally writes a versioned JSON payload with success, source revision, the pre-initialization schema version when readable, `cleanup_verified`, and ordered initialization events. Events identify database role and stage, outcome, elapsed milliseconds, a coarse error category, and an optional SQLite code. The initializer explicitly closes any activity or manifest connection already acquired when another initialization step fails. A failed SQLx connect is conservatively reported with `cleanup_verified: false` because connection cleanup cannot be verified from that error result. If cleanup of an acquired connection fails, the same flag is false. The caller frees a returned JSON string with `dcli_string_free()`.

**Consuming Store Close**: `dcli_store_close()` takes ownership of a non-null store pointer, awaits closure of both the activity and manifest SQLx connections, and returns true only when both closes succeed. The pointer remains consumed when the result is false and cannot be reused. `dcli_store_free()` remains a legacy best-effort destructor that does not wait for SQLite workers; callers that need the close result use `dcli_store_close()`.

The embedded `source_revision` comes from `DCLI_BUILD_REVISION` at compile time when it contains a 40-character hexadecimal revision; otherwise it is `unknown`. This payload is limited to initialization and does not change the result contract of other FFI operations.

**Database Stat Queries**: `dcli_store_get_crucible_stats()` hardcodes all-time period as 10 years in the past to now. Does not expose configurable time periods.

**Manifest Path Convention**: Functions expect `data_dir` containing `manifest.sqlite3` and `manifest_info.json`. Must match dcli library expectations.

**Runtime Creation Overhead**: Each `_init()` or `_new()` call creates a new tokio runtime. For many operations, this is acceptable; for high-frequency calls, consider pooling on the app side.

**Unsafe Blocks**: Extensive use of `unsafe {}` for FFI pointer operations. Safety ensured by null checks and proper Box ownership transfer. All unsafe blocks are necessary for C interop.

Created and maintained by Nori.
