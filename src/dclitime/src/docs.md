# Noridoc: dclitime Source

Path: @/dcli/src/dclitime/src

### Overview

Source implementation of dclitime command-line tool for calculating Destiny 2 weekly and daily reset timestamps.

### How it fits into the larger codebase

Implementation of dclitime CLI tool declared in @/dcli/src/dclitime/Cargo.toml. Uses date/time utility functions from @/dcli/src/dcli/src/utils.rs (`get_last_weekly_reset()`, `get_last_daily_reset()`, `get_last_friday_reset()`). Useful for scripting and automation that needs to align with Destiny 2's schedule.

### Core Implementation

**Single File**: All logic in main.rs.

**Reset Types**:
1. **Weekly Reset**: Tuesday 17:00 UTC
2. **Daily Reset**: Daily 17:00 UTC
3. **Friday Reset**: Friday 17:00 UTC (Trials of Osiris)

**Output Formats**:
- RFC3339 timestamp: `2024-11-01T17:00:00+00:00`
- Unix epoch seconds: `1730476800`
- Human-readable: `Tuesday, November 1, 2024 at 5:00 PM UTC`

**Algorithm** (from @/dcli/src/dcli/src/utils.rs):
```rust
// Uses modulo math on seconds since known past reset
fn find_previous_moment(past_reset: DateTime<Utc>, interval: i64) -> DateTime<Utc> {
    let now = Utc::now();
    now - Duration::seconds((now - past_reset).num_seconds() % interval)
}
```

**Known Anchors**:
- Weekly: 2020-11-10 17:00:00 UTC (a known Tuesday reset)
- Daily: 2020-11-10 17:00:00 UTC (any known reset time)
- Friday: 2020-12-04 17:00:00 UTC (a known Friday reset)

### Things to Know

**Reset Schedule Stability**: Bungie's reset times (Tuesday 17:00 UTC) have been consistent for years. Algorithm assumes this continues. If Bungie changes reset schedule, anchor dates need updating.

**Time Zone Conversion**: All calculations in UTC. Output can be formatted in other zones for display but core logic is UTC.

**Use in Other Tools**: dcliah and other tools use these functions internally for "weekly" and "daily" time period filters. dclitime exposes same calculations for external scripts.

**Trials Weekend**: Friday reset marks start of Trials of Osiris (competitive weekend event). Important timing for PvP-focused applications.

**Scripting Examples**:
```bash
# Get timestamp for "this week" queries
weekly_reset=$(dclitime --weekly --format epoch)
dcliah --name player#1234 --start-date $weekly_reset

# Check if reset just happened
last_reset=$(dclitime --weekly --format epoch)
if [ $(date +%s) -lt $(($last_reset + 3600)) ]; then
  echo "Reset was less than 1 hour ago"
fi
```

**Output Precision**: Timestamps are exact to the second (17:00:00), matching Bungie's reset timing.

Created and maintained by Nori.
