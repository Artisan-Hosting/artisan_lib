package statepersistence

import (
    "encoding/json"
    "os"
)

type Aggregator struct {
    SocketPath       string  `json:"socket_path"`
    SocketPermission *uint32 `json:"socket_permission,omitempty"`
}

type GitConfig struct {
    DefaultServer   string `json:"default_server"`
    CredentialsFile string `json:"credentials_file"`
}

type DatabaseConfig struct {
    URL      string `json:"url"`
    PoolSize uint32 `json:"pool_size"`
}

type AppConfig struct {
    AppName     string          `json:"app_name"`
    MaxRamUsage uint64          `json:"max_ram_usage"`
    MaxCpuUsage uint64          `json:"max_cpu_usage"`
    Environment string          `json:"environment"`
    DebugMode   bool            `json:"debug_mode"`
    LogLevel    string          `json:"log_level"`
    Git         *GitConfig      `json:"git,omitempty"`
    Database    *DatabaseConfig `json:"database,omitempty"`
    Aggregator  *Aggregator     `json:"aggregator,omitempty"`
}

type ErrorItem struct {
    ErrType string `json:"err_type"`
    ErrMesg string `json:"err_mesg"`
}

type Output struct {
    Timestamp uint64 `json:"timestamp"`
    Line      string `json:"line"`
}

type AppState struct {
    Name             string      `json:"name"`
    Version          string      `json:"version"`
    Data             string      `json:"data"`
    Status           string      `json:"status"`
    PID              uint32      `json:"pid"`
    LastUpdated      uint64      `json:"last_updated"`
    StaredAt         uint64      `json:"stared_at"`
    EventCounter     uint32      `json:"event_counter"`
    ErrorLog         []ErrorItem `json:"error_log"`
    Config           AppConfig   `json:"config"`
    SystemApplication bool       `json:"system_application"`
    Stdout           []Output    `json:"stdout"`
    Stderr           []Output    `json:"stderr"`
}

func SaveState(path string, state *AppState) error {
    data, err := json.MarshalIndent(state, "", "  ")
    if err != nil {
        return err
    }
    return os.WriteFile(path, data, 0o600)
}

func LoadState(path string) (*AppState, error) {
    data, err := os.ReadFile(path)
    if err != nil {
        return nil, err
    }
    var state AppState
    if err := json.Unmarshal(data, &state); err != nil {
        return nil, err
    }
    return &state, nil
}
