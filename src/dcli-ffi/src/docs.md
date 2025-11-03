# Noridoc: dcli FFI Source

Path: @/dcli/src/dcli-ffi/src

### Overview

Contains the single-file implementation of C FFI bindings exposing Rust dcli functionality to Swift applications.

### How it fits into the larger codebase

This is the implementation layer called by @/Last Banner Swift code through C interop. The source here wraps @/dcli/src/dcli library calls, manages tokio runtimes for blocking FFI compatibility, and handles all C type conversions (pointers, structs, strings). Changes here directly affect the Swift API surface at @/Last Banner/Services/DcliDatabaseService.swift.

### Core Implementation

**Single File Architecture** (@/dcli/src/dcli-ffi/src/lib.rs): All 884 lines in one file, organized by functional area with MARK comments.

**Sections**:
1. **Imports and Type Definitions** (lines 1-35): Use statements, opaque handle structs, C-repr structs
2. **API Client Functions** (lines 36-261): Player search and stats retrieval from Bungie API
3. **Activity Store Functions** (lines 263-617): Database operations for syncing and querying activities
4. **Manifest Management** (lines 619-864): Manifest download and update checking
5. **Utility Functions** (lines 866-883): String memory management and error retrieval

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

**Pointer Safety Pattern**:
```rust
if client.is_null() || bungie_name.is_null() || out_stats.is_null() {
    return false;
}
```
Every FFI function validates all pointer parameters before use.

**String Handling**:
```rust
// Input: Swift C string -> Rust &str
let name_str = unsafe {
    match CStr::from_ptr(bungie_name).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    }
};

// Output: Rust String -> C string (caller must free)
let c_msg = CString::new(message)?;
cb(c_msg.as_ptr(), current, total, user_data);
```

### Things to Know

**No Fine-Grained Error Reporting**: Functions return bool/i32 for success. Internal Error details logged via eprintln! but not propagated to Swift. `dcli_get_last_error()` stub exists (line 880) but returns null.

**Progress Callback Threading**: Sync operations like `dcli_store_sync_player_with_progress()` call progress callback on same thread that invoked FFI. Swift must not block in callback.

**Database Stat Queries**: `dcli_store_get_crucible_stats()` hardcodes all-time period (lines 534-540) as 10 years in the past to now. Does not expose configurable time periods to Swift.

**Manifest Path Convention**: Functions expect `data_dir` containing `manifest.sqlite3` and `manifest_info.json`. Must match dcli library expectations.

**Runtime Creation Overhead**: Each `_init()` or `_new()` call creates a new tokio runtime. For many operations, this is acceptable; for high-frequency calls, consider pooling on Swift side.

**Unsafe Blocks**: Extensive use of `unsafe {}` for FFI pointer operations. Safety ensured by null checks and proper Box ownership transfer. All unsafe blocks are necessary for C interop.

**eprintln! Debugging**: Sync and manifest operations print status to stderr (lines 462-474, 754-859). Useful for debugging but not production logging.

Created and maintained by Nori.
