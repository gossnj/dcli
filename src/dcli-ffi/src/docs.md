# Noridoc: dcli FFI Source

Path: @/dcli/src/dcli-ffi/src

### Overview

Contains the single-file implementation of C FFI bindings exposing Rust dcli functionality to Swift/Kotlin applications on iOS and Android.

### How it fits into the larger codebase

This is the implementation layer called by mobile apps through C interop. The source here wraps @/dcli/src/dcli library calls, manages tokio runtimes for blocking FFI compatibility, and handles all C type conversions (pointers, structs, strings). Changes here directly affect the Swift API surface at @/Last Banner/Services/DcliDatabaseService.swift. Platform-specific logging uses OSLog on Apple platforms and android_logger on Android.

### Core Implementation

**Single File Architecture** (@/dcli/src/dcli-ffi/src/lib.rs): ~988 lines in one file, organized by functional area with MARK comments.

**Sections**:
1. **Platform Logging** (lines 36-78): Conditional compilation for iOS (OSLog), Android (android_logger), or no-op. Filters sqlx query spam to Warn level.
2. **API Client Functions**: Player search and stats retrieval from Bungie API
3. **Activity Store Functions**: Database operations for syncing and querying activities
4. **Manifest Management**: Manifest download and update checking with 2-minute timeout
5. **Utility Functions**: String memory management and error retrieval

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

**No Fine-Grained Error Reporting**: Functions return bool/i32 for success. Error details logged via platform logger (info!/error! macros) but not propagated to caller. `dcli_get_last_error()` stub exists but returns null.

**Database Stat Queries**: `dcli_store_get_crucible_stats()` hardcodes all-time period as 10 years in the past to now. Does not expose configurable time periods.

**Manifest Path Convention**: Functions expect `data_dir` containing `manifest.sqlite3` and `manifest_info.json`. Must match dcli library expectations.

**Runtime Creation Overhead**: Each `_init()` or `_new()` call creates a new tokio runtime. For many operations, this is acceptable; for high-frequency calls, consider pooling on the app side.

**Unsafe Blocks**: Extensive use of `unsafe {}` for FFI pointer operations. Safety ensured by null checks and proper Box ownership transfer. All unsafe blocks are necessary for C interop.

Created and maintained by Nori.
