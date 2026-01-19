# Noridoc: dclisync Source

Path: @/dcli/src/dclisync/src

### Overview

Single-file implementation of the dclisync command-line tool for syncing Destiny 2 activity history to local SQLite database.

### How it fits into the larger codebase

Implementation of the dclisync CLI tool declared in @/dcli/src/dclisync/Cargo.toml. Uses @/dcli/src/dcli `ActivityStoreInterface` for all sync operations. This is the canonical reference implementation for how to sync player activities using the dcli library.

### Core Implementation

**main.rs** (16609 lines): Complete CLI application in single file.

**Structure**:
1. **Command-line Arguments**: structopt derives CLI argument parsing from struct definitions
   - Player name (Bungie name format: name#1234)
   - Data directory path (optional, defaults to system data dir)
   - Operation mode: --add, --remove, --sync, --sync-single

2. **Main Function**: Async main using tokio runtime
   - Parse arguments
   - Initialize ActivityStoreInterface with data directory
   - Execute requested operation
   - Handle errors and display results

3. **Signal Handling**: Platform-specific signal handlers
   - Unix: signal-hook for SIGTERM/SIGINT
   - Windows: ctrlc crate for Ctrl+C
   - Allows graceful shutdown during long syncs

4. **Sync Operations**:
   - `add_player_to_sync()`: Add player to sync list in database
   - `remove_player_from_sync()`: Remove player from sync list
   - `sync_player()`: Fetch activities from API, transform, store in database
   - Progress updates via ActivityStoreInterface's built-in progress bars

**Key Logic Flow**:
```rust
// Pseudo-code representation
let store = ActivityStoreInterface::init_with_path(&data_dir, None).await?;
store.add_player_to_sync(&player_name).await?;
store.sync_player(&player_name).await?;
```

### Things to Know

**Single-File Architecture**: Despite size (16k+ lines), everything in one file. No module splitting. This is typical for smaller CLI tools in Rust.

**Async/Await Throughout**: Uses tokio for async runtime. Main function is `#[tokio::main] async fn main()`.

**Error Propagation**: Heavy use of `?` operator for error propagation. Errors bubble up to main where they're formatted and displayed.

**Data Directory Management**: Uses `determine_data_dir()` from @/dcli/src/dcli/src/utils.rs to find appropriate system directory if not specified.

**PlayerName Parsing**: Uses `PlayerName::from_bungie_name()` to parse and validate Bungie name format (name#code).

**Incremental Syncing**: ActivityStoreInterface tracks last sync time per player. Only fetches activities since last sync, making subsequent syncs fast.

**Concurrent PGCR Fetching**: ActivityStoreInterface fetches PGCRs in chunks (25 concurrent requests) to balance speed vs API rate limits.

**Database Transactions**: All database writes wrapped in transactions for consistency. Interrupted syncs leave database in valid state.

**Logging**: Supports RUST_LOG environment variable via env_logger for debug output.

Created and maintained by Nori.
