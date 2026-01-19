# Noridoc: dcli Core Library

Path: @/dcli/src/dcli

### Overview

The dcli core library is the foundational module providing all shared functionality for interacting with the Destiny 2 API, managing local data storage, and processing Crucible statistics. It handles HTTP communication with Bungie's servers, SQLite-based activity storage, manifest database queries, and data structure definitions.

### How it fits into the larger codebase

This library is the central dependency for all dcli workspace members. CLI tools (@/dcli/src/dclisync, @/dcli/src/dcliah, etc.) use it directly for their operations. The FFI wrapper at @/dcli/src/dcli-ffi depends on this library and exposes selected functionality to Swift. The library abstracts all Destiny 2 API complexity, data persistence, and business logic away from consumers. It depends only on @/dcli/src/tell for console output formatting.

### Core Implementation

**Entry Point**: @/dcli/src/dcli/src/lib.rs declares all public modules.

**Key Modules**:
- **apiinterface.rs**: `ApiInterface` struct provides high-level async API methods like `retrieve_alltime_crucible_stats()`, `retrieve_characters()`, `retrieve_activities_page()`. Wraps lower-level `ApiClient`.
- **activitystoreinterface.rs**: `ActivityStoreInterface` manages the SQLite activity database. Handles syncing activities from API, computing aggregated stats, and querying historical data. Contains `fix_pgcr_data()` which corrects Bungie API inconsistencies for Competitive modes.
- **manifestinterface.rs**: `ManifestInterface` queries the Destiny 2 manifest SQLite database for weapon definitions, activity names, medal info, etc. Uses hash-to-ID conversion for manifest lookups.
- **apiclient.rs**: Low-level HTTP client wrapper around reqwest. Handles API key injection and response parsing.
- **crucible.rs**: Core data structures like `CrucibleActivity`, `Player`, `Member`, `PlayerName`, `CrucibleStats`.
- **enums/**: Type-safe enums for `Mode`, `Platform`, `CharacterClass`, `Standing`, `DateTimePeriod`, etc.
- **response/**: Serde-deserializable structs matching Bungie API JSON responses (PGCR, activities, stats, character data).
- **utils.rs**: Utility functions for KD ratio calculations, date/time handling, activity hash constants, error formatting.

**Database Schema**: @/dcli/src/dcli/actitvity_store_schema.sql defines tables: `member`, `character`, `activity`, `character_activity_stats`, `weapon_result`, `medal_result`, `modes`, `team_result`, `activity_queue`, `sync`.

**Data Flow**:
1. API requests via ApiInterface → ApiClient → Bungie servers
2. Responses deserialized into response structs
3. ActivityStoreInterface transforms and persists to SQLite
4. Queries aggregate data from SQLite for statistics

### Things to Know

**Competitive Mode Hash Constants** (@/dcli/src/dcli/src/utils.rs lines 47-50):
- `COMPETITIVE_PVP_ACTIVITY_HASH = 2754695317` (pre-Season 25)
- `FREELANCE_COMPETITIVE_PVP_ACTIVITY_HASH = 2607135461`
- `COMPETITIVE_PVP_ACTIVITY_HASH_S25 = 814159553` (Season 25+)

**PGCR Data Correction**: Starting Season 25, Bungie's API returns correct `director_activity_hash` but wrong mode IDs for Competitive. The `fix_pgcr_data()` function (activitystoreinterface.rs:951-1050) detects competitive hashes and transforms modes: `ClashQuickplay(71) → ClashCompetitive(72)`, `ZoneControl(89) → CollisionCompetitive(704)`.

**Async Runtime**: All API and database operations are async using tokio. CLI tools create tokio runtimes; FFI layer creates its own runtime per operation.

**DCLI_FIX_DATA Environment Variable**: When set to `TRUE`, attempts to re-fetch corrupt data from Bungie API. Significantly slows initial sync but improves data quality for applications building datastores.

**Database Schema Version**: Current version is 10 (DB_SCHEMA_VERSION constant). Schema upgrades handled in activitystoreinterface.rs.

**Manifest Hash Conversion**: Bungie API uses unsigned 32-bit hashes; manifest database uses signed 64-bit IDs. Conversion function in manifestinterface.rs:44-52.

Created and maintained by Nori.
