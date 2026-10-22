use super::DcliActivityStore;
use dcli::activitystoreinterface::{
    SyncControl, SyncEvent, SyncOutcome, SyncPhase, SyncSliceDisposition,
};
use dcli::crucible::PlayerName;
use dcli::enums::platform::Platform;
use dcli::error::Error;
use std::cell::Cell;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU32, Ordering};

/// Independent cancellation signal; retain until sync and all cancellation callers return.
pub struct DcliSyncControl {
    control: SyncControl,
    error_code: AtomicU32,
}

fn sync_error_code(error: &Error) -> u32 {
    match error {
        Error::ApiTransport | Error::ApiRequest { .. } | Error::Request => 2,
        Error::RequestTimedOut => 3,
        Error::ApiThrottle { .. } => 4,
        Error::ApiHttpStatus { .. } => 5,
        Error::ApiStatus { .. } | Error::ApiNotAvailableException => 6,
        Error::PrivacyException => 7,
        Error::ApiParse { .. } | Error::ApiResponseMissing => 8,
        Error::Database { .. } => 9,
        Error::IoError { .. }
        | Error::IoErrorDirIsFile { .. }
        | Error::IoFileDoesNotExist { .. }
        | Error::ZipError { .. }
        | Error::SystemDirectoryNotFound => 10,
        Error::ParameterParseFailure
        | Error::InvalidParameters
        | Error::InvalidArgument { .. }
        | Error::ApiKeyMissingFromRequest
        | Error::DateTimePeriodOrder
        | Error::MaxActivitiesRequestCountExceeded => 11,
        Error::BungieNameNotFound
        | Error::NoProfilesFound
        | Error::NoCharacters
        | Error::CharacterDoesNotExist
        | Error::CharacterDataNotFound
        | Error::ActivityNotFound => 12,
        Error::ApiBodyTooLarge => 13,
        Error::ApiUnsupportedEncoding | Error::ApiMalformedEncoding => 14,
        _ => 1,
    }
}

fn set_sync_error(control: *const DcliSyncControl, code: u32) {
    if let Some(control) = unsafe { control.as_ref() } {
        control.error_code.store(code, Ordering::Release);
    }
}

/// Version 1 coverage snapshot. Counts refer to the requested player's sync.
/// Flags: 1 discovery complete, 2 recent ready, 4 full history complete, 8 cancelled.
/// Phases: 0 starting, 1 discovering, 2 committed, 3 recent ready, 4 complete,
/// 5 cancelled, 6 incomplete. Only committed data contributes to readiness.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DcliSyncEventV1 {
    pub version: u32,
    pub phase: u32,
    pub discovered: u32,
    pub committed: u32,
    pub pending: u32,
    pub unavailable: u32,
    pub flags: u32,
}

impl DcliSyncEventV1 {
    fn empty() -> Self {
        Self {
            version: 1,
            phase: 0,
            discovered: 0,
            committed: 0,
            pending: 0,
            unavailable: 0,
            flags: 0,
        }
    }

    fn snapshot(phase: u32, outcome: &SyncOutcome) -> Self {
        Self {
            version: 1,
            phase,
            discovered: outcome.discovered,
            committed: outcome.committed,
            pending: outcome.pending,
            unavailable: outcome.unavailable,
            flags: u32::from(outcome.discovery_complete)
                | (u32::from(outcome.recent_ready) << 1)
                | (u32::from(outcome.complete) << 2)
                | (u32::from(outcome.cancelled) << 3),
        }
    }

    fn event(event: &SyncEvent) -> Self {
        let phase = match event.phase {
            SyncPhase::Starting => 0,
            SyncPhase::Discovering => 1,
            SyncPhase::Committed => 2,
            SyncPhase::RecentReady => 3,
            SyncPhase::Complete => 4,
            SyncPhase::Cancelled => 5,
            SyncPhase::Incomplete => 6,
        };
        Self::snapshot(phase, &event.outcome)
    }
}

/// The event pointer is valid only during this synchronous callback.
/// Copy it before dispatching elsewhere; never reenter the owning store.
pub type DcliSyncCallbackV1 =
    extern "C" fn(*const DcliSyncEventV1, *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DcliSyncSliceOptionsV1 {
    pub version: u32,
    pub budget_milliseconds: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DcliSyncSliceResultV1 {
    pub version: u32,
    pub disposition: u32,
    pub newly_discovered: u32,
    pub snapshot: DcliSyncEventV1,
}

impl DcliSyncSliceResultV1 {
    fn empty() -> Self {
        Self {
            version: 1,
            disposition: 0,
            newly_discovered: 0,
            snapshot: DcliSyncEventV1::empty(),
        }
    }
}

#[no_mangle]
pub extern "C" fn dcli_store_sync_member_slice_v1(
    store: *mut DcliActivityStore,
    member_id: i64,
    membership_type: i32,
    options: *const DcliSyncSliceOptionsV1,
    control: *const DcliSyncControl,
    callback: Option<DcliSyncCallbackV1>,
    user_data: *mut c_void,
    out_result: *mut DcliSyncSliceResultV1,
) -> bool {
    set_sync_error(control, 0);
    if !out_result.is_null() {
        unsafe {
            *out_result = DcliSyncSliceResultV1::empty();
        }
    }
    if store.is_null()
        || out_result.is_null()
        || options.is_null()
        || member_id <= 0
        || !(1..=6).contains(&membership_type)
    {
        return false;
    }
    let options = unsafe { &*options };
    if options.version != 1 {
        return false;
    }
    let sync_control = unsafe { control.as_ref() }
        .map(|token| token.control.clone())
        .unwrap_or_default();
    let last = Cell::new(DcliSyncEventV1::empty());
    let store = unsafe { &mut *store };
    let result =
        store
            .runtime
            .block_on(store.store.sync_member_slice_with_control(
                member_id,
                Platform::from_id(membership_type as u32),
                std::time::Duration::from_millis(
                    options.budget_milliseconds as u64,
                ),
                &sync_control,
                |event| {
                    let event = DcliSyncEventV1::event(&event);
                    last.set(event);
                    if let Some(callback) = callback {
                        callback(&event, user_data);
                    }
                },
            ));
    match result {
        Ok(result) => {
            let disposition = match result.disposition {
                SyncSliceDisposition::Deferred => 0,
                SyncSliceDisposition::Checkpointed => 1,
                SyncSliceDisposition::FullComplete => 2,
                SyncSliceDisposition::Cancelled => 3,
            };
            let phase = if result.outcome.cancelled {
                5
            } else if result.outcome.complete {
                4
            } else {
                6
            };
            unsafe {
                *out_result = DcliSyncSliceResultV1 {
                    version: 1,
                    disposition,
                    newly_discovered: result.outcome.newly_discovered,
                    snapshot: DcliSyncEventV1::snapshot(phase, &result.outcome),
                };
            }
            true
        }
        Err(error) => {
            set_sync_error(control, sync_error_code(&error));
            log::error!("Sync slice failed: {}", error);
            unsafe {
                (*out_result).disposition = 4;
                (*out_result).snapshot = last.get();
            }
            false
        }
    }
}

#[no_mangle]
pub extern "C" fn dcli_sync_control_new() -> *mut DcliSyncControl {
    Box::into_raw(Box::new(DcliSyncControl {
        control: SyncControl::default(),
        error_code: AtomicU32::new(0),
    }))
}

#[no_mangle]
pub extern "C" fn dcli_sync_control_error_code(
    control: *const DcliSyncControl,
) -> u32 {
    unsafe { control.as_ref() }
        .map(|control| control.error_code.load(Ordering::Acquire))
        .unwrap_or(0)
}

/// May run concurrently with sync. Null is a no-op; freeing must wait for all users.
#[no_mangle]
pub extern "C" fn dcli_sync_control_cancel(control: *const DcliSyncControl) {
    if let Some(control) = unsafe { control.as_ref() } {
        control.control.cancel();
    }
}

#[no_mangle]
pub extern "C" fn dcli_sync_control_is_cancelled(
    control: *const DcliSyncControl,
) -> bool {
    unsafe { control.as_ref() }
        .map(|control| control.control.is_cancelled())
        .unwrap_or(false)
}

/// Free only after the owning sync and all cancellation callers have returned.
#[no_mangle]
pub extern "C" fn dcli_sync_control_free(control: *mut DcliSyncControl) {
    if !control.is_null() {
        unsafe {
            drop(Box::from_raw(control));
        }
    }
}

/// Runs sync on the store's owner. True means a valid outcome, not full coverage.
/// Inspect flag 4 for full completion. Token, callback context, and output storage
/// must remain alive through return; cancellation never accesses the store pointer.
#[no_mangle]
pub extern "C" fn dcli_store_sync_player_v1(
    store: *mut DcliActivityStore,
    bungie_name: *const c_char,
    control: *const DcliSyncControl,
    callback: Option<DcliSyncCallbackV1>,
    user_data: *mut c_void,
    out_outcome: *mut DcliSyncEventV1,
) -> bool {
    set_sync_error(control, 0);
    if !out_outcome.is_null() {
        unsafe {
            *out_outcome = DcliSyncEventV1::empty();
        }
    }
    if store.is_null() || bungie_name.is_null() {
        return false;
    }
    let name = match unsafe { CStr::from_ptr(bungie_name) }.to_str() {
        Ok(name) => PlayerName::from_bungie_name(name),
        Err(_) => return false,
    };
    if !name.is_valid_bungie_name() {
        return false;
    }
    let sync_control = unsafe { control.as_ref() }
        .map(|token| token.control.clone())
        .unwrap_or_default();
    let last = Cell::new(DcliSyncEventV1::empty());
    let store = unsafe { &mut *store };
    let result = store.runtime.block_on(store.store.sync_player_with_control(
        &name,
        &sync_control,
        |event| {
            let event = DcliSyncEventV1::event(&event);
            last.set(event);
            if let Some(callback) = callback {
                callback(&event, user_data);
            }
        },
    ));
    let success = match result {
        Ok(outcome) => {
            let phase = if outcome.cancelled {
                5
            } else if outcome.complete {
                4
            } else {
                6
            };
            last.set(DcliSyncEventV1::snapshot(phase, &outcome));
            true
        }
        Err(error) => {
            set_sync_error(control, sync_error_code(&error));
            log::error!("Sync failed: {}", error);
            false
        }
    };
    if !out_outcome.is_null() {
        unsafe {
            *out_outcome = last.get();
        }
    }
    success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_categories_do_not_export_error_details() {
        let cases = [
            (
                Error::Unknown {
                    description: "private".into(),
                },
                1,
            ),
            (Error::ApiTransport, 2),
            (Error::RequestTimedOut, 3),
            (Error::ApiThrottle { code: 123 }, 4),
            (Error::ApiHttpStatus { status: 503 }, 5),
            (
                Error::ApiStatus {
                    description: "private".into(),
                },
                6,
            ),
            (Error::PrivacyException, 7),
            (
                Error::ApiParse {
                    description: "private".into(),
                },
                8,
            ),
            (
                Error::Database {
                    description: "private".into(),
                },
                9,
            ),
            (
                Error::IoError {
                    description: "private".into(),
                },
                10,
            ),
            (Error::InvalidParameters, 11),
            (Error::NoProfilesFound, 12),
            (Error::ApiBodyTooLarge, 13),
            (Error::ApiMalformedEncoding, 14),
        ];
        for (error, code) in cases {
            assert_eq!(sync_error_code(&error), code);
        }
    }

    #[test]
    fn error_code_is_zero_for_null_and_cleared_before_each_call() {
        assert_eq!(dcli_sync_control_error_code(std::ptr::null()), 0);
        let control = dcli_sync_control_new();
        assert_eq!(dcli_sync_control_error_code(control), 0);
        set_sync_error(control, 8);
        assert!(!dcli_store_sync_player_v1(
            std::ptr::null_mut(),
            std::ptr::null(),
            control,
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut()
        ));
        assert_eq!(dcli_sync_control_error_code(control), 0);
        set_sync_error(control, 9);
        assert!(!dcli_store_sync_member_slice_v1(
            std::ptr::null_mut(),
            0,
            0,
            std::ptr::null(),
            control,
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut()
        ));
        assert_eq!(dcli_sync_control_error_code(control), 0);
        dcli_sync_control_free(control);
    }
}
