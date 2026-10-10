# Noridoc: dclisync Source

Path: @/src/dclisync/src

### Overview

- [main.rs](main.rs) implements argument parsing, store setup, player management, syncing, daemon behavior, and command output.
- [cli_contract.rs](cli_contract.rs) captures the parser behavior that forms the command-line contract.
- The executable delegates data retrieval and persistence to the shared [dcli crate](../../dcli).

### How it fits into the larger codebase

- `main` parses options, resolves the data directory, and initializes `ActivityStoreInterface` before running requested operations.
- Import and sync use core API calls; add, remove, list, and sync use the activity store, which is also read by the reporting CLIs.
- The package is one member of the workspace in [../Cargo.toml](../Cargo.toml), built and tested with the pinned toolchain and lockfile.

### Core Implementation

- Clap parses flags and one-or-more Bungie names into typed values. The sync option permits no names to mean all configured players; add and remove require at least one name.
- Operation dispatch follows import, add, remove, sync, and list order. This lets one invocation apply player-list changes before syncing or listing.
- Sync selects all configured players when the parsed name list is empty, otherwise it sends the explicit list to the core store. Daemon mode repeats the sync after the configured interval.
- Errors from setup and sync are formatted through shared utilities; per-player add/remove failures are reported while processing other names.

### Things to Know

- Parser validation prevents API-dependent operations from running without an explicit or environment-provided runtime API key. The compiled program also contains the compile-time key required by the workspace.
- Signal handling differs by target family, while the store and sync behavior remains in the shared library.
- The module includes parser contract tests directly so changes to `--version`, multi-value handling, and required operations stay tied to the executable's actual Clap definitions.

Created and maintained by Nori.
