# Noridoc: dclim CLI Tool

Path: @/dcli/src/dclim

### Overview

Command-line tool for managing the Destiny 2 manifest database. Downloads the latest manifest from Bungie's servers, extracts it, and saves it to the local data directory. The manifest contains definitions for all game items, activities, and statistics.

### How it fits into the larger codebase

dclim manages the manifest database that @/dcli/src/dcli `ManifestInterface` queries. Both dclisync and dcliah require the manifest to resolve item names, activity names, and other game definitions. The FFI layer at @/dcli/src/dcli-ffi exposes manifest management via `dcli_manifest_needs_update()` and `dcli_manifest_download()` for the Swift app at @/Last Banner to handle manifest updates.

### Core Implementation

**Main File** (@/dcli/src/dclim/src/main.rs, 10942 lines): Manifest download orchestration.

**Helper Module** (@/dcli/src/dclim/src/manifest_info.rs, 1960 lines): Manifest metadata handling.

**Operations**:
1. Call Bungie API endpoint `/Platform/Destiny2/Manifest/`
2. Parse response to get latest manifest version and download URL
3. Check local manifest version (stored in manifest_info.json)
4. If outdated or missing, download manifest ZIP file
5. Extract SQLite database from ZIP
6. Save as manifest.sqlite3 in data directory
7. Save metadata to manifest_info.json (version, URL)

**Manifest Location**: Default system data directory (e.g., `~/Library/Application Support/dcli/manifest.sqlite3` on macOS). Can be overridden with command-line option.

**Manifest Structure**: The manifest.sqlite3 is a read-only SQLite database provided by Bungie containing tables like DestinyInventoryItemDefinition, DestinyActivityDefinition, DestinyHistoricalStatsDefinition, etc. Each table stores JSON blobs indexed by hash IDs.

### Things to Know

**Version Checking**: Compares local manifest_info.json URL against remote manifest URL. Different URL means new manifest version available.

**Manifest Updates**: Bungie periodically updates the manifest when game content changes (new seasons, patches, etc.). Should run dclim periodically to stay current.

**API Key Requirement**: Like all dcli tools, requires DESTINY_API_KEY at compile time for API access.

**File Permissions**: Downloads to user data directory, no special permissions needed. Manifest file size typically 20-100 MB compressed, larger uncompressed.

**Automatic Extraction**: Handles ZIP extraction internally. Manifest comes as ZIP from Bungie with single SQLite file inside.

**Critical Dependency**: Without manifest, other dcli tools can store and query activity data but cannot resolve human-readable names for weapons, maps, medals, etc. Manifest required for meaningful output.

**Hash to ID Conversion**: Manifest uses signed 64-bit IDs, but Bungie API uses unsigned 32-bit hashes. Conversion function in @/dcli/src/dcli/src/manifestinterface.rs:44-52 handles this discrepancy.

Created and maintained by Nori.
