# Noridoc: dcliah Source

Path: @/dcli/src/dcliah/src

### Overview

Single-file implementation of dcliah command-line tool for displaying Destiny 2 Crucible activity history and statistics with formatted output.

### How it fits into the larger codebase

Implementation of dcliah CLI tool declared in @/dcli/src/dcliah/Cargo.toml. Uses @/dcli/src/dcli `ActivityStoreInterface::retrieve_activities_summary()` for stat queries. Demonstrates complete usage pattern for querying and displaying aggregated statistics from the dcli database.

### Core Implementation

**main.rs** (31690 lines): Massive single-file CLI application with extensive formatting logic.

**Key Sections**:
1. **Argument Parsing**: structopt-based CLI with filters for character, mode, time period, output format
2. **Database Queries**: Calls retrieve_activities_summary() with various filters
3. **Output Formatting**:
   - Human-readable tables with aligned columns
   - TSV (tab-separated values) for scripting
   - JSON for programmatic consumption
4. **Stat Calculations**: KD ratio, efficiency, win percentage, averages
5. **Auto-Sync**: Optional sync before displaying stats

**Output Sections** (Human Format):
- Player name and character info
- Time period and mode filters
- Games played, wins, losses, win rate
- Kills, deaths, KD ratio
- Efficiency (K+A)/D
- Opponents defeated
- Assists
- Suicides
- Precision kills and percentage
- Average lifespan
- Total time played

**Query Filters**:
- Character: Specific class or all characters via CharacterClassSelection
- Mode: Individual modes or compound modes (all_pvp, all_competitive, etc.)
- Time: all_time, weekly, daily, custom date range via DateTimePeriod
- Player: By Bungie name

### Things to Know

**Large File Size**: 31k+ lines because extensive formatting logic for tables, text wrapping, column alignment. Each output format requires significant code.

**Table Formatting**: Uses custom formatting logic (not external crate) for aligned columns. Calculates column widths based on content.

**TSV Mode**: Useful for scripting. Each stat on separate line: `stat_name\tvalue\n`. Easy to parse with awk/grep.

**JSON Mode**: Complete structured output. All stats in single JSON object. Useful for integrating with other tools.

**Mode Filtering Examples**:
- `--mode control`: Only Control mode
- `--mode all_pvp`: All PvP modes aggregated
- `--mode pvp_competitive`: All competitive modes

**Time Period Edge Cases**: Weekly/daily periods use Destiny 2 reset times (Tuesday 17:00 UTC for weekly). "This week" means since last Tuesday reset.

**Auto-Sync Integration**: Can call sync_player() before querying. Ensures data is current. Adds latency but guarantees freshness.

**Error Handling**: Comprehensive error messages for missing database, player not found, invalid arguments. Uses tell library for user-friendly output.

Created and maintained by Nori.
