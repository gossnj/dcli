/* JNI wrappers for Kotlin and Java callers. */

use jni::objects::{
    GlobalRef, JClass, JFloatArray, JIntArray, JLongArray, JObject, JString,
    JThrowable, JValue,
};
use jni::sys::{jboolean, jfloat, jint, jlong};
use jni::{JNIEnv, JavaVM};
use std::ffi::{c_void, CString};

use crate::{
    dcli_client_free, dcli_client_new, dcli_get_characters,
    dcli_get_crucible_stats, dcli_manifest_download,
    dcli_manifest_needs_update, dcli_search_player, dcli_store_add_player,
    dcli_store_backfill_scoreboard_values, dcli_store_close, dcli_store_free,
    dcli_store_get_crucible_stats, dcli_store_init, dcli_store_remove_player,
    dcli_store_sync_member_slice_v1, dcli_store_sync_player,
    dcli_store_sync_player_v1, dcli_sync_control_cancel,
    dcli_sync_control_error_code, dcli_sync_control_free,
    dcli_sync_control_is_cancelled, dcli_sync_control_new, DcliActivityStore,
    DcliApiClient, DcliCharacter, DcliCrucibleStats, DcliSyncControl,
    DcliSyncEventV1, DcliSyncSliceOptionsV1, DcliSyncSliceResultV1,
};

fn jstring_to_cstring(env: &mut JNIEnv, s: &JString) -> Option<CString> {
    let java_str = env.get_string(s).ok()?;
    let rust_str: String = java_str.into();
    CString::new(rust_str).ok()
}

fn sync_event_values(event: &DcliSyncEventV1) -> [jlong; 7] {
    [
        event.version,
        event.phase,
        event.discovered,
        event.committed,
        event.pending,
        event.unavailable,
        event.flags,
    ]
    .map(jlong::from)
}

fn new_long_array(env: &mut JNIEnv, values: &[jlong]) -> jni::sys::jlongArray {
    let array = match env.new_long_array(values.len() as i32) {
        Ok(array) => array,
        Err(_) => return std::ptr::null_mut(),
    };
    if env.set_long_array_region(&array, 0, values).is_err() {
        return std::ptr::null_mut();
    }
    array.into_raw()
}

struct SyncListener {
    vm: JavaVM,
    listener: GlobalRef,
    control: *const DcliSyncControl,
    failure: Option<GlobalRef>,
    failed: bool,
}

fn record_listener_failure(
    failed: &mut bool,
    failure: &mut Option<GlobalRef>,
    control: *const DcliSyncControl,
    env: &mut JNIEnv,
) {
    *failed = true;
    if let Ok(true) = env.exception_check() {
        if let Ok(throwable) = env.exception_occurred() {
            let _ = env.exception_clear();
            *failure = env.new_global_ref(throwable).ok();
        }
    }
    dcli_sync_control_cancel(control);
}

extern "C" fn sync_listener_callback(
    event: *const DcliSyncEventV1,
    user_data: *mut c_void,
) {
    let listener = unsafe { &mut *(user_data as *mut SyncListener) };
    if listener.failed || event.is_null() {
        return;
    }
    let mut env = match listener.vm.attach_current_thread() {
        Ok(env) => env,
        Err(_) => {
            listener.failed = true;
            dcli_sync_control_cancel(listener.control);
            return;
        }
    };
    let values = sync_event_values(unsafe { &*event });
    let array = match env.new_long_array(7) {
        Ok(array) => array,
        Err(_) => {
            record_listener_failure(
                &mut listener.failed,
                &mut listener.failure,
                listener.control,
                &mut env,
            );
            return;
        }
    };
    let delivered =
        env.set_long_array_region(&array, 0, &values).and_then(|_| {
            env.call_method(
                listener.listener.as_obj(),
                "onProgress",
                "([J)V",
                &[JValue::Object(&array)],
            )
            .map(|_| ())
        });
    let _ = env.delete_local_ref(array);
    if delivered.is_err() {
        record_listener_failure(
            &mut listener.failed,
            &mut listener.failure,
            listener.control,
            &mut env,
        );
    }
}

fn sync_listener(
    env: &mut JNIEnv,
    listener: JObject,
    control: *const DcliSyncControl,
) -> Option<SyncListener> {
    if listener.is_null() {
        return None;
    }
    match (env.get_java_vm(), env.new_global_ref(listener)) {
        (Ok(vm), Ok(listener)) => Some(SyncListener {
            vm,
            listener,
            control,
            failure: None,
            failed: false,
        }),
        _ => {
            throw_runtime_exception(env, "Failed to prepare sync listener");
            None
        }
    }
}

fn restore_listener_failure(env: &mut JNIEnv, listener: &SyncListener) {
    if let Some(throwable) = &listener.failure {
        if let Ok(local) = env.new_local_ref(throwable.as_obj()) {
            let _ = env.throw(JThrowable::from(local));
            return;
        }
    }
    throw_runtime_exception(env, "Sync progress listener failed");
}

fn throw_runtime_exception(env: &mut JNIEnv, message: &str) {
    let _ = env.throw_new("java/lang/RuntimeException", message);
}

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
            throw_runtime_exception(
                &mut env,
                "Failed to convert API key string",
            );
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert bungie name string",
            );
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

    let platform_array = [platform];
    if let Err(e) = env.set_int_array_region(&out_platform, 0, &platform_array)
    {
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set platform: {:?}", e),
        );
        return 0;
    }

    member_id
}

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

    let mut characters = [
        DcliCharacter {
            id: 0,
            class_type: 0,
            minutes_played_total: 0,
        },
        DcliCharacter {
            id: 0,
            class_type: 0,
            minutes_played_total: 0,
        },
        DcliCharacter {
            id: 0,
            class_type: 0,
            minutes_played_total: 0,
        },
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

    let ids: Vec<i64> = characters
        .iter()
        .take(count as usize)
        .map(|c| c.id)
        .collect();
    let class_types: Vec<i32> = characters
        .iter()
        .take(count as usize)
        .map(|c| c.class_type)
        .collect();
    let minutes: Vec<i64> = characters
        .iter()
        .take(count as usize)
        .map(|c| c.minutes_played_total)
        .collect();

    if let Err(e) = env.set_long_array_region(&out_ids, 0, &ids) {
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set ids: {:?}", e),
        );
        return 0;
    }
    if let Err(e) = env.set_int_array_region(&out_class_types, 0, &class_types)
    {
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set class types: {:?}", e),
        );
        return 0;
    }
    if let Err(e) = env.set_long_array_region(&out_minutes_played, 0, &minutes)
    {
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set minutes: {:?}", e),
        );
        return 0;
    }

    count
}

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
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set stats: {:?}", e),
        );
        return 0;
    }

    1
}

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
            throw_runtime_exception(
                &mut env,
                "Failed to convert data dir string",
            );
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

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeClose(
    _env: JNIEnv,
    _class: JClass,
    store: jlong,
) -> jboolean {
    dcli_store_close(store as *mut DcliActivityStore) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeCreateSyncControl(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    dcli_sync_control_new() as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeCancelSync(
    _env: JNIEnv,
    _class: JClass,
    control: jlong,
) {
    dcli_sync_control_cancel(control as *const DcliSyncControl);
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeSyncControlIsCancelled(
    _env: JNIEnv,
    _class: JClass,
    control: jlong,
) -> jboolean {
    dcli_sync_control_is_cancelled(control as *const DcliSyncControl)
        as jboolean
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeSyncControlErrorCode(
    _env: JNIEnv,
    _class: JClass,
    control: jlong,
) -> jint {
    dcli_sync_control_error_code(control as *const DcliSyncControl) as jint
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeFreeSyncControl(
    _env: JNIEnv,
    _class: JClass,
    control: jlong,
) {
    dcli_sync_control_free(control as *mut DcliSyncControl);
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeSyncPlayerV1(
    mut env: JNIEnv,
    _class: JClass,
    store: jlong,
    name: JString,
    control: jlong,
    listener: JObject,
) -> jni::sys::jlongArray {
    if store == 0 || control == 0 || name.is_null() {
        throw_runtime_exception(&mut env, "Invalid sync arguments");
        return std::ptr::null_mut();
    }
    let name = match jstring_to_cstring(&mut env, &name) {
        Some(name) => name,
        None => {
            throw_runtime_exception(&mut env, "Invalid player name");
            return std::ptr::null_mut();
        }
    };
    let mut listener =
        sync_listener(&mut env, listener, control as *const DcliSyncControl);
    if env.exception_check().unwrap_or(true) {
        return std::ptr::null_mut();
    }
    let mut outcome = DcliSyncEventV1 {
        version: 1,
        phase: 0,
        discovered: 0,
        committed: 0,
        pending: 0,
        unavailable: 0,
        flags: 0,
    };
    let callback = listener
        .as_mut()
        .map(|_| sync_listener_callback as crate::DcliSyncCallbackV1);
    let context = listener
        .as_mut()
        .map(|listener| listener as *mut SyncListener as *mut c_void)
        .unwrap_or(std::ptr::null_mut());
    let success = dcli_store_sync_player_v1(
        store as *mut DcliActivityStore,
        name.as_ptr(),
        control as *const DcliSyncControl,
        callback,
        context,
        &mut outcome,
    );
    if let Some(listener) = &listener {
        if listener.failed {
            restore_listener_failure(&mut env, listener);
            return std::ptr::null_mut();
        }
    }
    let mut values = [0; 9];
    values[0] = jlong::from(success);
    values[1..8].copy_from_slice(&sync_event_values(&outcome));
    values[8] = jlong::from(dcli_sync_control_error_code(
        control as *const DcliSyncControl,
    ));
    new_long_array(&mut env, &values)
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeSyncMemberSliceV1(
    mut env: JNIEnv,
    _class: JClass,
    store: jlong,
    member: jlong,
    platform: jint,
    budget_milliseconds: jlong,
    control: jlong,
    listener: JObject,
) -> jni::sys::jlongArray {
    if store == 0
        || control == 0
        || member <= 0
        || !(1..=6).contains(&platform)
        || !(0..=u32::MAX as jlong).contains(&budget_milliseconds)
    {
        throw_runtime_exception(&mut env, "Invalid sync slice arguments");
        return std::ptr::null_mut();
    }
    let mut listener =
        sync_listener(&mut env, listener, control as *const DcliSyncControl);
    if env.exception_check().unwrap_or(true) {
        return std::ptr::null_mut();
    }
    let options = DcliSyncSliceOptionsV1 {
        version: 1,
        budget_milliseconds: budget_milliseconds as u32,
    };
    let mut result = DcliSyncSliceResultV1 {
        version: 1,
        disposition: 0,
        newly_discovered: 0,
        snapshot: DcliSyncEventV1 {
            version: 1,
            phase: 0,
            discovered: 0,
            committed: 0,
            pending: 0,
            unavailable: 0,
            flags: 0,
        },
    };
    let callback = listener
        .as_mut()
        .map(|_| sync_listener_callback as crate::DcliSyncCallbackV1);
    let context = listener
        .as_mut()
        .map(|listener| listener as *mut SyncListener as *mut c_void)
        .unwrap_or(std::ptr::null_mut());
    let success = dcli_store_sync_member_slice_v1(
        store as *mut DcliActivityStore,
        member,
        platform,
        &options,
        control as *const DcliSyncControl,
        callback,
        context,
        &mut result,
    );
    if let Some(listener) = &listener {
        if listener.failed {
            restore_listener_failure(&mut env, listener);
            return std::ptr::null_mut();
        }
    }
    let mut values = [0; 12];
    values[0] = jlong::from(success);
    values[1] = jlong::from(result.version);
    values[2] = jlong::from(result.disposition);
    values[3] = jlong::from(result.newly_discovered);
    values[4..11].copy_from_slice(&sync_event_values(&result.snapshot));
    values[11] = jlong::from(dcli_sync_control_error_code(
        control as *const DcliSyncControl,
    ));
    new_long_array(&mut env, &values)
}

#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeBackfillScoreboardValues(
    _env: JNIEnv,
    _class: JClass,
    store: jlong,
) -> jboolean {
    dcli_store_backfill_scoreboard_values(store as *mut DcliActivityStore)
        as jboolean
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert bungie name string",
            );
            return 0;
        }
    };

    if dcli_store_add_player(
        ptr as *mut DcliActivityStore,
        c_bungie_name.as_ptr(),
    ) {
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert bungie name string",
            );
            return 0;
        }
    };

    if dcli_store_remove_player(
        ptr as *mut DcliActivityStore,
        c_bungie_name.as_ptr(),
    ) {
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert bungie name string",
            );
            return 0;
        }
    };

    if dcli_store_sync_player(
        ptr as *mut DcliActivityStore,
        c_bungie_name.as_ptr(),
    ) {
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert bungie name string",
            );
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
        throw_runtime_exception(
            &mut env,
            &format!("Failed to set stats: {:?}", e),
        );
        return 0;
    }

    1
}

/// Checks manifest freshness: 1 needs an update, 0 is current, and -1 is an error.
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliStore_nativeManifestNeedsUpdate(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jint {
    let c_data_dir = match jstring_to_cstring(&mut env, &data_dir) {
        Some(s) => s,
        None => {
            throw_runtime_exception(
                &mut env,
                "Failed to convert data dir string",
            );
            return -1;
        }
    };

    let mut needs_update: bool = false;

    let success =
        dcli_manifest_needs_update(c_data_dir.as_ptr(), &mut needs_update);

    if !success {
        return -1;
    }

    if needs_update {
        1
    } else {
        0
    }
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
            throw_runtime_exception(
                &mut env,
                "Failed to convert data dir string",
            );
            return 0;
        }
    };

    let c_api_key = match jstring_to_cstring(&mut env, &api_key) {
        Some(s) => s,
        None => {
            throw_runtime_exception(
                &mut env,
                "Failed to convert API key string",
            );
            return 0;
        }
    };

    if dcli_manifest_download(c_data_dir.as_ptr(), c_api_key.as_ptr()) {
        1
    } else {
        0
    }
}
