# Noridoc: dclia Source

Path: @/dcli/src/dclia/src

### Overview

Source implementation of dclia command-line tool for displaying a player's current or recent Destiny 2 activity in real-time.

### How it fits into the larger codebase

Implementation of dclia CLI tool declared in @/dcli/src/dclia/Cargo.toml. Uses @/dcli/src/dcli `ApiInterface::retrieve_current_activity()` to query live player status from Bungie API. Useful for monitoring and notification scripts.

### Core Implementation

**Single File**: All logic in main.rs.

**Query Flow**:
1. Parse player name from arguments
2. Call ApiInterface::search_destiny_player() to get member ID
3. Call ApiInterface::retrieve_current_activity() for active session
4. Parse and display activity details

**Output Information**:
- Activity name (e.g., "Endless Vale", "Control")
- Activity mode and type
- Map/destination name
- Start time (how long ago activity started)
- Character in activity
- Fireteam members (if available)
- Activity instance ID

**Use Cases**:
- Shell scripts that trigger on activity changes
- Notifications when loading into Crucible map
- Monitoring play sessions
- Detecting when player enters specific activity

### Things to Know

**Live API Only**: Always makes API call. No local database involved. Fresh data but requires API access.

**Privacy Dependency**: Requires "Show my Destiny game Activity feed" enabled. Otherwise returns no activity or error.

**Activity Detection Window**: Can only detect active activities or very recently completed. If player in orbit/tower, may show last activity or nothing.

**Notification Script Pattern**: Common use is monitoring loop:
```bash
while true; do
  dclia --name player#1234 | grep "Endless Vale" && notify "Map detected!"
  sleep 30
done
```

**Manifest Required**: Needs manifest to resolve activity names from hashes. Without manifest, shows numeric IDs only.

**Character Context**: Shows which character is in activity. Useful for tracking per-character playtime.

**Fireteam Detection**: Can show fireteam members if privacy settings allow. Useful for detecting group play vs solo.

Created and maintained by Nori.
