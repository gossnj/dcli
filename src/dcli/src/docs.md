# Noridoc: dcli Core Library Source

Path: @/src/dcli/src

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

**Initial History Sync** ([activitystoreinterface/sync.rs](activitystoreinterface/sync.rs), [apiclient.rs](apiclient.rs)):
- Each member/character/mode stream freezes an activity-ID boundary from the highest ID already stored. That boundary is independent of the raw resume position: an interrupted scan keeps it and admits returned entries from the current head until it encounters that boundary. Arrivals after the head page has been read wait for a later scan with a fresh boundary.
- Resume uses the auxiliary version-1 `sync_history_resume` metadata: epoch, frozen boundary, the first eight raw IDs from the current scan's first page, the last eight raw IDs from its last committed page, and a page hint. The optimization assumes scanned history is mostly stable; it cannot detect unseen deep backfills, gives no snapshot guarantee, and does not require a full verification pass.
- A resumed stream bridges up to eight pages from the current head, then probes around the saved hint adjusted by the detected head shift, alternating earlier and later within a radius of eight pages. A landing requires at least three anchors in order, including an adjacent anchor pair. Missing or invalid checkpoint metadata is discarded for contiguous scanning from the head; when valid metadata cannot establish a landing, discovery continues contiguously from the head prefix already scanned. Failed probe requests propagate their error while retaining the saved checkpoint for retry; missing or invalid overlap still uses fallback. The probe cache holds at most three decoded pages; ordinary transient work is bounded to four outstanding or buffered pages globally.
- Each page's discovered IDs and queue rows are inserted in batches of at most 200 rows in one transaction. A checkpoint is written only with the page commit, so failed admission does not advance either; an interrupted head bridge keeps the deeper saved checkpoint, and a completed stream clears it. `INSERT OR IGNORE` makes replay idempotent, newly-discovered counts use inserted-row counts, and queue coverage recounts are skipped only when page admission proves queue membership unchanged.
- Before pending reports are processed, lifecycle reconciliation marks queue entries complete when their activity rows already exist. Discovery and report download remain separate: each member/character/mode stream pages Crucible and private-match history, and the recent window waits until every stream supplies an initial frontier before choosing its newest 50. Recent readiness is emitted only after each unfinished stream crosses the time cutoff and the selected report writes commit.
- The global page window includes both outstanding requests and decoded buffered pages. After recent readiness, at most two pages per stream may be in flight during remaining discovery. Full completion additionally requires all streams to reach their frozen boundary or API end and no pending or unavailable reports; only full completion updates the member sync timestamp.
- Bounded member slices reconcile and process existing pending work before profile/history enumeration, using queue reads of at most 256 rows per pass. Their deadline also bounds discovery and downloads; expiration is deferred/checkpointed work, while explicit cancellation remains separately reported. `sync_scan`, `sync_history`, and `sync_unavailable` are installed during store initialization; only `sync_history_resume` is created lazily. These auxiliary tables leave the version-10 activity schema and existing full-sync progress callback contract unchanged.
- `insert_activities_batch()` preserves the parent/child report transaction boundary: each report uses a savepoint inside the batch transaction, so one report failure can remain queued while later reports commit.
- The shared client starts requests at a 100-per-second production cadence, admits at most 25 in flight, and buffers at most four response bodies. It accepts identity or gzip encoding, caps encoded transfer at 16 MiB and decoded bodies at 8 MiB, and applies retry and server-cooldown handling to transient or throttled responses. See [the bounded HTTP client](apiclient.rs).
- Gzip support is provided by the locked `reqwest` and `async-compression` dependencies in [Cargo.toml](../Cargo.toml) and [Cargo.lock](../../Cargo.lock), under the workspace's Rust 1.83.0 toolchain.

**Scoreboard Data Persistence** (activitystoreinterface.rs):
- `scoreboard_result` junction table stores scoreboardValues from Bungie PGCRs (scoreboard scores, reward scores, multipliers) linked to `character_activity_stats` via foreign key.
- Table created via **lazy migration** in `init_with_path()`: checks `sqlite_master` for existence and creates if missing, avoiding a schema version bump (which would drop all tables and require full re-sync).
- New activities: scoreboardValues inserted inline during `_insert_character_activity_stats()` alongside existing medal_result and weapon_result bulk inserts.
- `backfill_scoreboard_values<F>()`: re-fetches PGCRs for activities since `SCOREBOARD_VALUES_START_DATE` (2025-08-19) that have no scoreboard_result rows. Processes in PGCR_REQUEST_CHUNK_AMOUNT-sized concurrent batches. Uses `INSERT OR IGNORE` to handle duplicates safely. Idempotent -- becomes a no-op when all activities are caught up.
- Scoreboard data sourced from `extended.scoreboard_values` on the PGCR response (`HashMap<String, DestinyHistoricalStatsValue>` in @/dcli/src/dcli/src/response/pgcr.rs).

**activitystoreinterface.rs Critical Logic**:
- `fix_pgcr_data()`: Transforms incorrect Competitive mode IDs when `director_activity_hash` matches known competitive values. Essential for Season 25+ data accuracy.
- Database operations use sqlx with SQLite, all async.
- Progress reporting via indicatif ProgressBar for CLI, callback-based for FFI.

**Database Initialization and Indexes** (`activitystoreinterface.rs`, `actitvity_store_schema.sql`): `init_with_path()` applies the schema when the stored version is not current, then ensures the scoreboard table and two lookup indexes exist. Scoreboard creation remains a lazy migration to avoid a version bump and full history rebuild. `activity_queue_pending_character_index` covers unsynced queue rows by character and descending activity ID; `team_result_activity_index` covers result rows by activity. `IF NOT EXISTS` makes index setup repeatable for fresh and existing stores, and SQLite setup errors are returned from initialization.

**Initialization Diagnostics** (`initializationdiagnostics.rs`, `activitystoreinterface.rs`): The FFI store initializer can pass a per-call recorder through store setup. It records ordered activity-database and manifest initialization stages with elapsed time and coarse SQLx error categories/codes, including schema read and application, lookup-index setup, API-client construction, and manifest existence checks. The schema version is captured before schema setup when available. These events describe the existing initialization path; the older initializer remains available without diagnostics.

The diagnostic result also includes `cleanup_verified`. Initialization closes an already-acquired activity or manifest connection when its paired setup fails; a failed close marks cleanup unverified. SQLx connect errors are conservatively marked unverified because the error does not confirm whether a worker was acquired or fully closed.

**Connection Close Ownership** (`activitystoreinterface.rs`, `manifestinterface.rs`): Each interface's consuming `close(self)` awaits its owned SQLx connection's close result. The FFI `dcli_store_close()` consumes the combined store handle and reports success only when both activity and manifest workers close successfully. Its legacy `dcli_store_free()` path remains a best-effort drop without a worker-close wait.

**Activity Write Boundary** (`activitystoreinterface.rs`): A batch has one outer transaction and one savepoint per report. Failures within a report roll back all rows written for that report, including its parent and child rows, while subsequent reports may commit. Errors during savepoint control or transaction commit roll the batch back and propagate to the caller. Because queue completion is part of the report write, a rolled-back report remains available for retry.

**Mode Enum** (enums/mode.rs): Contains 70+ mode variants covering all Crucible game types. Includes methods `is_crucible()`, `is_private()`, `from_id()`, `as_id()`. Modes can be compound (e.g., `AllPvP`, `AllPvPQuickplay`).

**Response Structs**: Tightly coupled to Bungie API JSON structure. Field names often match API exactly. Heavy use of `Option<T>` for nullable API fields. Custom deserializers in some cases.

**Hash Constants System** (utils.rs): Special activity hashes identify specific playlists that need special handling - Competitive, Checkmate variants, Iron Banner modes. These drive filtering and data correction logic throughout the codebase.

**Async Everywhere**: Nearly all public APIs are async functions returning `Result<T, Error>`. Callers must provide tokio runtime context.

Created and maintained by Nori.
