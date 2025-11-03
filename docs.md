# Noridoc: dcli Project Root

Path: @/dcli

### Overview

The dcli (Destiny Command Line Interface) project is a collection of Rust-based tools and libraries for interacting with the Destiny 2 API. It provides command-line utilities for viewing player statistics, a core library for API communication and data persistence, and FFI bindings for integration with native applications like the Last Banner iOS/macOS app.

### How it fits into the larger codebase

This is a Git submodule within the Last Banner project located at @/Last Banner/dcli. The compiled FFI static library from @/dcli/src/dcli-ffi is linked into the Swift application at @/Last Banner. The Swift code at @/Last Banner/Services/DcliDatabaseService.swift calls into the Rust FFI layer to sync activities, query stats, and manage the manifest. Database files created by dcli are stored in the iOS app's application support directory and queried by the Swift app through the FFI interface.

### Core Implementation

**Project Structure**:
```
dcli/
├── src/                          # Rust workspace (@/dcli/src)
│   ├── dcli/                     # Core library
│   ├── dcli-ffi/                 # FFI bindings for Swift
│   ├── tell/                     # Console output library
│   └── [CLI tools]/              # Command-line applications
├── examples/                      # Usage examples and scripts
├── tests/                         # Integration tests
├── service/                       # Service/daemon configurations
└── README.md                      # Project documentation
```

**Architecture**:

```
┌─────────────────────────────────────────────┐
│  Last Banner Swift App (@/Last Banner)      │
│  ┌─────────────────────────────────────┐    │
│  │ DcliDatabaseService.swift           │    │
│  │ - Calls FFI functions               │    │
│  │ - Manages local database            │    │
│  └──────────────┬──────────────────────┘    │
└─────────────────┼──────────────────────────-┘
                  │ C ABI
                  ↓
┌─────────────────────────────────────────────┐
│  dcli-ffi (@/dcli/src/dcli-ffi)             │
│  ┌─────────────────────────────────────┐    │
│  │ lib.rs (C-compatible wrappers)      │    │
│  │ - dcli_store_sync_player()          │    │
│  │ - dcli_store_get_crucible_stats()   │    │
│  │ - dcli_manifest_download()          │    │
│  └──────────────┬──────────────────────┘    │
└─────────────────┼──────────────────────────-┘
                  │ Rust
                  ↓
┌─────────────────────────────────────────────┐
│  dcli Core Library (@/dcli/src/dcli)        │
│  ┌──────────────────┐  ┌─────────────────┐  │
│  │ ApiInterface     │  │ ActivityStore   │  │
│  │ - HTTP Client    │  │ - SQLite DB     │  │
│  │ - API Calls      │  │ - Sync Logic    │  │
│  └────────┬─────────┘  └────────┬────────┘  │
│           │                     │            │
│           ↓                     ↓            │
│  ┌──────────────────────────────────────┐   │
│  │ ManifestInterface                    │   │
│  │ - Manifest SQLite queries            │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
                  │
                  ↓
           Bungie.net API
```

**Key Components**:
1. **Core Library** (@/dcli/src/dcli): API communication, SQLite persistence, manifest queries, data structures
2. **FFI Layer** (@/dcli/src/dcli-ffi): C ABI exports for Swift integration
3. **CLI Tools**: Standalone utilities for syncing, querying, and managing data
4. **Tell Library** (@/dcli/src/tell): Console output formatting

**Data Flow**:
1. Bungie API → ApiInterface → JSON responses
2. Responses → ActivityStoreInterface → SQLite database (dcli.sqlite3)
3. Database ← Query methods ← CLI tools or FFI layer
4. Manifest definitions ← ManifestInterface ← Manifest database (manifest.sqlite3)

**Databases**:
- **dcli.sqlite3**: Activity history and stats (schema in @/dcli/src/dcli/actitvity_store_schema.sql)
- **manifest.sqlite3**: Game definitions provided by Bungie (weapons, activities, medals)

### Things to Know

**Competitive Mode Filtering Bug Fix**: Critical dual-layer fix for Season 25 API inconsistencies:
- **Rust Layer** (@/dcli/src/dcli/src/activitystoreinterface.rs:963-1004): Transforms incorrect mode IDs at sync time
- **Swift Layer** (@/Last Banner/Services/DcliDatabaseService.swift:82-84, 130-160): Adds hash-based filtering in queries
- **Problem**: Season 25+ returns correct `director_activity_hash` (814159553) but wrong mode IDs
- **Solution**: Fix modes during sync AND query by hash to catch any missed transformations

**API Key Management**: Bungie API key must be set as `DESTINY_API_KEY` environment variable at Rust compile time. Key is embedded in compiled binaries. Swift app must also have key for direct API calls not going through FFI.

**Database Schema Version**: Current version 10. Schema defined in @/dcli/src/dcli/actitvity_store_schema.sql. Version stored in database for migration detection.

**Platform Builds**: FFI library compiled for multiple Apple architectures via @/dcli/src/dcli-ffi/build-apple.sh:
- macOS: x86_64-apple-darwin, aarch64-apple-darwin
- iOS: aarch64-apple-ios
- iOS Simulator: x86_64-apple-ios, aarch64-apple-ios-sim

Output in @/RustLibraries as universal static libraries.

**Privacy Requirements**: Players must enable "Show my Destiny game Activity feed on Bungie.net" in privacy settings at bungie.net/7/en/User/Account/Privacy for activity syncing to work.

**Activity Hash Constants** (@/dcli/src/dcli/src/utils.rs:47-63): Special playlist hashes drive filtering and data correction:
- Competitive: 2754695317, 2607135461, 814159553
- Checkmate variants: Multiple hashes for Clash, Control, Countdown, Rumble, Survival
- Iron Banner: 2955009825 (Tribute), 2888503916 (Fortress)

**Async/Tokio Runtime**: All API and database operations async. FFI layer creates blocking interface via `runtime.block_on()` for C compatibility.

**DCLI_FIX_DATA Environment Variable**: Setting to TRUE enables aggressive data correction, re-fetching corrupt API responses. Significantly slows sync but improves data quality.

Created and maintained by Nori.
