#include "dcli.h"
#include <assert.h>
#include <stddef.h>

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

int main(void) {
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
