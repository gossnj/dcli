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

**Database Schema**: @/dcli/src/dcli/actitvity_store_schema.sql defines the activity, character, statistics, result, queue, and sync tables. `ActivityStoreInterface::init_with_path()` also installs an idempotent partial index on pending queue rows, ordered by character and descending activity ID, and an index on `team_result(activity)` for activity-scoped team lookups. It does so for both fresh and existing stores while leaving schema version 10 unchanged; version mismatch still invokes the existing schema rebuild path.

**Data Flow**:
1. API requests via ApiInterface → ApiClient → Bungie servers
2. Responses deserialized into response structs
3. ActivityStoreInterface transforms and persists to SQLite
4. Queries aggregate data from SQLite for statistics

During sync, pending activity IDs are selected by character with newest IDs first. PGCR results are inserted in a batch transaction, with a savepoint around each report. A report-level write failure rolls back that report's writes and leaves its queue entry pending for retry while successful sibling reports can commit. Batch control or commit failures roll back the transaction and return an error; rollback failures include the original error context.

### Things to Know

**Competitive Mode Hash Constants** (@/dcli/src/dcli/src/utils.rs lines 47-50):
- `COMPETITIVE_PVP_ACTIVITY_HASH = 2754695317` (pre-Season 25)
- `FREELANCE_COMPETITIVE_PVP_ACTIVITY_HASH = 2607135461`
- `COMPETITIVE_PVP_ACTIVITY_HASH_S25 = 814159553` (Season 25+)

**PGCR Data Correction**: Starting Season 25, Bungie's API returns correct `director_activity_hash` but wrong mode IDs for Competitive. The `fix_pgcr_data()` function (activitystoreinterface.rs:951-1050) detects competitive hashes and transforms modes: `ClashQuickplay(71) → ClashCompetitive(72)`, `ZoneControl(89) → CollisionCompetitive(704)`.

**Async Runtime**: All API and database operations are async using tokio. CLI tools create tokio runtimes; FFI layer creates its own runtime per operation.

**DCLI_FIX_DATA Environment Variable**: When set to `TRUE`, attempts to re-fetch corrupt data from Bungie API. Significantly slows initial sync but improves data quality for applications building datastores.

**Database Schema Version**: Current version is 10 (DB_SCHEMA_VERSION constant). Schema upgrades handled in activitystoreinterface.rs.

**Additive Index Initialization**: The pending queue and team-result indexes are ensured after any schema initialization on every store open. An index creation failure surfaces as an initialization error without rebuilding the database, so opening the store can be retried after the cause is removed. This batch behavior does not repair partial activity rows created before the savepoint handling existed; an existing activity row remains the completion check used by sync.

**Manifest Hash Conversion**: Bungie API uses unsigned 32-bit hashes; manifest database uses signed 64-bit IDs. Conversion function in manifestinterface.rs:44-52.

Created and maintained by Nori.
