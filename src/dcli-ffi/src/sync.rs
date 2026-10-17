use super::DcliActivityStore;
use dcli::activitystoreinterface::{
    SyncControl, SyncEvent, SyncOutcome, SyncPhase,
};
use dcli::crucible::PlayerName;
use std::cell::Cell;
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;

/// Independent cancellation signal; retain until sync and all cancellation callers return.
pub struct DcliSyncControl {
    control: SyncControl,
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

#[no_mangle]
pub extern "C" fn dcli_sync_control_new() -> *mut DcliSyncControl {
    Box::into_raw(Box::new(DcliSyncControl {
        control: SyncControl::default(),
    }))
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
    let control = unsafe { control.as_ref() }
        .map(|token| token.control.clone())
        .unwrap_or_default();
    let last = Cell::new(DcliSyncEventV1::empty());
    let store = unsafe { &mut *store };
    let result = store.runtime.block_on(store.store.sync_player_with_control(
        &name,
        &control,
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
