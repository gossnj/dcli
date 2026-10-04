# Noridoc: dcli Core Library Source

Path: @/dcli/src/dcli/src

### Overview

The source implementation of the dcli core library containing API communication, database persistence, data structures, and utility functions for Destiny 2 Crucible analytics. All Rust source files for the core library reside here.

### How it fits into the larger codebase

This directory contains the implementation of the dcli library declared in @/dcli/src/dcli/Cargo.toml. Files here are consumed by @/dcli/src/dcli-ffi for FFI exposure to Swift and by all CLI tools for direct library usage. The module structure defined in @/dcli/src/dcli/src/lib.rs determines what's publicly accessible to dependent crates.

### Core Implementation

**Primary Interface Files**:
- **apiinterface.rs** (700+ lines): High-level async API facade. Methods like `retrieve_alltime_crucible_stats()`, `search_destiny_player()`, `retrieve_activities_page()`, `retrieve_current_activity()`. Uses ApiClient internally.
- **activitystoreinterface.rs** (2600+ lines): SQLite interface managing activity database lifecycle. Key methods: `init_with_path()`, `sync_player()`, `sync_player_with_progress()`, `retrieve_activities_summary()`, `add_player_to_sync()`, `fix_pgcr_data()`.
- **manifestinterface.rs** (400+ lines): Queries manifest.sqlite3 for definitions. Caches frequently accessed items. Methods: `get_activity_definition()`, `get_inventory_item_definition()`, `get_stat_definition()`.

**Data Structure Files**:
- **crucible.rs**: Core types - `CrucibleActivity`, `Player`, `Member`, `PlayerName`, `Team`, `CrucibleStats`, `ExtendedCrucibleStats`, `WeaponStat`, `Medal`.
- **cruciblestats.rs**: Stats aggregation struct with calculated fields.
- **character.rs**: Character and player info structs.
- **statscontainer.rs**: Container for organizing stats by various dimensions.
- **playeractivitiessummary.rs**: Aggregated summary data from database queries.

**Subdirectories**:
- **enums/**: 11+ enum types providing type safety - `mode.rs` (600+ lines with all Crucible modes), `character.rs`, `platform.rs`, `moment.rs` (time period handling), `standing.rs`, `completionreason.rs`, `itemtype.rs`, `medaltier.rs`, `stat.rs`, `weaponsort.rs`.
- **response/**: API response structs - `pgcr.rs` (post-game carnage report), `activities.rs`, `stats.rs`, `character.rs`, `manifest.rs`, `gpr.rs` (get profile), and utility response types. All use serde for JSON deserialization.
- **manifest/**: Manifest definition structs - `definitions.rs` contains types like `ActivityDefinitionData`, `InventoryItemDefinitionData`, `HistoricalStatsDefinition`.

**Utility Files**:
- **utils.rs** (362 lines): Activity hash constants (competitive hashes lines 47-50), KD/efficiency calculations, date utilities (`get_last_weekly_reset()`, `human_date_format()`), string formatting.
- **apiclient.rs**: Thin reqwest wrapper. Injects API key from DESTINY_API_KEY env var or constructor parameter.
- **apiutils.rs**: URL constants and API helpers.
- **error.rs**: Error enum covering API, IO, parsing, and parameter errors.
- **emblem.rs**: Emblem-related utilities.
- **output.rs**: Output formatting enums.

### Things to Know

**Sync Architecture** (activitystoreinterface.rs):
- `SyncProgress` enum provides granular progress phases: `Starting`, `FetchingHistory`, `DownloadingActivities`, `SavingActivities`, `Complete`, `Failed`. Used by FFI layer for mobile app progress UI.
- `sync_player_with_progress<F>()` accepts a progress callback invoked at each sync phase. Wraps `sync_member_with_progress()` which iterates characters.
- Activity queue queries use `ORDER BY activity_id DESC` to prioritize recent games - ensures users see newest activities first if sync is interrupted.
- `insert_activities_batch()` wraps a PGCR batch in one SQLite transaction and each report in a savepoint. A failed report rolls back its own activity, related rows, upserts, and queue marker, while successful sibling reports can commit; transaction-control or commit errors roll back the batch and return an error.
- `PGCR_REQUEST_CHUNK_AMOUNT` controls concurrent API requests per batch.
- Initialization installs an idempotent partial index for pending queue rows ordered by character and descending activity ID, plus an activity index for team-result lookups. These are installed for fresh and existing databases without changing the schema version, whose mismatch path rebuilds the store.

**activitystoreinterface.rs Critical Logic**:
- `fix_pgcr_data()`: Transforms incorrect Competitive mode IDs when `director_activity_hash` matches known competitive values. Essential for Season 25+ data accuracy.
- The pending queue index supports the character-scoped, unsynced activity query used by both sync paths. The team-result index supports the activity-scoped lookup in `populate_activity_data()`.
- A report whose insertion fails during a batch remains pending because the savepoint restores its queue marker along with the report's database writes. Existing historical partial rows are not repaired; `has_activity()` continues to treat an existing activity row as already stored.
- Index creation errors are returned from initialization, leaving the existing data available for a later initialization retry.
- Database operations use sqlx with SQLite, all async.
- Progress reporting via indicatif ProgressBar for CLI, callback-based for FFI.

**Mode Enum** (enums/mode.rs): Contains 70+ mode variants covering all Crucible game types. Includes methods `is_crucible()`, `is_private()`, `from_id()`, `as_id()`. Modes can be compound (e.g., `AllPvP`, `AllPvPQuickplay`).

**Response Structs**: Tightly coupled to Bungie API JSON structure. Field names often match API exactly. Heavy use of `Option<T>` for nullable API fields. Custom deserializers in some cases.

**Hash Constants System** (utils.rs): Special activity hashes identify specific playlists that need special handling - Competitive, Checkmate variants, Iron Banner modes. These drive filtering and data correction logic throughout the codebase.

**Async Everywhere**: Nearly all public APIs are async functions returning `Result<T, Error>`. Callers must provide tokio runtime context.

Created and maintained by Nori.
