# Noridoc: dclisync CLI

Path: @/src/dclisync

### Overview

- `dclisync` manages the local activity-store player list and downloads Crucible history from Bungie.
- Other command-line tools and the FFI query the store populated by this CLI through the shared [dcli library](../dcli).
- Argument parsing and operation ordering are covered beside the implementation in [src/cli_contract.rs](src/cli_contract.rs).

### How it fits into the larger codebase

- The CLI resolves a data directory and initializes `ActivityStoreInterface`; the default location comes from shared utilities in [../dcli/src/utils.rs](../dcli/src/utils.rs).
- Add, remove, and sync operations update or read the same store used by reporting tools such as [dclistat](../dclistat) and [dcliah](../dcliah).
- Bungie requests use the core API client and require a key for API-backed operations. At runtime the key can come from `--api-key` or `DESTINY_API_KEY`; compilation also requires `DESTINY_API_KEY` because it is embedded in the binary.
- The Rust 1.83.0 workspace tests and executable smoke checks are documented in [../../tests/README.md](../../tests/README.md).

### Core Implementation

- Clap parses player names into `PlayerName` values before store operations. `--add` and `--remove` accept one or more names; `--sync` accepts zero or more, where zero names means sync every configured player.
- Multiple requested operations run in a defined order: import group members, add names, remove names, sync, then list configured players.
- Sync can run once or in daemon mode. The sync path chooses `sync_all` for an empty name list and `sync_players` for explicit names; interval controls daemon pauses.
- `--version` is provided by the Clap command metadata, and parser contract tests exercise version output and argument requirements.

### Things to Know

- Sync, add, and group import require an API key. Listing and removing configured players do not perform Bungie API lookups.
- Player-name validation happens during Clap parsing, before the database is opened. Runtime API or database failures are formatted at the CLI boundary and terminate the relevant operation.
- Interrupt handling is platform-specific: Unix listens for termination signals and Windows uses Ctrl-C handling so a running sync can finish its current operation and shut down.

Created and maintained by Nori.
