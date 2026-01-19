# Noridoc: dclisync CLI Tool

Path: @/dcli/src/dclisync

### Overview

Command-line tool for downloading and syncing Destiny 2 Crucible activity history from the Bungie API into a local SQLite database. Primary tool for building the activity datastore that other dcli tools query.

### How it fits into the larger codebase

dclisync is the data ingestion layer for dcli. It uses @/dcli/src/dcli `ActivityStoreInterface` to manage the database at a user-specified path. Other CLI tools like @/dcli/src/dcliah query the database that dclisync populates. The FFI layer at @/dcli/src/dcli-ffi exposes equivalent sync functionality via `dcli_store_sync_player()` for the Swift app at @/Last Banner.

### Core Implementation

**Single Main File** (@/dcli/src/dclisync/src/main.rs, 16609 lines): All CLI logic in main.rs.

**Dependencies** (@/dcli/src/dclisync/Cargo.toml):
- structopt: Command-line argument parsing
- tokio: Async runtime with full feature set
- log + env_logger: RUST_LOG environment variable support
- Platform-specific: signal-hook (Unix) / ctrlc (Windows) for graceful shutdown

**Core Operations**:
- **Add Player**: Add Bungie name to sync list in database
- **Remove Player**: Remove player from sync list
- **Sync**: Download activity history for all players in sync list
- **Sync Single**: Sync specific player without adding to list

**Data Flow**:
1. Parse command-line arguments (player name, data directory, operation)
2. Initialize `ActivityStoreInterface` with data directory path
3. Execute operation: add_player_to_sync(), remove_player_from_sync(), or sync_player()
4. ActivityStoreInterface handles API calls, PGCR fetching, database insertion
5. Progress feedback via indicatif progress bars (from dcli library)

**Signal Handling**: Registers SIGTERM/SIGINT handlers for graceful shutdown during long-running syncs. Allows database to be left in consistent state if interrupted.

### Things to Know

**Initial Sync Duration**: First sync of a player with extensive history can take several minutes. Subsequent syncs are fast since only new activities are fetched.

**Concurrent Requests**: Uses chunked concurrent requests (25 at a time) to Bungie API for PGCR data. Balance between speed and API rate limiting.

**Database Location**: Default is system data directory (e.g., `~/Library/Application Support/dcli` on macOS). Can be overridden with command-line option. Both dclisync and querying tools must point to same database path.

**Incremental Syncing**: Smart about only fetching activities not already in database. Tracks last sync time per player in sync table.

**API Key Requirement**: Must have DESTINY_API_KEY environment variable set at compile time. Key embedded in binary.

**Error Recovery**: If sync fails partway through, can simply re-run. Will resume from where it left off. Database transactions ensure consistency.

Created and maintained by Nori.
