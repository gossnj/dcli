# Noridoc: Enums Module

Path: @/dcli/src/dcli/src/enums

### Overview

Type-safe enum definitions providing strongly-typed abstractions for Destiny 2 game concepts. Includes game modes, character classes, platforms, time periods, completion reasons, and other categorical data from the Bungie API.

### How it fits into the larger codebase

Enums are used throughout @/dcli/src/dcli for type safety and validation. They prevent invalid state (e.g., requesting stats for non-existent mode). Used in API interfaces (@/dcli/src/dcli/src/apiinterface.rs), database queries (@/dcli/src/dcli/src/activitystoreinterface.rs), and exposed through FFI (@/dcli/src/dcli-ffi) as i32 values with conversion functions.

### Core Implementation

**mod.rs**: Declares all enum modules and re-exports types.

**Key Enums**:

- **mode.rs** (22555 lines): `Mode` enum with 70+ variants representing all Crucible game types. Critical methods:
  - `from_id(u32) -> Result<Mode>`: Convert Bungie API ID to enum
  - `as_id() -> u32`: Convert enum to API ID
  - `is_crucible() -> bool`: Check if mode is PvP
  - `is_private() -> bool`: Check if private match
  - Compound modes like `AllPvP`, `AllPvPQuickplay` that aggregate multiple modes

- **character.rs** (5353 lines): `CharacterClass` (Titan, Hunter, Warlock, Unknown) and `CharacterClassSelection` (adds LastActive and All options for queries)

- **platform.rs** (3300 lines): `Platform` enum - Xbox, PlayStation, Steam, Stadia, BungieNext. Methods: `from_id()`, `as_id()`, `to_human_string()`

- **moment.rs** (12127 lines): `DateTimePeriod` struct and `Moment` enum. Handles time period filtering (Daily, Reset, Week, AllTime, Custom). Uses `get_last_weekly_reset()` and related utilities from @/dcli/src/dcli/src/utils.rs

- **standing.rs** (2487 lines): `Standing` enum - Victory, Defeat, Mercy, Unknown. Represents match outcome.

- **completionreason.rs** (2409 lines): `CompletionReason` enum - Completed, Failed, Mercy. Why the activity ended.

- **itemtype.rs** (4127 lines): `ItemType` and `ItemSubType` enums. Categorizes weapons and equipment (Kinetic, Energy, Heavy, weapon subtypes like HandCannon, SniperRifle, etc.)

- **medaltier.rs** (1861 lines): `MedalTier` enum - Bronze, Silver, Gold, Unknown. Medal rarity classification.

- **stat.rs** (4328 lines): `Stat` enum representing different statistic types tracked by Bungie API.

- **weaponsort.rs** (2158 lines): `WeaponSort` enum for sorting weapon statistics by various criteria.

**Common Pattern**:
```rust
impl Mode {
    pub fn from_id(id: u32) -> Result<Mode> { /* ... */ }
    pub fn as_id(&self) -> u32 { /* ... */ }
}
```

All enums provide bidirectional conversion between Bungie API numeric IDs and strongly-typed enum variants.

### Things to Know

**Mode Enum Complexity**: The Mode enum is the most complex with 70+ variants and sophisticated logic for compound modes. Understanding this enum is critical for filtering queries correctly.

**Competitive Modes**: Several competitive mode variants exist: `PvPCompetitive`, `ClashCompetitive`, `CollisionCompetitive`, `RiftCompetitive`, `ShowdownCompetitive`, `SurvivalCompetitive`. These are used in conjunction with activity hash filtering for Season 25+ data.

**CharacterClassSelection vs CharacterClass**: `CharacterClass` represents actual in-game classes. `CharacterClassSelection` adds query-specific options (All characters, LastActive character) not valid in game but useful for stat aggregation.

**DateTimePeriod Construction**: `DateTimePeriod::with_start_end_time()` creates custom time ranges. Predefined periods (Daily, Weekly) use Destiny 2 reset schedule (Tuesday 17:00 UTC).

**Serde Integration**: Most enums derive Serialize/Deserialize for JSON compatibility with Bungie API responses. Custom deserializers handle API inconsistencies.

**Error Handling**: `from_id()` methods return `Result<T, Error>` to handle unknown/invalid IDs from API. Important for forward compatibility when Bungie adds new modes/platforms.

**Mode Groups**: Modes can belong to multiple categories. Private match modes often include both the private variant and the base mode (e.g., PrivateMatchesControl includes Control mode).

Created and maintained by Nori.
