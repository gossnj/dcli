# Noridoc: dcli Source Root

Path: @/dcli/src

### Overview

The dcli workspace contains a collection of Rust utilities and libraries for interacting with the Destiny 2 API. This includes command-line tools for viewing player stats, a core library providing common functionality, and an FFI wrapper for integration with Swift/iOS applications.

### How it fits into the larger codebase

The dcli source tree is the backend of the Last Banner application. The @/dcli/src/dcli-ffi module provides C-compatible FFI bindings that bridge Rust functionality to the Swift/iOS frontend at @/Last Banner. The core @/dcli/src/dcli library handles all Destiny 2 API communication, data persistence via SQLite, and manifest management. Command-line tools (dclisync, dcliah, dclim, etc.) provide standalone functionality and serve as reference implementations of the library's capabilities.

### Core Implementation

This is a Cargo workspace with 8 workspace members defined in @/dcli/src/Cargo.toml:
- **dcli**: Core library providing API client, activity storage, manifest management, and data structures
- **dcli-ffi**: FFI wrapper exposing dcli functionality to Swift via C ABI (compiled as staticlib)
- **tell**: Output/logging library for console applications
- **dclisync**: CLI tool for syncing activity history from Bungie API to local SQLite database
- **dcliah**: CLI tool for displaying activity history and stats
- **dclim**: CLI tool for downloading and managing the Destiny 2 manifest database
- **dcliad**: CLI tool for displaying detailed activity/match information
- **dclia**, **dclistat**, **dclitime**: Additional CLI utilities

The workspace uses aggressive size optimization in release builds: `opt-level = 'z'`, `lto = true`, `codegen-units = 1`.

### Things to Know

**Compilation requires DESTINY_API_KEY environment variable** - The Bungie API key must be available at compile time.

**API Data Correction Logic**: The codebase includes sophisticated logic to fix inconsistencies in Bungie's API, particularly around Competitive mode detection starting in Season 25. This is implemented in @/dcli/src/dcli/src/activitystoreinterface.rs `fix_pgcr_data()` function which transforms incorrect mode IDs (e.g., ClashQuickplay → ClashCompetitive) based on `director_activity_hash` values.

**Three-tier architecture**: API layer (@/dcli/src/dcli/src/apiinterface.rs), persistence layer (@/dcli/src/dcli/src/activitystoreinterface.rs), and presentation layer (CLI tools or FFI).

**Platform targets**: The FFI layer is specifically designed for Apple platforms (iOS, macOS, visionOS) and is compiled as a universal static library via @/dcli/src/dcli-ffi/build-apple.sh.

Created and maintained by Nori.
