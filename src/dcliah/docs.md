# Noridoc: dcliah CLI Tool

Path: @/dcli/src/dcliah

### Overview

Command-line tool for displaying Destiny 2 Crucible activity history and aggregated statistics. Queries the local database populated by dclisync to generate reports on player performance across various time periods, modes, and characters.

### How it fits into the larger codebase

dcliah is a consumer of the database created by @/dcli/src/dclisync. It uses @/dcli/src/dcli `ActivityStoreInterface::retrieve_activities_summary()` to query aggregated stats. Demonstrates how to use the dcli library for reporting. The Swift app at @/Last Banner implements similar querying logic via FFI at @/dcli/src/dcli-ffi but with GUI presentation instead of console output.

### Core Implementation

**Single Main File** (@/dcli/src/dcliah/src/main.rs, 31690 lines): All CLI logic including argument parsing, query execution, and formatted output.

**Key Functionality**:
- Query stats by player name (Bungie name format)
- Filter by character class (Titan, Hunter, Warlock, all)
- Filter by Crucible mode (specific modes or aggregates like all_pvp)
- Filter by time period (all_time, weekly, daily, custom date range)
- Display formatted stats: KD, efficiency, win rate, kills, deaths, games played, etc.
- Option to auto-sync before displaying stats

**Output Formats**:
- Human-readable formatted tables (default)
- TSV (tab-separated values) for scripting
- JSON for programmatic consumption

**Data Query Flow**:
1. Parse arguments for player, character, mode, time period
2. Initialize ActivityStoreInterface with database path
3. Call retrieve_activities_summary() with filters
4. Receive PlayerActivitiesSummary with aggregated data
5. Format and display via tell library

### Things to Know

**Auto-Sync Option**: Can trigger sync before displaying stats to ensure data freshness. Calls same sync_player() method that dclisync uses.

**Time Period Handling**: Uses @/dcli/src/dcli/src/enums/moment.rs `DateTimePeriod` for time filtering. Supports weekly/daily reset times specific to Destiny 2 (17:00 UTC Tuesday for weekly).

**Competitive Mode Filtering**: Benefits from the mode transformation logic in @/dcli/src/dcli/src/activitystoreinterface.rs. Queries for competitive modes will correctly include Season 25+ activities due to hash-based filtering implemented in the core library.

**Character Selection**: Can aggregate across all characters or filter to specific class. Uses CharacterClassSelection enum from core library.

**Database Requirement**: Must have database created and populated by dclisync first. Tool will error if database doesn't exist or player not found.

**Mode Aggregation**: Modes like "all_pvp" or "all_pvp_quickplay" query multiple individual modes and aggregate results. Handled by Mode enum's compound mode support.

Created and maintained by Nori.
