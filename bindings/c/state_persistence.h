#ifndef STATE_PERSISTENCE_H
#define STATE_PERSISTENCE_H

#include <stdint.h>

typedef struct {
    const char *socket_path;
    uint32_t socket_permission;
} Aggregator;

typedef struct {
    const char *default_server;
    const char *credentials_file;
} GitConfig;

typedef struct {
    const char *url;
    uint32_t pool_size;
} DatabaseConfig;

typedef struct {
    const char *app_name;
    uint64_t max_ram_usage;
    uint64_t max_cpu_usage;
    const char *environment;
    int debug_mode;
    const char *log_level;
    GitConfig *git;
    DatabaseConfig *database;
    Aggregator *aggregator;
} AppConfig;

typedef struct {
    const char *err_type;
    const char *err_mesg;
} ErrorItem;

typedef struct {
    uint64_t timestamp;
    const char *line;
} Output;

typedef struct {
    const char *name;
    const char *version;
    const char *data;
    const char *status;
    uint32_t pid;
    uint64_t last_updated;
    uint64_t stared_at;
    uint32_t event_counter;
    ErrorItem *error_log;
    uint32_t error_log_len;
    AppConfig config;
    int system_application;
    Output *stdout_entries;
    uint32_t stdout_len;
    Output *stderr_entries;
    uint32_t stderr_len;
} AppState;

int save_state(const AppState *state, const char *path);
int load_state(AppState *state, const char *path);

#endif
