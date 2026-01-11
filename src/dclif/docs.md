# Noridoc: dclif CLI Tool

Path: @/dcli/src/dclif

### Overview

Command-line tool for querying specific Destiny 2 PVP statistics with filtering capabilities. Provides targeted stat queries from local database or live API.

### How it fits into the larger codebase

Uses @/dcli/src/dcli library for data access. Similar to @/dcli/src/dclistat but with more sophisticated filtering options. Bridges the gap between comprehensive reports (@/dcli/src/dcliah) and simple single-stat queries (@/dcli/src/dclistat).

### Core Implementation

**Dependencies** (@/dcli/src/dclif/Cargo.toml):
- structopt: CLI argument parsing
- tokio: Async runtime
- chrono: Date/time handling for time period filters
- num-format: Number formatting for output
- dcli + tell: Core library and output

**Filtering Capabilities**:
- Mode-specific queries
- Time period filtering (daily, weekly, custom ranges)
- Character class filtering
- Formatted numeric output via num-format

**Use Cases**: Automation scripts requiring specific stat values with filtering, integration with monitoring systems, custom reporting.

### Things to Know

**Enhanced Filtering**: More sophisticated than dclistat with support for time periods and character filtering while maintaining focus on specific stat extraction.

**Number Formatting**: Uses num-format crate for locale-aware number formatting. Improves readability of large numbers (kills, games played, etc.).

**Query Optimization**: Designed for efficient single-stat extraction from database rather than full stat computation.

Created and maintained by Nori.
