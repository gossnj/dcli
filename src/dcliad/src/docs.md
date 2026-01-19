# Noridoc: dcliad Source

Path: @/dcli/src/dcliad/src

### Overview

Source implementation of dcliad command-line tool for displaying detailed Destiny 2 Crucible match information including per-player stats, weapons, and medals.

### How it fits into the larger codebase

Implementation of dcliad CLI tool declared in @/dcli/src/dcliad/Cargo.toml. Queries individual activity records from database via @/dcli/src/dcli `ActivityStoreInterface` or fetches live PGCR data via `ApiInterface`. Provides granular match analysis as opposed to aggregated stats in @/dcli/src/dcliah.

### Core Implementation

**Single File**: All logic in main.rs.

**Key Features**:
- Query by activity ID (instance ID from Bungie)
- Display all players in match with stats
- Show weapon usage per player (kills per weapon)
- Display medals earned
- Team compositions and results
- Match metadata (map, mode, duration, date)

**Data Sources**:
1. **Local Database**: Query activity, character_activity_stats, weapon_result, medal_result tables joined by activity ID
2. **Live API**: Fetch PGCR directly via `ApiInterface::retrieve_post_game_carnage_report()`

**Output Format**:
- Match header (map, mode, date, duration)
- Team results (Alpha/Bravo scores, standings)
- Player table (columns: name, class, kills, deaths, KD, assists, score, team)
- Weapon breakdown per player
- Medal list per player

### Things to Know

**Activity ID Format**: Bungie instance IDs are 64-bit integers. Typically obtained from dcliah output or Bungie.net match URLs.

**PGCR Richness**: PGCRs contain most detailed data available - exact weapon loadouts, ability kills, medals, precision kills per weapon. Much more than aggregated stats.

**Manifest Dependency**: Heavy reliance on manifest for name resolution:
  - Weapon hashes → weapon names
  - Medal hashes → medal names
  - Activity hashes → map names
  - Without manifest, only numeric IDs displayed

**Privacy Considerations**: Can only display data for activities where players have appropriate privacy settings. Private match data only available to participants.

**Team Display**: Shows team IDs and standings (Victory/Defeat). Some modes (Rumble) have no teams, displayed differently.

**Detailed Weapon Stats**: Per-weapon precision kills, kills, precision kill percentage. Identifies meta weapons and loadout patterns.

**Medal Significance**: Medals indicate exceptional performance (multi-kills, kill streaks, etc.). Tier information (Bronze/Silver/Gold) from manifest.

Created and maintained by Nori.
