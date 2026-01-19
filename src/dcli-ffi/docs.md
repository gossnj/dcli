# Noridoc: dcli FFI Layer

Path: @/dcli/src/dcli-ffi

### Overview

Foreign Function Interface (FFI) wrapper that exposes dcli core library functionality to Swift/iOS applications via C-compatible ABI. Compiled as a static library (staticlib) for linking with Apple platform applications.

### How it fits into the larger codebase

This module bridges Rust dcli functionality to the Swift application at @/Last Banner. The Swift side at @/Last Banner/Services/DcliDatabaseService.swift calls these FFI functions through C interop. Build outputs (libdcli_ffi.a) are placed in @/RustLibraries as universal static libraries for macOS, iOS device, and iOS simulator architectures. The FFI layer depends on @/dcli/src/dcli for all actual functionality and only provides C-compatible wrappers.

### Core Implementation

**Library Configuration** (@/dcli/src/dcli-ffi/Cargo.toml): `crate-type = ["staticlib"]` produces .a files for static linking.

**Build Script**: @/dcli/src/dcli-ffi/build-apple.sh compiles for multiple Apple targets using rustup and lipo to create universal binaries.

**Single Source File** (@/dcli/src/dcli-ffi/src/lib.rs, 884 lines): All FFI functions in one file.

**Opaque Handle Pattern**:
- `DcliApiClient` struct wraps `ApiInterface` + tokio runtime
- `DcliActivityStore` struct wraps `ActivityStoreInterface` + `ManifestInterface` + tokio runtime
- Swift receives opaque pointers, calls `_new()` to create, `_free()` to destroy

**C-Compatible Structs**:
- `DcliCrucibleStats` (`#[repr(C)]`): 15 f32 fields matching crucible stats (kills, deaths, KD, efficiency, etc.)
- `DcliCharacter` (`#[repr(C)]`): id (i64), class_type (i32), minutes_played_total (i64)
- `DcliManifestInfo` (`#[repr(C)]`): version and URL pointers with lengths

**API Client Functions** (lines 73-261):
- `dcli_client_new(api_key: *const c_char) -> *mut DcliApiClient`: Creates API client
- `dcli_client_free(client: *mut DcliApiClient)`: Destroys client
- `dcli_search_player(client, bungie_name, out_member_id, out_platform) -> bool`: Player search
- `dcli_get_crucible_stats(client, member_id, character_id, platform, mode, out_stats) -> bool`: Fetch stats from API
- `dcli_get_characters(client, member_id, platform, out_characters, max_characters) -> i32`: Retrieve character list

**Activity Store Functions** (lines 263-617):
- `dcli_store_init(data_dir: *const c_char) -> *mut DcliActivityStore`: Initialize store with path
- `dcli_store_free(store: *mut DcliActivityStore)`: Destroy store
- `dcli_store_add_player(store, bungie_name) -> bool`: Add player to sync list
- `dcli_store_remove_player(store, bungie_name) -> bool`: Remove player from sync list
- `dcli_store_sync_player(store, bungie_name) -> bool`: Sync activities from API
- `dcli_store_sync_player_with_progress(store, bungie_name, callback, user_data) -> bool`: Sync with progress callbacks
- `dcli_store_get_crucible_stats(store, bungie_name, character_class, mode, out_stats) -> bool`: Query local database for stats

**Manifest Management Functions** (lines 619-864):
- `dcli_manifest_needs_update(data_dir, out_needs_update) -> bool`: Check if manifest needs update
- `dcli_manifest_download(data_dir, api_key) -> bool`: Download and install manifest from Bungie

**Helper Functions**:
- `dcli_string_free(s: *mut c_char)`: Free C strings allocated by FFI
- `dcli_get_last_error() -> *mut c_char`: Get last error (currently stub returning null)

**Progress Callback**: `ProgressCallback` type alias for C function pointer - `extern "C" fn(*const c_char, u32, u32, *mut c_void)` - allows Swift to receive progress updates during sync.

### Things to Know

**Tokio Runtime Management**: Each opaque handle owns its own tokio runtime. FFI functions use `runtime.block_on()` to synchronously execute async operations, blocking the calling thread until completion. This is necessary because C FFI cannot handle async/await.

**Memory Safety**: All pointer parameters validated for null before dereferencing. CString/CStr conversions wrapped in match statements to handle UTF-8 errors. Box::into_raw() and Box::from_raw() manage heap allocation for opaque handles.

**Error Handling**: FFI functions return bool for success/failure or i32 for counts. Rust Result/Option types converted to success booleans. No detailed error propagation to C side (last_error stub exists but unused).

**String Conversions**: Swift passes UTF-8 C strings (`*const c_char`). Rust converts via `CStr::from_ptr()` and validates `.to_str()`. Output strings allocated via `CString::new()` must be freed by caller via `dcli_string_free()`.

**Stats Calculation** in `dcli_store_get_crucible_stats()` (lines 521-606): Queries database for all-time stats (past 10 years using `DateTimePeriod`), converts `PlayerActivitiesSummary` to `DcliCrucibleStats`. Some fields like `average_kill_distance` and `suicides` unavailable in summary, set to 0.

**Platform Encoding**: Platform IDs passed as i32, converted via `Platform::from_id()`. Mode IDs passed as i32, validated via `Mode::from_id()` which returns Result.

**Character Class Selection** (lines 513-519): Maps integer to enum - 0=Titan, 1=Hunter, 2=Warlock, 3=LastActive, _=All.

**No Concurrent Access Safety**: FFI handles not thread-safe. Swift must ensure exclusive access or provide its own synchronization.

Created and maintained by Nori.
