# Noridoc: Manifest Module

Path: @/dcli/src/dcli/src/manifest

### Overview

Data structures representing Destiny 2 manifest definitions. The manifest database contains JSON blobs defining every item, activity, stat, and game element in Destiny 2.

### How it fits into the larger codebase

Manifest types are deserialized from JSON stored in the manifest.sqlite3 database. Queried by @/dcli/src/dcli/src/manifestinterface.rs when resolving hashes to human-readable names. Used throughout the codebase to display weapon names, map names, medal names, etc., instead of opaque hash values.

### Core Implementation

**mod.rs**: Module declarations and re-exports.

**definitions.rs** (2942 lines): Core manifest definition structs:

- **InventoryItemDefinitionData**: Weapon and item definitions
  - Fields: hash, name, description, icon path, item type, item sub type, tier, rarity
  - Used to resolve weapon hashes to names like "Ace of Spades" or "Igneous Hammer"

- **ActivityDefinitionData**: Activity/map definitions
  - Fields: hash, name, description, activity type hash, place hash, destination hash
  - Used to resolve activity hashes to map names like "Endless Vale" or "Javelin-4"

- **ActivityTypeDefinitionData**: Activity type categories (Crucible, Strike, Raid, etc.)

- **PlaceDefinitionData**: Location/place names

- **DestinationDefinitionData**: Destination names (planets, locations)

- **HistoricalStatsDefinition**: Stat type definitions (what stat IDs mean)

- **DisplayPropertiesData**: Common display properties (name, description, icon)
  - Nested in many definition types for UI presentation data

**JSON Structure**: Bungie manifest tables store JSON blobs in a `json` column. Each row is keyed by signed 64-bit ID (converted from unsigned 32-bit hash). JSON structure varies by table type.

**Common Pattern**:
```rust
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItemDefinitionData {
    pub hash: u32,
    pub display_properties: DisplayPropertiesData,
    pub item_type: u32,
    pub item_sub_type: u32,
    // ...
}
```

### Things to Know

**Hash to ID Conversion**: Bungie API uses unsigned 32-bit hashes. Manifest database uses signed 64-bit IDs. Conversion in @/dcli/src/dcli/src/manifestinterface.rs:44-52 handles this mismatch via bit manipulation.

**Nested Display Properties**: Most definitions have `display_properties` field containing name, description, icon. This is the primary way to get human-readable strings.

**Incomplete Definitions**: Not all hash values have manifest definitions. Some are procedurally generated or deprecated. Code must handle missing definitions gracefully.

**Manifest Updates**: When Bungie releases new content, manifest is updated. Old hash values may be removed or definitions changed. Important to keep manifest synced via @/dcli/src/dclim.

**Item Type Taxonomy**: `item_type` and `item_sub_type` use numeric codes defined in @/dcli/src/dcli/src/enums/itemtype.rs. Example: itemType=3 is "Weapon", itemSubType=9 is "Hand Cannon".

**Localization**: Manifest database can contain multiple language versions. dcli uses English locale (`en` path in manifest URLs and queries).

Created and maintained by Nori.
