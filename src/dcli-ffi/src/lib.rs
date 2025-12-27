/*
* Copyright 2023 Mike Chambers
* https://github.com/mikechambers/dcli
*
* Permission is hereby granted, free of charge, to any person obtaining a copy of
* this software and associated documentation files (the "Software"), to deal in
* the Software without restriction, including without limitation the rights to
* use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
* of the Software, and to permit persons to whom the Software is furnished to do
* so, subject to the following conditions:
*
* The above copyright notice and this permission notice shall be included in all
* copies or substantial portions of the Software.
*
* THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
* IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
* FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
* COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
* IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
* CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

use log::{info, error, debug, LevelFilter};

#[cfg(any(target_os = "ios", target_os = "macos"))]
use oslog::OsLogger;

/// Initialize logging for Apple platforms. Safe to call multiple times.
/// Filters out verbose sqlx query logging to improve performance.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn init_apple_logging() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        OsLogger::new("com.ottercreeksoftware.Last-Banner.dcli")
            .level_filter(LevelFilter::Debug)
            .category_level_filter("sqlx", LevelFilter::Warn)  // Silence sqlx query spam
            .init()
            .ok();  // Ignore errors if already initialized
    });
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn init_apple_logging() {
    // No-op on non-Apple platforms
}

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use dcli::apiinterface::ApiInterface;
use dcli::enums::platform::Platform;
use dcli::enums::mode::Mode;
use dcli::crucible::PlayerName;
use dcli::activitystoreinterface::ActivityStoreInterface;
use dcli::manifestinterface::ManifestInterface;
use dcli::enums::character::CharacterClassSelection;
use dcli::enums::moment::DateTimePeriod;
use chrono::{Utc, Duration};

/// Opaque pointer to ApiInterface
pub struct DcliApiClient {
    client: ApiInterface,
    runtime: tokio::runtime::Runtime,
}

/// Represents a Crucible stats result from the API
#[repr(C)]
pub struct DcliCrucibleStats {
    pub activities_entered: f32,
    pub activities_won: f32,
    pub activities_lost: f32,
    pub assists: f32,
    pub kills: f32,
    pub average_kill_distance: f32,
    pub seconds_played: f32,
    pub deaths: f32,
    pub average_lifespan: f32,
    pub opponents_defeated: f32,
    pub efficiency: f32,
    pub kills_deaths_ratio: f32,
    pub kills_deaths_assists: f32,
    pub suicides: f32,
    pub precision_kills: f32,
}

/// Represents a Destiny character
#[repr(C)]
pub struct DcliCharacter {
    pub id: i64,
    pub class_type: i32,
    pub minutes_played_total: i64,
}

/// Creates a new API client with the given Bungie API key
/// Returns null on error
/// Caller must call dcli_client_free when done
#[no_mangle]
pub extern "C" fn dcli_client_new(api_key: *const c_char) -> *mut DcliApiClient {
    init_apple_logging();

    if api_key.is_null() {
        return std::ptr::null_mut();
    }

    let key = unsafe {
        match CStr::from_ptr(api_key).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let client = match ApiInterface::new_with_key(key) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    Box::into_raw(Box::new(DcliApiClient { client, runtime }))
}

/// Frees a client created with dcli_client_new
#[no_mangle]
pub extern "C" fn dcli_client_free(client: *mut DcliApiClient) {
    if !client.is_null() {
        unsafe {
            drop(Box::from_raw(client));
        }
    }
}

/// Searches for a player by Bungie name and returns their member ID
/// Returns true on success, false on error
/// Format: "PlayerName#1234"
#[no_mangle]
pub extern "C" fn dcli_search_player(
    client: *mut DcliApiClient,
    bungie_name: *const c_char,
    out_member_id: *mut i64,
    out_platform: *mut i32,
) -> bool {
    if client.is_null() || bungie_name.is_null() || out_member_id.is_null() || out_platform.is_null() {
        return false;
    }

    let client = unsafe { &mut *client };

    let name_str = unsafe {
        match CStr::from_ptr(bungie_name).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    // Parse the Bungie name
    let player_name = PlayerName::from_bungie_name(name_str);

    // Validate it's a proper Bungie name
    if !player_name.is_valid_bungie_name() {
        return false;
    }

    // Call the API
    let result = client.runtime.block_on(async {
        client.client.search_destiny_player(&player_name).await
    });

    match result {
        Ok(user_info) => {
            unsafe {
                *out_member_id = user_info.membership_id;
                *out_platform = user_info.membership_type.as_id() as i32;
            }
            true
        }
        Err(_) => false,
    }
}

/// Retrieves all-time Crucible stats for a character
/// Returns true on success, false on error
#[no_mangle]
pub extern "C" fn dcli_get_crucible_stats(
    client: *mut DcliApiClient,
    member_id: i64,
    character_id: i64,
    platform: i32,
    mode: i32,
    out_stats: *mut DcliCrucibleStats,
) -> bool {
    if client.is_null() || out_stats.is_null() {
        return false;
    }

    let client = unsafe { &mut *client };

    let platform = Platform::from_id(platform as u32);
    let mode = match Mode::from_id(mode as u32) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let result = client.runtime.block_on(async {
        client.client.retrieve_alltime_crucible_stats(
            &member_id,
            &character_id,
            &platform,
            &mode,
        ).await
    });

    match result {
        Ok(Some(stats)) => {
            let dcli_stats = DcliCrucibleStats {
                activities_entered: stats.activities_entered,
                activities_won: stats.activities_won,
                activities_lost: stats.get_activities_lost(),
                assists: stats.assists,
                kills: stats.kills,
                average_kill_distance: stats.average_kill_distance,
                seconds_played: stats.seconds_played,
                deaths: stats.deaths,
                average_lifespan: stats.average_lifespan,
                opponents_defeated: stats.opponents_defeated,
                efficiency: stats.efficiency,
                kills_deaths_ratio: stats.kills_deaths_ratio,
                kills_deaths_assists: stats.kills_deaths_assists,
                suicides: stats.suicides,
                precision_kills: stats.precision_kills,
            };

            unsafe {
                *out_stats = dcli_stats;
            }
            true
        }
        _ => false,
    }
}

/// Retrieves the list of characters for a player
/// Returns the number of characters found, 0 on error
/// out_characters must be an array of at least 3 DcliCharacter structs
#[no_mangle]
pub extern "C" fn dcli_get_characters(
    client: *mut DcliApiClient,
    member_id: i64,
    platform: i32,
    out_characters: *mut DcliCharacter,
    max_characters: i32,
) -> i32 {
    if client.is_null() || out_characters.is_null() || max_characters < 1 {
        return 0;
    }

    let client = unsafe { &mut *client };
    let platform = Platform::from_id(platform as u32);

    let result = client.runtime.block_on(async {
        client.client.retrieve_characters(&member_id, &platform).await
    });

    match result {
        Ok(Some(characters)) => {
            let chars = characters.characters;
            let count = chars.len().min(max_characters as usize);

            for (i, character) in chars.iter().take(count).enumerate() {
                let dcli_char = DcliCharacter {
                    id: character.id,
                    class_type: character.class_type.as_id() as i32,
                    minutes_played_total: character.minutes_played_total as i64,
                };

                unsafe {
                    *out_characters.offset(i as isize) = dcli_char;
                }
            }

            count as i32
        }
        _ => 0,
    }
}

// MARK: - Activity Store Interface

/// Opaque pointer to ActivityStoreInterface
pub struct DcliActivityStore {
    store: ActivityStoreInterface,
    manifest: ManifestInterface,
    runtime: tokio::runtime::Runtime,
}

/// Progress callback type for sync operations
/// Parameters: message (const char*), current (u32), total (u32), user_data (void*)
pub type ProgressCallback = extern "C" fn(*const c_char, u32, u32, *mut std::ffi::c_void);

/// Creates a new activity store with the given data directory
/// Returns null on error
/// Caller must call dcli_store_free when done
#[no_mangle]
pub extern "C" fn dcli_store_init(
    data_dir: *const c_char,
) -> *mut DcliActivityStore {
    init_apple_logging();

    if data_dir.is_null() {
        return std::ptr::null_mut();
    }

    let dir = unsafe {
        match CStr::from_ptr(data_dir).to_str() {
            Ok(s) => PathBuf::from(s),
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return std::ptr::null_mut(),
    };

    // Initialize store and manifest
    let result = runtime.block_on(async {
        let store = ActivityStoreInterface::init_with_path(&dir, None).await;
        let manifest = ManifestInterface::new(&dir, false).await;

        match (store, manifest) {
            (Ok(s), Ok(m)) => Some((s, m)),
            _ => None,
        }
    });

    match result {
        Some((store, manifest)) => {
            Box::into_raw(Box::new(DcliActivityStore {
                store,
                manifest,
                runtime,
            }))
        }
        None => std::ptr::null_mut(),
    }
}

/// Frees a store created with dcli_store_init
#[no_mangle]
pub extern "C" fn dcli_store_free(store: *mut DcliActivityStore) {
    if !store.is_null() {
        unsafe {
            drop(Box::from_raw(store));
        }
    }
}

// Note: Manifest syncing is handled separately via dclim tool
// The manifest database must be downloaded before using the store

/// Adds a player to the sync list by Bungie name
/// Returns true on success, false on error
#[no_mangle]
pub extern "C" fn dcli_store_add_player(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
) -> bool {
    if store.is_null() || bungie_name.is_null() {
        return false;
    }

    let name_str = unsafe {
        match CStr::from_ptr(bungie_name).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    let player_name = PlayerName::from_bungie_name(name_str);
    if !player_name.is_valid_bungie_name() {
        return false;
    }

    unsafe {
        let store_ptr = store as *mut DcliActivityStore;
        let runtime = &mut (*store_ptr).runtime;
        let store_ref = &mut (*store_ptr).store;

        runtime.block_on(async {
            match store_ref.add_player_to_sync(&player_name).await {
                Ok(_) => true,
                Err(_) => false,
            }
        })
    }
}

/// Removes a player from the sync list by Bungie name
/// Returns true on success, false on error
#[no_mangle]
pub extern "C" fn dcli_store_remove_player(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
) -> bool {
    if store.is_null() || bungie_name.is_null() {
        return false;
    }

    let name_str = unsafe {
        match CStr::from_ptr(bungie_name).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    let player_name = PlayerName::from_bungie_name(name_str);
    if !player_name.is_valid_bungie_name() {
        return false;
    }

    unsafe {
        let store_ptr = store as *mut DcliActivityStore;
        let runtime = &mut (*store_ptr).runtime;
        let store_ref = &mut (*store_ptr).store;

        runtime.block_on(async {
            match store_ref.remove_player_from_sync(&player_name).await {
                Ok(_) => true,
                Err(_) => false,
            }
        })
    }
}

/// Syncs a player's activities from the API by Bungie name
/// Returns true on success, false on error
/// Optionally accepts a progress callback for progress updates
#[no_mangle]
pub extern "C" fn dcli_store_sync_player(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
) -> bool {
    dcli_store_sync_player_with_progress(store, bungie_name, None, std::ptr::null_mut())
}

/// Syncs a player's activities from the API by Bungie name with progress callback
/// Returns true on success, false on error
/// callback: Progress callback function pointer (can be null for no progress)
/// user_data: User data pointer passed to callback
#[no_mangle]
pub extern "C" fn dcli_store_sync_player_with_progress(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
    callback: Option<ProgressCallback>,
    user_data: *mut std::ffi::c_void,
) -> bool {
    if store.is_null() || bungie_name.is_null() {
        return false;
    }

    let name_str = unsafe {
        match CStr::from_ptr(bungie_name).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    let player_name = PlayerName::from_bungie_name(name_str);
    if !player_name.is_valid_bungie_name() {
        return false;
    }

    // Helper to send progress updates
    let send_progress = |message: &str, current: u32, total: u32| {
        if let Some(cb) = callback {
            if let Ok(c_msg) = CString::new(message) {
                cb(c_msg.as_ptr(), current, total, user_data);
            }
        }
    };

    unsafe {
        let store_ptr = store as *mut DcliActivityStore;
        let runtime = &mut (*store_ptr).runtime;
        let store_ref = &mut (*store_ptr).store;

        runtime.block_on(async {
            info!("Starting sync for player: {}", name_str);
            send_progress("Initializing sync...", 0, 100);

            match store_ref.sync_player(&player_name).await {
                Ok(_) => {
                    info!("Successfully synced player: {}", name_str);
                    send_progress("Sync complete!", 100, 100);
                    true
                },
                Err(e) => {
                    error!("Failed to sync player {}: {:?}", name_str, e);
                    send_progress(&format!("Sync failed: {:?}", e), 0, 100);
                    false
                }
            }
        })
    }
}

/// Gets Crucible stats from the local database for a character
/// Uses the `all_time` time period
/// Returns true on success, false on error
#[no_mangle]
pub extern "C" fn dcli_store_get_crucible_stats(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
    character_class: i32,  // 0=Titan, 1=Hunter, 2=Warlock, 4=All
    mode: i32,
    out_stats: *mut DcliCrucibleStats,
) -> bool {
    if store.is_null() || bungie_name.is_null() || out_stats.is_null() {
        return false;
    }

    let name_str = unsafe {
        match CStr::from_ptr(bungie_name).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    let player_name = PlayerName::from_bungie_name(name_str);
    if !player_name.is_valid_bungie_name() {
        return false;
    }

    let mode = match Mode::from_id(mode as u32) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let character_selection = match character_class {
        0 => CharacterClassSelection::Titan,
        1 => CharacterClassSelection::Hunter,
        2 => CharacterClassSelection::Warlock,
        3 => CharacterClassSelection::LastActive,
        _ => CharacterClassSelection::All,
    };

    let result = unsafe {
        let store_ptr = store as *mut DcliActivityStore;
        let runtime = &mut (*store_ptr).runtime;
        let store_ref = &mut (*store_ptr).store;

        runtime.block_on(async {
            // First find the member in the database
            let member = match store_ref.find_member(&player_name, false).await {
                Ok(m) => m,
                Err(_) => return None,
            };

            // Get the all-time time period (past 10 years)
            let time_period = match DateTimePeriod::with_start_end_time(
                Utc::now() - Duration::days(3650),
                Utc::now(),
            ) {
                Ok(tp) => tp,
                Err(_) => return None,
            };

            // Query the database for activity summary
            let summary = match store_ref.retrieve_activities_summary(
                &member,
                &character_selection,
                &mode,
                &time_period,
            ).await {
                Ok(Some(s)) => s,
                _ => return None,
            };

            // Convert PlayerActivitiesSummary to DcliCrucibleStats
            let total_activities = summary.total_activities as f32;
            let wins = summary.wins as f32;
            let losses = (summary.total_activities - summary.wins) as f32;

            let kills = summary.kills as f32;
            let deaths = summary.deaths as f32;
            let assists = summary.assists as f32;

            let kd_ratio = if deaths > 0.0 {
                kills / deaths
            } else {
                kills
            };

            let efficiency = if deaths > 0.0 {
                (kills + assists) / deaths
            } else {
                kills + assists
            };

            let kda = if deaths > 0.0 {
                (kills + assists) / deaths
            } else {
                kills + assists
            };

            // Calculate average kill distance - not available in summary, so we estimate
            let avg_kill_distance = 0.0;
            let avg_lifespan = if deaths > 0.0 && summary.time_played_seconds > 0 {
                (summary.time_played_seconds as f32) / deaths
            } else {
                0.0
            };

            Some(DcliCrucibleStats {
                activities_entered: total_activities,
                activities_won: wins,
                activities_lost: losses,
                assists,
                kills,
                average_kill_distance: avg_kill_distance,
                seconds_played: summary.time_played_seconds as f32,
                deaths,
                average_lifespan: avg_lifespan,
                opponents_defeated: summary.opponents_defeated as f32,
                efficiency,
                kills_deaths_ratio: kd_ratio,
                kills_deaths_assists: kda,
                suicides: 0.0, // Not in summary
                precision_kills: summary.precision as f32,
            })
        })
    };

    match result {
        Some(stats) => {
            unsafe {
                *out_stats = stats;
            }
            true
        }
        None => false,
    }
}

// MARK: - Manifest Management

use dcli::apiclient::ApiClient;
use dcli::response::manifest::ManifestResponse;
use std::fs;

/// Manifest info structure returned to Swift
#[repr(C)]
pub struct DcliManifestInfo {
    pub version_len: usize,
    pub version_ptr: *mut c_char,
    pub url_len: usize,
    pub url_ptr: *mut c_char,
}

/// Checks if a manifest needs updating
/// Returns true if update needed, false otherwise
#[no_mangle]
pub extern "C" fn dcli_manifest_needs_update(
    data_dir: *const c_char,
    out_needs_update: *mut bool,
) -> bool {
    if data_dir.is_null() || out_needs_update.is_null() {
        return false;
    }

    let dir = unsafe {
        match CStr::from_ptr(data_dir).to_str() {
            Ok(s) => PathBuf::from(s),
            Err(_) => return false,
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return false,
    };

    let result = runtime.block_on(async {
        // Get remote manifest info
        let client = match ApiClient::new() {
            Ok(c) => c,
            Err(_) => return None,
        };

        let url = "https://www.bungie.net/Platform/Destiny2/Manifest/";
        let response = match client.call_and_parse::<ManifestResponse>(url).await {
            Ok(r) => r,
            Err(_) => return None,
        };

        let manifest = match &response.response {
            Some(e) => e,
            None => return None,
        };

        let remote_url = &manifest.mobile_world_content_paths.en;

        // Check if local manifest exists and matches
        let manifest_path = dir.join("manifest.sqlite3");
        let manifest_info_path = dir.join("manifest_info.json");

        if !manifest_path.exists() || !manifest_info_path.exists() {
            return Some(true);
        }

        // Read local manifest info
        let json = match fs::read_to_string(&manifest_info_path) {
            Ok(j) => j,
            Err(_) => return Some(true),
        };

        // Parse JSON to get local URL
        let local_info: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(_) => return Some(true),
        };

        let local_url = match local_info.get("url") {
            Some(serde_json::Value::String(s)) => s,
            _ => return Some(true),
        };

        // Compare URLs
        Some(local_url != remote_url)
    });

    match result {
        Some(needs_update) => {
            unsafe {
                *out_needs_update = needs_update;
            }
            true
        }
        None => false,
    }
}

/// Timeout for manifest downloads (2 minutes) - manifest is ~100MB compressed
const MANIFEST_DOWNLOAD_TIMEOUT_SECS: u64 = 120;

/// Downloads and installs the manifest
/// Returns true on success, false on error
#[no_mangle]
pub extern "C" fn dcli_manifest_download(
    data_dir: *const c_char,
    api_key: *const c_char,
) -> bool {
    init_apple_logging();

    if data_dir.is_null() || api_key.is_null() {
        return false;
    }

    let dir = unsafe {
        match CStr::from_ptr(data_dir).to_str() {
            Ok(s) => PathBuf::from(s),
            Err(_) => return false,
        }
    };

    let key = unsafe {
        match CStr::from_ptr(api_key).to_str() {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    // Ensure directory exists
    if let Err(_) = fs::create_dir_all(&dir) {
        return false;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return false,
    };

    let result = runtime.block_on(async {
        // Get manifest info from API (use standard client for quick API call)
        debug!("Creating API client...");
        let client = match ApiClient::new_with_key(key) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create API client: {:?}", e);
                return false;
            }
        };

        debug!("Fetching manifest info from Bungie API...");
        let manifest_url = "https://www.bungie.net/Platform/Destiny2/Manifest/";
        let response = match client.call_and_parse::<ManifestResponse>(manifest_url).await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to fetch manifest info: {:?}", e);
                return false;
            }
        };

        let manifest = match &response.response {
            Some(e) => e,
            None => {
                error!("Empty manifest response");
                return false;
            }
        };

        let download_url = if manifest.mobile_world_content_paths.en.starts_with("http") {
            manifest.mobile_world_content_paths.en.clone()
        } else {
            format!("https://www.bungie.net{}", manifest.mobile_world_content_paths.en)
        };
        let version = &manifest.version;
        info!("Downloading manifest version {} from {} (timeout: {}s)", version, download_url, MANIFEST_DOWNLOAD_TIMEOUT_SECS);

        // Create a separate client with longer timeout for the large manifest download
        let download_client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(MANIFEST_DOWNLOAD_TIMEOUT_SECS))
            .build() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to create download client: {:?}", e);
                return false;
            }
        };

        // Download the manifest zip file
        let mut download_response = match download_client.get(&download_url).send().await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to download manifest: {:?}", e);
                return false;
            }
        };

        debug!("Reading manifest chunks...");
        let mut out: Vec<u8> = Vec::new();
        while let Some(chunk) = match download_response.chunk().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read chunk: {:?}", e);
                return false;
            }
        } {
            out.extend_from_slice(&chunk);
        }
        debug!("Downloaded {} bytes", out.len());

        // Unzip the manifest
        debug!("Unzipping manifest...");
        let cursor = std::io::Cursor::new(out);
        let mut zip = match zip::ZipArchive::new(cursor) {
            Ok(z) => z,
            Err(e) => {
                error!("Failed to open zip archive: {:?}", e);
                return false;
            }
        };

        let mut manifest_file = match zip.by_index(0) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to get file from zip: {:?}", e);
                return false;
            }
        };

        let manifest_path = dir.join("manifest.sqlite3");
        debug!("Writing manifest to {:?}", manifest_path);
        let mut outfile = match fs::File::create(&manifest_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to create manifest file: {:?}", e);
                return false;
            }
        };

        if let Err(e) = std::io::copy(&mut manifest_file, &mut outfile) {
            error!("Failed to write manifest: {:?}", e);
            return false;
        }

        // Save manifest info
        let info_json = format!(
            r#"{{"version":"{}","url":"{}"}}"#,
            version,
            manifest.mobile_world_content_paths.en
        );

        let info_path = dir.join("manifest_info.json");
        debug!("Writing manifest info to {:?}", info_path);
        if let Err(e) = fs::write(&info_path, &info_json) {
            error!("Failed to write manifest info: {:?}", e);
            return false;
        }

        info!("Manifest download complete!");
        true
    });

    result
}

/// Creates a string that can be freed with dcli_string_free
#[no_mangle]
pub extern "C" fn dcli_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

/// Gets the last error message
/// Returns null if no error
/// Caller must free the returned string with dcli_string_free
#[no_mangle]
pub extern "C" fn dcli_get_last_error() -> *mut c_char {
    // For now, return null. In the future, we could store the last error in thread-local storage
    std::ptr::null_mut()
}
