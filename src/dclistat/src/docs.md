# Noridoc: dclistat Source

Path: @/src/dclistat/src

### Overview

- [main.rs](main.rs) defines the `dclistat` parser, query flow, stat transformation, and output.
- [cli_contract.rs](cli_contract.rs) records the parser behavior, including the required stat selection.
- The executable delegates data access and calculations to the shared [dcli crate](../../dcli).

### How it fits into the larger codebase

- The CLI opens the same activity store populated by [dclisync](../../dclisync) and used by other reporting applications.
- It calls shared date, mode, character, and stat parsers before asking `ActivityStoreInterface` for a filtered summary.
- The package is built and tested as part of the workspace in [../Cargo.toml](../Cargo.toml).

### Core Implementation

- Clap requires a player name and at least one `--stat` value; one invocation can request several stats.
- The runtime resolves the data directory, initializes the store, looks up the player, optionally syncs, and retrieves the summary for the selected date period, mode, and character class.
- The output loop converts each selected field to text in the same order it was requested, then joins values with commas. Empty query results take the explicit `No data found` path.
- The parser enables `--version`, and the adjacent contract tests exercise required and repeated stat values.

### Things to Know

- Custom start and end moments require parsed RFC 3339 timestamps. `DateTimePeriod` rejects a range whose end is not after its start.
- Optional sync errors are reported while existing local data remains available for the query; initialization, player lookup, and query errors terminate with a formatted message.
- Summary values are formatted through shared dcli calculation helpers, keeping ratio and average behavior consistent with other tools.

Created and maintained by Nori.
