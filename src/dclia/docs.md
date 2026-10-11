# Noridoc: dclia CLI Tool

Path: @/dcli/src/dclia

### Overview

Command-line tool for displaying information about a player's current activity within Destiny 2. Shows real-time or recent activity status.

### How it fits into the larger codebase

Uses @/dcli/src/dcli `ApiInterface::retrieve_current_activity()` to query Bungie API for player's active session. Provides live data rather than historical data from the database. Useful for monitoring when a player loads into a new match or determining current game mode and map.

### Core Implementation

Queries Bungie API for player's current or most recent activity session. Displays:
- Current activity name and mode
- Map/destination
- Characters in the activity
- Fireteam information
- Activity start time

**Use Cases**:
- Notification scripts when loading into Crucible match
- Determining current map before match starts
- Monitoring play sessions

### Things to Know

**Live API Only**: Does not use local database. Always makes API call to Bungie for real-time data.

**Privacy Requirement**: Player must have "Show my Destiny game Activity feed on Bungie.net" enabled in privacy settings.

**Activity Detection**: Can only detect active activities. If player in orbit or not playing, may return no activity or most recent completed activity.

Created and maintained by Nori.
