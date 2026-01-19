# Noridoc: dclistat Source

Path: @/dcli/src/dclistat/src

### Overview

Source implementation of dclistat command-line tool for displaying specific Destiny 2 PVP statistics with minimal formatting for scripting purposes.

### How it fits into the larger codebase

Implementation of dclistat CLI tool declared in @/dcli/src/dclistat/Cargo.toml. Uses @/dcli/src/dcli library for stat queries. Designed for shell script integration and automation where minimal, parseable output is preferred over formatted tables.

### Core Implementation

**Single File**: All logic in main.rs.

**Key Characteristics**:
- Minimal output formatting (just stat value)
- Fast single-stat extraction
- Script-friendly output format
- Support for specific stat queries

**Query Types**:
- KD ratio
- Win rate / Win percentage
- Total kills
- Total deaths
- Games played
- Efficiency
- Specific mode stats
- Time period filtered stats

**Output Format**:
```
# Example
$ dclistat --name player#1234 --stat kd
1.45
```

Simple numeric output or stat=value pairs. Easy to parse with shell tools.

### Things to Know

**Scripting Focus**: Primary use case is shell scripts and automation pipelines. Not designed for human-readable reports (use dcliah for that).

**Performance**: Optimized for quick queries. May use database indexes effectively for targeted stat extraction.

**Multiple Stats**: Some invocations may support multiple stats in single call, output one per line.

**Data Source Options**: May query local database (fast) or live API (slow but current) depending on stat requested.

**Combination with Other Tools**: Often piped or composed with other CLI tools:
```bash
# Example patterns
kd=$(dclistat --name player#1234 --stat kd)
if [ "$kd" -gt 1.5 ]; then echo "High KD!"; fi

# Or store in CSV
echo "player,kd,efficiency" > stats.csv
dclistat --name player#1234 --stat kd,efficiency >> stats.csv
```

**Mode and Time Filters**: Supports same filtering as dcliah but returns just requested stat value(s).

Created and maintained by Nori.
