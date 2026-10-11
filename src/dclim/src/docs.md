# Noridoc: dclim Source

Path: @/dcli/src/dclim/src

### Overview

Source implementation of dclim command-line tool for managing the Destiny 2 manifest database. Handles downloading, extracting, and versioning of the manifest.

### How it fits into the larger codebase

Implementation of dclim CLI tool declared in @/dcli/src/dclim/Cargo.toml. Manages the manifest.sqlite3 file that @/dcli/src/dcli `ManifestInterface` queries. Essential setup step before using other dcli tools.

### Core Implementation

**main.rs** (10942 lines): Manifest download orchestration and CLI logic.

**manifest_info.rs** (1960 lines): Manifest metadata handling - version tracking, URL storage, local manifest info persistence.

**Key Operations**:
1. **Check for Updates**:
   - Call Bungie API `/Platform/Destiny2/Manifest/`
   - Parse ManifestResponse for version and URL
   - Compare against local manifest_info.json
   - Determine if download needed

2. **Download Manifest**:
   - HTTP GET manifest ZIP from Bungie CDN
   - Stream download with progress indication
   - Save ZIP to temp location

3. **Extract and Install**:
   - Unzip manifest (single SQLite file inside)
   - Replace existing manifest.sqlite3
   - Update manifest_info.json with new version/URL

4. **Version Management**:
   - Store current manifest version in manifest_info.json
   - Format: `{"version": "123.45.67.890", "url": "/common/destiny2_content/..."}`
   - Used for update checking on subsequent runs

**Data Flow**:
```
Bungie API → ManifestResponse → Parse URL
   ↓
Download ZIP → Stream to disk → Extract SQLite
   ↓
Save manifest.sqlite3 → Update manifest_info.json
```

### Things to Know

**Manifest Structure**: ZIP contains single SQLite file. File size typically 20-100MB compressed, larger uncompressed.

**Manifest Location**: Default is `{system_data_dir}/dcli/manifest.sqlite3`. Same location used by all dcli tools. Must match for tools to find manifest.

**Version Format**: Bungie's version string format like "123.45.67.890". Exact format may vary but always string comparison for equality check.

**URL Handling**: Manifest URL from API may be relative or absolute. Code checks for "http" prefix and prepends "https://www.bungie.net" if needed.

**Progress Display**: Uses indicatif or similar for download progress bars. Shows bytes downloaded and percentage complete.

**Manifest Info JSON**: Simple JSON file storing version and URL. Used as local cache to avoid re-downloading same manifest version. Not part of manifest.sqlite3 itself.

**Error Recovery**: If download or extraction fails, preserves existing manifest if present. Errors don't delete working manifest.

**Atomic Updates**: Downloads to temp location then moves/copies to final destination. Reduces risk of corrupting manifest during failed update.

Created and maintained by Nori.
