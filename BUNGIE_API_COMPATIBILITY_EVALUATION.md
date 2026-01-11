# Bungie API Compatibility Evaluation for dcli

**Evaluation Date:** January 2026
**Library Version:** 0.99.9 (Last updated: November 2024)
**Bungie API Version:** 2.21.8+

## Executive Summary

The dcli library is a mature, well-designed Rust toolkit for Destiny 2 data analysis, primarily focused on Crucible/PvP statistics. However, significant changes to the Bungie API and Destiny 2's content structure since November 2024 present several opportunities for improvement. This document outlines specific areas where the library could be enhanced to better align with current API capabilities.

---

## 1. Missing Content Periods (Seasons/Episodes)

### Current State
The `Moment` enum in `src/dcli/src/enums/moment.rs` tracks Destiny 2 seasons through **Season of the Wish** (November 2023). This is significantly outdated.

### Missing Content Periods

#### The Final Shape Era (Year 7 - 2024)
| Content | Release Date | Status |
|---------|--------------|--------|
| **The Final Shape** | June 4, 2024 | Missing |
| **Episode: Echoes** (Acts 1-3) | June 11, 2024 | Missing |
| **Episode: Revenant** (Acts 1-3) | October 8, 2024 | Missing |
| **Episode: Heresy** (Acts 1-3) | February 4, 2025 | Missing |

#### The Fate Saga (Year 8 - 2025+)
| Content | Release Date | Status |
|---------|--------------|--------|
| **The Edge of Fate** | July 15, 2025 | Missing |
| **Renegades** | December 2025 | Missing |

### Recommendation
Update `Moment` enum to include:
```rust
// The Final Shape Era
TheFinalShape,
EpisodeEchoes,
EpisodeRevenant,
EpisodeHeresy,

// Fate Saga
TheEdgeOfFate,
Renegades,
```

**Note:** Bungie transitioned from the seasonal model (4 seasons/year) to an episodic model (3 episodes/year in Y7) and now to a bi-annual expansion model (2 expansions/year in Y8+). The library may benefit from abstracting this into a more flexible "content period" system.

---

## 2. Activity Mode Updates

### Current State
The `Mode` enum in `src/dcli/src/enums/mode.rs` includes modes up to `Relic` (ID 92) from the official API, plus custom dcli-defined modes (IDs 700-801) for competitive variants and Iron Banner modes.

### Potentially Missing Official Modes

Based on API documentation, these modes may need verification:
- **Master/Legend Lost Sectors** - Enhanced Lost Sector tracking
- **New Crucible modes** - Any new PvP modes introduced with Episodes/Expansions
- **Portal activities** - New activity type mentioned in recent API updates

### Recommended Actions
1. Verify current `DestinyActivityModeType` enum values against latest API schema
2. Add any new official mode IDs
3. Consider adding modes for new Episode-specific activities

---

## 3. New API Fields and Data Structures

### 3.1 Post-Game Carnage Report Enhancements

**New Field: `scoreboardValues`**
Added to `DestinyPostGameCarnageReportExtendedData` (August 2025):
- Crucible player scores
- Reward scores
- Multipliers
- Additional scoring details

**Current Library State:** Not implemented

**Recommendation:** Update `DestinyPostGameCarnageReportExtendedData` struct in `src/dcli/src/response/pgcr.rs`:
```rust
pub struct DestinyPostGameCarnageReportExtendedData {
    pub values: HashMap<String, DestinyHistoricalStatsValue>,
    pub weapons: Option<Vec<DestinyHistoricalWeaponStats>>,
    #[serde(rename = "scoreboardValues")]
    pub scoreboard_values: Option<HashMap<String, DestinyHistoricalStatsValue>>, // NEW
}
```

### 3.2 PGCR Difficulty and Skull Data

**New Fields in `DestinyPostGameCarnageReportData`:**
- `difficultyTierIndex` - Difficulty tier of the activity
- `selectedSkullHashes` - Active modifiers/skulls during activity

**Recommendation:** Update PGCR response structures to capture this data.

### 3.3 Activity Definition Enhancements

**New Fields in `DestinyActivityDefinition`:**
- `curatorBlockDefinition` - Curator-related data
- `durationEstimate` - Estimated activity duration
- Activity families, traits, difficulty tiers
- Selectable skull collections

**Impact:** May improve activity classification and filtering.

### 3.4 Item Definition Enhancements

**New Fields:**
- `isHolofoil` - Holofoil item variant flag
- `isAdept` - Adept weapon indicator
- `gearTier` - Gear tier classification

**Impact:** Could enhance weapon usage statistics reporting.

### 3.5 Character Activities Component

**New Fields:**
- `difficultyTierCollections`
- `selectableSkullCollections`

**Impact:** Better current activity tracking in `dclia`.

### 3.6 Season Pass Changes

**New Fields:**
- `seasonPassList` - Supports multiple reward passes per season
- `currentSeasonPassHash` - Current active season pass
- `seasonPassHashes` on player profiles

**Impact:** Season pass tracking if desired feature.

---

## 4. API Endpoint Changes

### 4.1 Player Search API Enhancement

**New Feature:** POST-based player name search with special character support.

The current library uses `SearchDestinyPlayerByBungieName` which works, but newer endpoints support searching for names containing colons, slashes, angle brackets, etc.

### 4.2 Removed APIs

**Fireteam Finder API:** Specs removed for 3rd-party apps. This doesn't affect dcli as it wasn't using these endpoints.

### 4.3 Rate Limiting

**New Rate Limits:**
- `InsertSocketPlugFree` API: 2 socket actions per-second, per-user

Not directly relevant to dcli's read-only use case.

---

## 5. Dependency Updates

### Current Dependencies (from Cargo.toml)
| Dependency | Current Version | Latest Stable | Update Priority |
|------------|-----------------|---------------|-----------------|
| reqwest | 0.11.12 | 0.12.x | Medium |
| sqlx | 0.6.2 | 0.8.x | Medium |
| serde | 1.0.147 | 1.0.x | Low |
| chrono | 0.4.23 | 0.4.x | Low |
| tokio | (via sqlx) | Latest | Medium |

### Recommendations
1. **reqwest 0.12.x** - Includes HTTP/3 support, improved performance
2. **sqlx 0.8.x** - Better compile-time checking, new features
3. Consider migrating from **structopt** to **clap v4** (structopt is deprecated)

---

## 6. Architecture Considerations

### 6.1 Episode/Act Model

The current `Moment` enum assumes a linear season progression. The new episodic content model (Episodes with Acts) may benefit from:
- Hierarchical time period representation
- Act-level granularity for filtering

### 6.2 Flexible Mode Handling

The library uses custom mode IDs (700+) for modes not in the official API. Consider:
- A more robust mapping system for mode aliases
- Dynamic mode discovery from manifest data

### 6.3 Error Handling for New Fields

New API fields are often added as optional. The library's serde configuration handles this well with `#[serde(default)]`, but explicit handling of new optional fields would improve data completeness.

---

## 7. Priority Recommendations

### High Priority
1. **Add missing content periods** - TheFinalShape through Renegades
2. **Add `scoreboardValues` to PGCR** - Valuable Crucible scoring data
3. **Verify and update Mode enum** - Ensure all current modes are supported

### Medium Priority
4. **Add PGCR difficulty/skull data** - Useful for activity analysis
5. **Update dependencies** - reqwest, sqlx, migrate from structopt
6. **Add `isAdept`/`isHolofoil` to weapon data** - Enhanced weapon tracking

### Low Priority
7. **Season pass tracking fields** - If this becomes a desired feature
8. **Activity duration estimates** - Nice-to-have for UX
9. **Episode/Act hierarchical model** - Architectural enhancement

---

## 8. Testing Recommendations

After implementing updates:
1. Verify PGCR parsing with recent match data
2. Test activity filtering for Episode-era activities
3. Validate mode recognition for any new PvP modes
4. Ensure manifest sync captures new definition types

---

## References

- [Bungie.net API GitHub Repository](https://github.com/Bungie-net/api)
- [Bungie.net API Documentation](https://bungie-net.github.io/)
- [Light.gg API Update Tracker](https://www.light.gg/db/changes/)
- [Destiny 2 Post-Release Content - Wikipedia](https://en.wikipedia.org/wiki/Destiny_2_post-release_content)
- [Years of Destiny - Bungie Help](https://help.bungie.net/hc/en-us/articles/4408041224852-Years-of-Destiny)

---

## Conclusion

The dcli library has a solid foundation but requires updates to remain compatible with the evolving Destiny 2 API and content structure. The most impactful changes are:

1. Adding missing content periods (Episodes, new expansions)
2. Incorporating new PGCR scoring data
3. Updating dependencies for security and performance

These changes would bring the library up to date with the current state of Destiny 2 (Year 8, Fate Saga) and leverage new API capabilities for enhanced player statistics analysis.
