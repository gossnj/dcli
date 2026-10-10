# Noridoc: dclistat CLI

Path: @/src/dclistat

### Overview

- `dclistat` prints selected Crucible summary statistics for a Bungie player.
- It queries the shared local activity database through [dcli](../dcli), using the same data created by [dclisync](../dclisync).
- The command-line contract is exercised in [src/cli_contract.rs](src/cli_contract.rs).

### How it fits into the larger codebase

- The executable uses the common activity-store interface and shared parsers/calculations rather than maintaining separate API or database logic.
- It can request a sync before querying with `--sync`; otherwise it reads existing data for the selected time window, mode, and character class.
- Workspace test and smoke commands are documented in [../../tests/README.md](../../tests/README.md).

### Core Implementation

- Clap requires a Bungie name and one or more `--stat` values. It parses the requested mode, time range, and character class before store access.
- The tool resolves the data directory and initializes the activity store, looks up the player, optionally syncs, then retrieves a summary for the chosen filters.
- Each requested stat is transformed from the summary into its CLI representation and emitted in request order as comma-separated values. A missing summary prints `No data found` and returns.
- `--version` is enabled in the Clap command metadata.

### Things to Know

- Custom date moments require their corresponding RFC 3339 timestamps, and the end time must be later than the start time; invalid periods exit before querying.
- Store initialization and player lookup failures are reported at the CLI boundary. If an optional sync fails, the tool reports that condition and continues with existing stored data.
- The compiled binary requires a compile-time API key; API-backed runtime actions can also receive the key through `--api-key` or `DESTINY_API_KEY`.

Created and maintained by Nori.
