# Noridoc: dclistat CLI Tool

Path: @/dcli/src/dclistat

### Overview

Command-line tool for displaying specific Destiny 2 PVP statistics. Focused utility for querying individual stat values from aggregated player data.

### How it fits into the larger codebase

Uses @/dcli/src/dcli library to query statistics from the local database or live API. Complements @/dcli/src/dcliah by providing targeted stat queries rather than comprehensive reports. Useful for scripting and automation where only specific metrics are needed.

### Core Implementation

**Targeted Stat Queries**:
- Individual stats like KD ratio, win rate, kills, deaths
- Specific mode performance
- Single metric extraction

**Output**: Simple formatted output suitable for parsing by scripts or piping to other tools.

**Data Sources**: Can query both local database (via ActivityStoreInterface) and live API (via ApiInterface) depending on stat requested.

### Things to Know

**Scripting Focus**: Designed for integration with shell scripts and automation. Output format optimized for parsing.

**Stat Specificity**: Rather than showing all stats, allows querying just the stat(s) you need. Reduces noise for automation.

**Fast Queries**: Optimized for quick single-stat lookups from database.

Created and maintained by Nori.
