# Noridoc: dclif Source

Path: @/dcli/src/dclif/src

### Overview

Source implementation of dclif command-line tool for querying specific Destiny 2 PVP statistics with advanced filtering capabilities.

### How it fits into the larger codebase

Implementation of dclif CLI tool declared in @/dcli/src/dclif/Cargo.toml. Uses @/dcli/src/dcli library for stat queries. Sits between simple stat extraction (@/dcli/src/dclistat) and comprehensive reports (@/dcli/src/dcliah), offering filtered targeted queries.

### Core Implementation

**Single File**: All logic in main.rs.

**Key Features**:
- Specific stat queries (like dclistat)
- Advanced filtering (like dcliah)
- Formatted numeric output (via num-format)
- Support for mode, time, character filters

**Dependencies**:
- **chrono**: Date/time handling for time period filters
- **num-format**: Locale-aware number formatting (1000 → 1,000)
- **structopt**: CLI argument parsing
- **dcli**: Core library for queries

**Query Patterns**:
```bash
# KD for specific mode
dclif --name player#1234 --stat kd --mode control

# Win rate for this week
dclif --name player#1234 --stat winrate --moment weekly

# Efficiency for specific character
dclif --name player#1234 --stat efficiency --class hunter

# Multiple filters combined
dclif --name player#1234 --stat kd --mode pvp_competitive --moment weekly --class warlock
```

**Output Format**: Formatted numbers with thousands separators and appropriate precision:
```
KD Ratio: 1.45
Total Kills: 1,234
Win Rate: 67.5%
```

### Things to Know

**Filtering Flexibility**: Supports all major filters (mode, time, character) while maintaining focus on specific stat output. Best of both worlds between dclistat simplicity and dcliah comprehensiveness.

**Number Formatting**: Uses num-format crate for locale-aware display. Makes large numbers (total kills, games played) more readable without scientific notation.

**Query Performance**: Likely uses same underlying database queries as dcliah but returns only requested stat(s). May be more efficient than running full stat computation if only one value needed.

**Script Integration**: Still script-friendly due to focused output, but with better human readability than raw numbers from dclistat.

**Stat Calculation**: Uses same calculation logic as dcliah for KD, efficiency, etc. Ensures consistency across tools.

**Database Source**: Queries local database populated by dclisync. Fast queries, no API latency.

**Mode Compound Support**: Supports compound modes like all_pvp, all_competitive for aggregated queries across multiple individual modes.

Created and maintained by Nori.
