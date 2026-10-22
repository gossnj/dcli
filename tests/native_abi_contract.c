#include "dcli.h"
#include <assert.h>
#include <stddef.h>
#include <pthread.h>

_Static_assert(sizeof(DcliCrucibleStats) == 60, "Crucible stats ABI size changed");
_Static_assert(offsetof(DcliCrucibleStats, kills) == 16, "Crucible kills offset changed");
_Static_assert(offsetof(DcliCrucibleStats, deaths) == 28, "Crucible deaths offset changed");
_Static_assert(offsetof(DcliCharacter, class_type) == 8, "Character class offset changed");

static bool (*sync_progress)(DcliActivityStore *, const char *,
                             void (*)(const char *, uint32_t, uint32_t, void *), void *) =
    dcli_store_sync_player_with_progress;
static bool (*backfill_progress)(DcliActivityStore *,
                                 void (*)(const char *, uint32_t, uint32_t, void *), void *) =
    dcli_store_backfill_scoreboard_values_with_progress;
static DcliActivityStore *(*diagnostic_init)(const char *, char **) =
    dcli_store_init_with_diagnostics;
static bool (*checked_close)(DcliActivityStore *) = dcli_store_close;

_Static_assert(sizeof(DcliSyncEventV1) == 28, "Version 1 sync event layout changed");
_Static_assert(offsetof(DcliSyncEventV1, flags) == 24, "Version 1 coverage flags moved");
_Static_assert(sizeof(DcliSyncSliceOptionsV1) == 8, "Slice options layout changed");
_Static_assert(sizeof(DcliSyncSliceResultV1) == 40, "Slice result layout changed");
_Static_assert(offsetof(DcliSyncSliceResultV1, snapshot) == 12, "Slice snapshot moved");

static bool (*sync_slice)(DcliActivityStore *, int64_t, int32_t,
                         const DcliSyncSliceOptionsV1 *, const DcliSyncControl *,
                         DcliSyncCallbackV1, void *, DcliSyncSliceResultV1 *) =
    dcli_store_sync_member_slice_v1;
static uint32_t (*sync_error_code)(const DcliSyncControl *) = dcli_sync_control_error_code;


static void *cancel_from_another_thread(void *control) {
    dcli_sync_control_cancel(control);
    return NULL;
}

int main(void) {
    assert(sync_error_code(NULL) == 0);
    DcliSyncSliceOptionsV1 options = {1, 20};
    DcliSyncSliceResultV1 slice = {0};
    assert(!sync_slice(NULL, 10, 3, &options, NULL, NULL, NULL, &slice));
    assert(slice.version == 1 && slice.snapshot.version == 1 && slice.snapshot.flags == 0);
    assert(!sync_slice(NULL, 10, 3, &options, NULL, NULL, NULL, NULL));
    DcliSyncEventV1 outcome = {0};
    assert(!dcli_store_sync_player_v1(NULL, NULL, NULL, NULL, NULL, &outcome));
    assert(outcome.version == 1 && outcome.flags == 0);
    DcliSyncControl *control = dcli_sync_control_new();
    assert(control != NULL && !dcli_sync_control_is_cancelled(control));
    assert(sync_error_code(control) == 0);
    assert(!sync_slice(NULL, 10, 3, &options, control, NULL, NULL, &slice));
    assert(sync_error_code(control) == 0);
    assert(!dcli_store_sync_player_v1(NULL, NULL, control, NULL, NULL, &outcome));
    assert(sync_error_code(control) == 0);
    pthread_t cancelling_thread;
    assert(pthread_create(&cancelling_thread, NULL, cancel_from_another_thread, control) == 0);
    assert(pthread_join(cancelling_thread, NULL) == 0);
    assert(dcli_sync_control_is_cancelled(control));
    dcli_sync_control_cancel(control);
    dcli_sync_control_free(control);
    dcli_sync_control_cancel(NULL);
    dcli_sync_control_free(NULL);
    char *diagnostic = NULL;
    assert(diagnostic_init(NULL, &diagnostic) == NULL);
    if (diagnostic != NULL) {
        dcli_string_free(diagnostic);
    }
    assert(checked_close(NULL));
    assert(!sync_progress(NULL, NULL, NULL, NULL));
    assert(!backfill_progress(NULL, NULL, NULL));
    return 0;
}
