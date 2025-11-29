/*
 * JNI bindings for Android
 *
 * This module provides JNI wrappers around the C FFI functions
 * for use from Kotlin/Java on Android.
 */

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jlong;
use std::ffi::CString;

use crate::{DcliApiClient, dcli_client_new, dcli_client_free};

/// Helper to convert JString to CString, returning null pointer on failure
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

use crate::dcli_search_player;

/// Result structure for player search
#[repr(C)]
pub struct PlayerSearchResult {
    pub member_id: i64,
    pub platform: i32,
}

/// Searches for a player by Bungie name
/// Returns member_id, or throws exception on failure
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeSearchPlayer(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
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
        throw_runtime_exception(&mut env, "Player search failed");
        return 0;
    }

    member_id
}

/// Gets the platform from the last search
/// Must be called immediately after nativeSearchPlayer
#[no_mangle]
pub extern "system" fn Java_com_ottercreeksoftware_lastbanner_data_dcli_DcliClient_nativeGetLastPlatform(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bungie_name: JString,
) -> i32 {
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
        throw_runtime_exception(&mut env, "Player search failed");
        return 0;
    }

    platform
}
