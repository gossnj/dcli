# Noridoc: dclitime CLI Tool

Path: @/dcli/src/dclitime

### Overview

Command-line tool for generating date/time stamps for Destiny 2 weekly event moments. Calculates timestamps for weekly reset, daily reset, and other recurring events in Destiny 2's schedule.

### How it fits into the larger codebase

Utility tool using date/time functions from @/dcli/src/dcli/src/utils.rs like `get_last_weekly_reset()`, `get_last_daily_reset()`, and `get_last_friday_reset()`. These reset times are important for filtering activity data by week or day in other tools like @/dcli/src/dcliah.

### Core Implementation

**Reset Calculations**:
- Weekly reset: Tuesday 17:00 UTC (uses hardcoded past reset and interval math)
- Daily reset: Daily 17:00 UTC
- Friday reset: Friday 17:00 UTC (for Trials of Osiris)

**Algorithm** (@/dcli/src/dcli/src/utils.rs:273-283):
```rust
fn find_previous_moment(past_reset: DateTime<Utc>, interval: i64) -> DateTime<Utc> {
    let now = Utc::now();
    now - Duration::seconds((now - past_reset).num_seconds() % interval)
}
```

Uses modulo arithmetic on seconds since a known past reset to find most recent occurrence.

**Output**: Timestamps in various formats (RFC3339, Unix epoch, human-readable) for use in other tools or scripts.

### Things to Know

**Destiny 2 Schedule**: Weekly reset Tuesday 17:00 UTC is fundamental to Destiny 2's content cycle. Many activities and rewards reset at this time.

**Time Period Filtering**: The DateTimePeriod enum in @/dcli/src/dcli/src/enums/moment.rs uses these calculations to implement "weekly" and "daily" time period filters.

**Hardcoded Anchor**: Uses fixed past date (2020-11-10 17:00:00 UTC) as anchor for calculations. Works as long as reset schedule doesn't change.

**Trials Schedule**: Friday reset (17:00 UTC) marks start of Trials of Osiris weekend event.

Created and maintained by Nori.
