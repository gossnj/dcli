/*
 * JNI bindings for Android
 *
 * This module provides JNI wrappers around the C FFI functions
 * for use from Kotlin/Java on Android.
 */

use jni::JNIEnv;
use jni::objects::{JClass, JString, JLongArray, JIntArray, JFloatArray};
use jni::sys::{jlong, jint, jboolean, jfloat};
use std::ffi::CString;

use crate::{
    DcliApiClient, DcliActivityStore, DcliCrucibleStats, DcliCharacter,
    dcli_client_new, dcli_client_free, dcli_search_player,
    dcli_get_characters, dcli_get_crucible_stats,
    dcli_store_init, dcli_store_free, dcli_store_add_player,
    dcli_store_remove_player, dcli_store_sync_player,
    dcli_store_get_crucible_stats,
    dcli_manifest_needs_update, dcli_manifest_download,
};

/// Helper to convert JString to CString, returning None on failure
fn jstring_to_cstring(env: &mut JNIEnv, s: &JString) -> Option<CString> {
    let java_str = env.get_string(s).ok()?;
    let rust_str: String = java_str.into();
    CString::new(rust_str).ok()
}

/// Helper to throw a RuntimeException
fn throw_runtime_exception(env: &mut JNIEnv, message: &str) {
    let _ = env.throw_new("java/lang/RuntimeException", message);
}

// ============================================================================
// CLIENT METHODS
// ============================================================================

/// Creates a new API client with the given Bungie API key
/// Returns a pointer as jlong, or 0 on failure
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeNew(
    mut env: JNIEnv,
    _class: JClass,
    api_key: JString,
) -> jlong {
    let c_api_key = match jstring_to_cstring(&mut env, &api_key) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert API key string");
            return 0;
        }
    };

    let ptr = dcli_client_new(c_api_key.as_ptr());

    if ptr.is_null() {
        throw_runtime_exception(&mut env, "Failed to create API client");
        return 0;
    }

    ptr as jlong
}

/// Frees a client created with nativeNew
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        dcli_client_free(ptr as *mut DcliApiClient);
    }
}

// ============================================================================
// SEARCH PLAYER
// ============================================================================

/// Searches for a player by Bungie name
/// Returns member_id, or 0 on failure (also sets out_platform)
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeSearchPlayer(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
    out_platform: JIntArray,
) -> jlong {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Client pointer is null");
        return 0;
    }

    let c_bungie_name = match jstring_to_cstring(&mut env, &bungie_name) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert bungie name string");
            return 0;
        }
    };

    let mut member_id: i64 = 0;
    let mut platform: i32 = 0;

    let success = dcli_search_player(
        ptr as *mut DcliApiClient,
        c_bungie_name.as_ptr(),
        &mut member_id,
        &mut platform,
    );

    if !success {
        return 0;
    }

    // Write platform to output array
    let platform_array = [platform];
    if let Err(e) = env.set_int_array_region(&out_platform, 0, &platform_array) {
        throw_runtime_exception(&mut env, &format!("Failed to set platform: {:?}", e));
        return 0;
    }

    member_id
}

// ============================================================================
// GET CHARACTERS
// ============================================================================

/// Gets characters for a player
/// Returns count of characters, fills output arrays
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeGetCharacters(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    member_id: jlong,
    platform: jint,
    out_ids: JLongArray,
    out_class_types: JIntArray,
    out_minutes_played: JLongArray,
) -> jint {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Client pointer is null");
        return 0;
    }

    // Allocate space for up to 3 characters
    let mut characters = [
        DcliCharacter { id: 0, class_type: 0, minutes_played_total: 0 },
        DcliCharacter { id: 0, class_type: 0, minutes_played_total: 0 },
        DcliCharacter { id: 0, class_type: 0, minutes_played_total: 0 },
    ];

    let count = dcli_get_characters(
        ptr as *mut DcliApiClient,
        member_id,
        platform,
        characters.as_mut_ptr(),
        3,
    );

    if count <= 0 {
        return 0;
    }

    // Fill output arrays
    let ids: Vec<i64> = characters.iter().take(count as usize).map(|c| c.id).collect();
    let class_types: Vec<i32> = characters.iter().take(count as usize).map(|c| c.class_type).collect();
    let minutes: Vec<i64> = characters.iter().take(count as usize).map(|c| c.minutes_played_total).collect();

    if let Err(e) = env.set_long_array_region(&out_ids, 0, &ids) {
        throw_runtime_exception(&mut env, &format!("Failed to set ids: {:?}", e));
        return 0;
    }
    if let Err(e) = env.set_int_array_region(&out_class_types, 0, &class_types) {
        throw_runtime_exception(&mut env, &format!("Failed to set class types: {:?}", e));
        return 0;
    }
    if let Err(e) = env.set_long_array_region(&out_minutes_played, 0, &minutes) {
        throw_runtime_exception(&mut env, &format!("Failed to set minutes: {:?}", e));
        return 0;
    }

    count
}

// ============================================================================
// GET CRUCIBLE STATS
// ============================================================================

/// Gets Crucible stats for a character from the API
/// Returns true on success, fills output array with 15 floats
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeGetCrucibleStats(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    member_id: jlong,
    character_id: jlong,
    platform: jint,
    mode: jint,
    out_stats: JFloatArray,
) -> jboolean {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Client pointer is null");
        return 0;
    }

    let mut stats = DcliCrucibleStats {
        activities_entered: 0.0,
        activities_won: 0.0,
        activities_lost: 0.0,
        assists: 0.0,
        kills: 0.0,
        average_kill_distance: 0.0,
        seconds_played: 0.0,
        deaths: 0.0,
        average_lifespan: 0.0,
        opponents_defeated: 0.0,
        efficiency: 0.0,
        kills_deaths_ratio: 0.0,
        kills_deaths_assists: 0.0,
        suicides: 0.0,
        precision_kills: 0.0,
    };

    let success = dcli_get_crucible_stats(
        ptr as *mut DcliApiClient,
        member_id,
        character_id,
        platform,
        mode,
        &mut stats,
    );

    if !success {
        return 0;
    }

    // Pack stats into array (order matches Kotlin CrucibleStats constructor)
    let stats_array: [jfloat; 15] = [
        stats.activities_entered,
        stats.activities_won,
        stats.activities_lost,
        stats.assists,
        stats.kills,
        stats.average_kill_distance,
        stats.seconds_played,
        stats.deaths,
        stats.average_lifespan,
        stats.opponents_defeated,
        stats.efficiency,
        stats.kills_deaths_ratio,
        stats.kills_deaths_assists,
        stats.suicides,
        stats.precision_kills,
    ];

    if let Err(e) = env.set_float_array_region(&out_stats, 0, &stats_array) {
        throw_runtime_exception(&mut env, &format!("Failed to set stats: {:?}", e));
        return 0;
    }

    1
}

// ============================================================================
// STORE METHODS
// ============================================================================

/// Initializes the activity store with the given data directory
/// Returns a pointer as jlong, or 0 on failure
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jlong {
    let c_data_dir = match jstring_to_cstring(&mut env, &data_dir) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert data dir string");
            return 0;
        }
    };

    let ptr = dcli_store_init(c_data_dir.as_ptr());

    if ptr.is_null() {
        return 0;
    }

    ptr as jlong
}

/// Frees a store created with nativeInit
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if ptr != 0 {
        dcli_store_free(ptr as *mut DcliActivityStore);
    }
}

/// Adds a player to the sync list
/// Returns true on success
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeAddPlayer(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
) -> jboolean {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Store pointer is null");
        return 0;
    }

    let c_bungie_name = match jstring_to_cstring(&mut env, &bungie_name) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert bungie name string");
            return 0;
        }
    };

    if dcli_store_add_player(ptr as *mut DcliActivityStore, c_bungie_name.as_ptr()) {
        1
    } else {
        0
    }
}

/// Removes a player from the sync list
/// Returns true on success
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeRemovePlayer(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
) -> jboolean {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Store pointer is null");
        return 0;
    }

    let c_bungie_name = match jstring_to_cstring(&mut env, &bungie_name) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert bungie name string");
            return 0;
        }
    };

    if dcli_store_remove_player(ptr as *mut DcliActivityStore, c_bungie_name.as_ptr()) {
        1
    } else {
        0
    }
}

/// Syncs a player's activities
/// Returns true on success
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeSyncPlayer(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
) -> jboolean {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Store pointer is null");
        return 0;
    }

    let c_bungie_name = match jstring_to_cstring(&mut env, &bungie_name) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert bungie name string");
            return 0;
        }
    };

    if dcli_store_sync_player(ptr as *mut DcliActivityStore, c_bungie_name.as_ptr()) {
        1
    } else {
        0
    }
}

/// Gets Crucible stats from the local database
/// Returns true on success, fills output array with 15 floats
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeGetCrucibleStats(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
    character_class: jint,
    mode: jint,
    out_stats: JFloatArray,
) -> jboolean {
    if ptr == 0 {
        throw_runtime_exception(&mut env, "Store pointer is null");
        return 0;
    }

    let c_bungie_name = match jstring_to_cstring(&mut env, &bungie_name) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert bungie name string");
            return 0;
        }
    };

    let mut stats = DcliCrucibleStats {
        activities_entered: 0.0,
        activities_won: 0.0,
        activities_lost: 0.0,
        assists: 0.0,
        kills: 0.0,
        average_kill_distance: 0.0,
        seconds_played: 0.0,
        deaths: 0.0,
        average_lifespan: 0.0,
        opponents_defeated: 0.0,
        efficiency: 0.0,
        kills_deaths_ratio: 0.0,
        kills_deaths_assists: 0.0,
        suicides: 0.0,
        precision_kills: 0.0,
    };

    let success = dcli_store_get_crucible_stats(
        ptr as *mut DcliActivityStore,
        c_bungie_name.as_ptr(),
        character_class,
        mode,
        &mut stats,
    );

    if !success {
        return 0;
    }

    // Pack stats into array
    let stats_array: [jfloat; 15] = [
        stats.activities_entered,
        stats.activities_won,
        stats.activities_lost,
        stats.assists,
        stats.kills,
        stats.average_kill_distance,
        stats.seconds_played,
        stats.deaths,
        stats.average_lifespan,
        stats.opponents_defeated,
        stats.efficiency,
        stats.kills_deaths_ratio,
        stats.kills_deaths_assists,
        stats.suicides,
        stats.precision_kills,
    ];

    if let Err(e) = env.set_float_array_region(&out_stats, 0, &stats_array) {
        throw_runtime_exception(&mut env, &format!("Failed to set stats: {:?}", e));
        return 0;
    }

    1
}

// ============================================================================
// MANIFEST METHODS
// ============================================================================

/// Checks if the manifest needs updating
/// Returns true if update needed, false if current
/// Returns -1 on error
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeManifestNeedsUpdate(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jint {
    let c_data_dir = match jstring_to_cstring(&mut env, &data_dir) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert data dir string");
            return -1;
        }
    };

    let mut needs_update: bool = false;

    let success = dcli_manifest_needs_update(c_data_dir.as_ptr(), &mut needs_update);

    if !success {
        return -1;
    }

    if needs_update { 1 } else { 0 }
}

/// Downloads and installs the manifest
/// Returns true on success
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeManifestDownload(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    api_key: JString,
) -> jboolean {
    let c_data_dir = match jstring_to_cstring(&mut env, &data_dir) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert data dir string");
            return 0;
        }
    };

    let c_api_key = match jstring_to_cstring(&mut env, &api_key) {
        Some(s) => s,
        None => {
            throw_runtime_exception(&mut env, "Failed to convert API key string");
            return 0;
        }
    };

    if dcli_manifest_download(c_data_dir.as_ptr(), c_api_key.as_ptr()) {
        1
    } else {
        0
    }
}
