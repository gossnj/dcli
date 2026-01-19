# Noridoc: dcliad CLI Tool

Path: @/dcli/src/dcliad

### Overview

Command-line tool for displaying detailed Destiny 2 Crucible activity/match information. Shows per-match breakdowns including player performance, weapon stats, medals earned, and team results.

### How it fits into the larger codebase

dcliad provides granular activity details as opposed to @/dcli/src/dcliah which shows aggregated statistics. Uses @/dcli/src/dcli library to query individual activity records from the database populated by @/dcli/src/dclisync. Can also fetch live activity details directly from the Bungie API via PGCR (Post Game Carnage Report) endpoints.

### Core Implementation

**Operations**:
- Display detailed stats for a specific activity by activity ID
- Show weapon usage breakdown (kills per weapon)
- Display medals earned during the match
- Show all player performances in the activity
- Team scores and final standings
- Match metadata (map, mode, date/time, duration)

**Data Sources**:
- Local database: Queries activity, character_activity_stats, weapon_result, medal_result tables
- Live API: Can fetch PGCR directly from Bungie if not in database

**Output**: Formatted console output showing comprehensive match details with player names, scores, KD ratios, weapon statistics, and medals.

### Things to Know

**Activity ID Requirement**: Requires specific activity instance ID which can be obtained from dcliah output or Bungie.net URLs.

**PGCR Data**: Post Game Carnage Reports contain the most detailed per-match data including loadouts, ability kills, and precision kill counts per weapon.

**Manifest Dependency**: Heavily reliant on manifest database to resolve weapon names, medal names, and activity/map names from hash IDs.

**Player Privacy**: PGCR data only available if players have appropriate privacy settings enabled on Bungie.net account.

Created and maintained by Nori.
