#include "state_persistence.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/*
 * A very small text based serializer. Only a few fields are persisted to keep
 * the example simple and dependency free.
 */
int save_state(const AppState *state, const char *path) {
    FILE *f = fopen(path, "w");
    if (!f) {
        return -1;
    }
    /* Save name, version, pid and event counter each on its own line */
    fprintf(f, "%s\n%s\n%u\n%u\n", state->name, state->version, state->pid, state->event_counter);
    fclose(f);
    return 0;
}

int load_state(AppState *state, const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) {
        return -1;
    }
    char name[256];
    char version[256];
    if (!fgets(name, sizeof(name), f) || !fgets(version, sizeof(version), f)) {
        fclose(f);
        return -1;
    }
    name[strcspn(name, "\n")] = '\0';
    version[strcspn(version, "\n")] = '\0';
    state->name = strdup(name);
    state->version = strdup(version);
    if (fscanf(f, "%u\n%u\n", &state->pid, &state->event_counter) != 2) {
        fclose(f);
        return -1;
    }
    fclose(f);
    return 0;
}
