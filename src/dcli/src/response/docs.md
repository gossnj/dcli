# Noridoc: Response Module

Path: @/dcli/src/dcli/src/response

### Overview

Serde-deserializable structs matching Bungie API JSON response formats. These types provide strongly-typed representations of API responses for activities, characters, stats, and manifest data.

### How it fits into the larger codebase

Response structs are the bridge between raw JSON from Bungie API and typed Rust data structures. Used by @/dcli/src/dcli/src/apiinterface.rs when parsing API responses. Data from response structs is extracted and transformed into application-level types (like `CrucibleActivity`, `CrucibleStats`) defined in @/dcli/src/dcli/src/crucible.rs before being stored in the database or returned to callers.

### Core Implementation

**mod.rs**: Declares all response modules and re-exports types.

**Key Response Modules**:

- **pgcr.rs** (10496 lines): `PGCRResponse` and `DestinyPostGameCarnageReportData`. Post Game Carnage Report containing detailed match data. Includes:
  - `DestinyPostGameCarnageReportEntry`: Per-player performance
  - `DestinyHistoricalStatsValue`: Individual stat values
  - `DestinyPostGameCarnageReportActivityDetails`: Match metadata (mode, map, etc.)
  - Team results, player loadouts, weapon stats, medals

- **activities.rs** (5438 lines): `ActivitiesResponse` and `Activity`. List of activities from activity history endpoint. Contains activity IDs, dates, modes for pagination through player history.

- **stats.rs** (10687 lines): `AllTimePvPStatsResponse`, `DailyPvPStatsResponse`, `PvpStatsData`, `DailyPvPStatsValuesData`. Aggregated statistics from Bungie's stat endpoints.

- **character.rs** (2919 lines): `CharacterData` and `GetCharacterResponse`. Character information including class, light level, emblem, playtime.

- **gpr.rs** (4289 lines): `GetProfileResponse` and `CharacterActivitiesData`. Profile-level data including all characters and their current activities.

- **cr.rs** (1923 lines): `GetCharacterResponse`. Detailed character data from character endpoint.

- **sdpr.rs** (2578 lines): `SearchDestinyPlayerResponse`, `LinkedProfilesResponse`, `SearchDestinyPlayerPostData`. Player search and profile linking responses.

- **ggms.rs** (1950 lines): `GetGroupMemberResponse`, `GroupMemberResponse`. Clan/group membership data.

- **gmd.rs** (686 lines): `GetMembershipData`, `UserMembershipData`. User membership across platforms.

- **manifest.rs** (2005 lines): `ManifestResponse`. Manifest metadata including version and download URLs.

- **drs.rs** (2304 lines): `DestinyResponseStatus`, common response wrapper types. Includes `API_RESPONSE_STATUS_SUCCESS` constant.

- **utils.rs** (5908 lines): Shared response utilities and helper types.

**Common Pattern**:
```rust
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DestinyPostGameCarnageReportData {
    pub period: String,
    pub activity_details: ActivityDetails,
    pub entries: Vec<DestinyPostGameCarnageReportEntry>,
    // ...
}
```

All response structs use serde derives with `rename_all = "camelCase"` to match Bungie's JSON field naming.

### Things to Know

**Nested Structure**: Bungie API responses are deeply nested. Response types mirror this structure exactly. Example: `PGCRResponse.response.entries[].extended.values.kills.basic.value`.

**Option Fields Everywhere**: Most fields are `Option<T>` because Bungie API returns null for many fields depending on context (privacy settings, activity type, etc.). Code must handle missing data gracefully.

**Custom Deserializers**: Some types have custom serde deserializers to handle API quirks - numeric strings, inconsistent null vs missing fields, union types.

**DestinyHistoricalStatsValue Pattern**: Stats in PGCR use complex nested structure: `{ basic: { value: f32, displayValue: String }, pga: { value: f32 } }`. Repeated for every stat field.

**Activity History Pagination**: `ActivitiesResponse` includes activity IDs but not full match details. Must make separate PGCR requests for each activity to get detailed data. This is why syncing is slow - one API call per match.

**MAX_ACTIVITIES_REQUEST_COUNT**: Constant in activities.rs defining max activities per page (250). Used for pagination loops.

**Platform and Mode IDs**: Response structs use raw numeric IDs (u32, i32). These are converted to enum types (Platform, Mode, etc.) during processing in apiinterface.rs or activitystoreinterface.rs.

**Error Responses**: Bungie API wraps data in `{ Response: T, ErrorCode: i32, ErrorStatus: String }` structure. Response types handle this outer wrapper and check error codes.

**Privacy Impact**: Fields like player names, activity details can be null if privacy settings restrict access. Response parsing must handle these gracefully without panicking.

**Hash Fields**: Many fields are "hash" values (u32) that reference manifest definitions. Examples: `activityHash`, `itemHash`, `statHash`. These are looked up in manifest database via @/dcli/src/dcli/src/manifestinterface.rs.

Created and maintained by Nori.
