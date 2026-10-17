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

static void *cancel_from_another_thread(void *control) {
    dcli_sync_control_cancel(control);
    return NULL;
}

int main(void) {
    DcliSyncEventV1 outcome = {0};
    assert(!dcli_store_sync_player_v1(NULL, NULL, NULL, NULL, NULL, &outcome));
    assert(outcome.version == 1 && outcome.flags == 0);
    DcliSyncControl *control = dcli_sync_control_new();
    assert(control != NULL && !dcli_sync_control_is_cancelled(control));
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
