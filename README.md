# Artisan Middleware Library (`artisan_middleware`)

## Overview

`artisan_middleware` is a Rust-based library designed to provide essential middleware components for managing Git repositories, process control, state persistence, and more. This library is modular and reusable, allowing you to build applications that involve version control, monitoring, automation, and process management.

The primary goal of `artisan_middleware` is to simplify the development of applications that need to interact with Git, manage background processes, or maintain an application state across sessions.

## Features

- **Configuration Management**: Load and manage application settings from configuration files.
- **Git Operations**: Clone, pull, fetch, and manage Git repositories from popular platforms like GitHub, GitLab, or custom servers.
- **State Persistence**: Save and load the state of the application for reliability and robustness.
- **Process Management**: Manage background processes, including starting, stopping, killing, and restarting.
- **Encryption Utilities**: Securely handle sensitive data such as credentials.
- **Logging**: (Optional) Customize logging settings for different environments.

## Installation

Add the library to your `Cargo.toml`:

```toml
[dependencies]
artisan_middleware = { path = "../artisan_middleware" }
```

Replace the `path` with the correct relative or absolute path to the library.

## Modules

### 1. `config.rs`
Handles the loading of configuration settings from `Settings.toml`. This configuration file includes information such as Git credentials paths, polling intervals, and other application-specific settings.

#### Example

```rust
use artisan_middleware::config::AppConfig;

let config = AppConfig::new()?;
println!("Git credentials file: {}", config.git.credentials_file);
```

### 2. `git_actions.rs`
Provides utilities for interacting with Git repositories. This includes actions like cloning, pulling, and fetching updates.

- **`GitAuth`**: Represents Git repository credentials.
- **`GitAction`**: Enum representing different Git actions (Clone, Pull, Push, etc.).
- **`GitServer`**: Enum representing different Git servers (GitHub, GitLab, Custom).

#### Example

```rust
use artisan_middleware::git_actions::{GitAction, GitServer};

let git_action = GitAction::Clone {
    repo_name: "my_repo".into(),
    repo_owner: "my_user".into(),
    destination: "path/to/clone".into(),
    repo_branch: "main".into(),
    server: GitServer::GitHub,
};
git_action.execute().await?;
```

### 3. `state_persistence.rs`
Handles saving and loading the application's state. This is useful for keeping track of the last commit hash or other runtime data.

- **`AppState`**: Struct to define the application state.
- **`StatePersistence`**: Utility to save and load state to/from a file.

#### Example

```rust
use artisan_middleware::state_persistence::{AppState, StatePersistence};
use std::path::Path;

let app_state = AppState {
    last_commit_hashes: HashMap::new(),
};
StatePersistence::save_state(&app_state, Path::new("app_state.enc"))?;
```

### 4. `process_manager.rs`
Provides functions to manage background processes, such as starting, stopping, and restarting.

- **`spawn_process`**: Starts a new process.
- **`kill_process`**: Attempts to gracefully terminate a process, with the option to force kill if necessary.

#### Example

```rust
use artisan_middleware::process_manager::ProcessManager;

let child = ProcessManager::spawn_process("some_command", &["arg1", "arg2"])?;
ProcessManager::kill_process(child.id())?;
```

### 5. `encryption.rs`
Provides functions to encrypt and decrypt sensitive data. This is useful for handling sensitive credentials securely.

#### Example

```rust
use artisan_middleware::encryption::{encrypt_text, decrypt_text};

let encrypted = encrypt_text("my_secret")?;
let decrypted = decrypt_text(encrypted)?;
```

### 6. `notifications.rs`
(For Future Implementation) This module is intended to handle notifications, such as sending an email when an event occurs (e.g., a new commit is detected).

## How to Use

1. **Configuration**:
   - Define your `Settings.toml` file to include all necessary settings such as Git credentials and polling intervals.

   ```toml
   [git]
   credentials_file = "/path/to/artisan_middleware.cf"

   [polling]
   interval_seconds = 300  # Poll every 5 minutes.

   [app_specific]
   custom_value = "my_custom_setting"
   max_retries = 5
   ```

2. **Initialize the Application**:
   - Load configuration and credentials to set up the application.
   - Create instances of `GitAction` to perform Git operations.

3. **State Management**:
   - Use `StatePersistence` to maintain the state of your application.
   - This is useful to avoid redundant operations (e.g., pulling updates that have already been fetched).

 4. **Example Application Flow**:

   ```rust
   use artisan_middleware::config::AppConfig;
   use artisan_middleware::git_actions::{GitAction, GitServer};
   use artisan_middleware::state_persistence::{AppState, StatePersistence};
   use std::path::Path;

   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       // Load Configuration
       let config = AppConfig::new()?;
       
       // Load Application State
       let state_path = Path::new("app_state.enc");
       let mut app_state = StatePersistence::load_state(state_path)?;

       // Perform Git Action
       let git_action = GitAction::Clone {
           repo_name: "my_repo".into(),
           repo_owner: "my_user".into(),
           destination: "/path/to/clone".into(),
           repo_branch: "main".into(),
           server: GitServer::GitHub,
       };
       git_action.execute().await?;

       // Save Updated State
       app_state.last_commit_hashes.insert("my_repo".into(), "new_commit_hash".into());
       StatePersistence::save_state(&app_state, state_path)?;

       Ok(())
   }
   ```

## Language Bindings

The core types used for inter-process communication—`AppConfig`, `AppState`,
and the `StatePersistence` helpers—are also available in other languages. This
repository provides lightweight libraries to work with the same data structures
in **C**, **Python**, and **Go** under the [`bindings`](bindings) directory.

These bindings offer simple JSON or text based serialization so runners written
in different languages can exchange state information with the Rust
implementation.

* `bindings/python/state_persistence.py` – Python dataclasses and helpers.
* `bindings/go/statepersistence` – Go structs with load/save functions.
* `bindings/c` – C header and source with minimal serialization routines.

These modules are intentionally small and dependency free to serve as a starting
point for building runners in other languages.

## How to Contribute

1. **Fork the Repository**: Create a personal fork of the project.
2. **Create a Branch**: Create a feature or bugfix branch for your changes.
3. **Submit a Pull Request**: Create a pull request describing your changes.

## License

This project is licensed under the MIT License.

## Contact

For any questions or issues, feel free to open an issue on the repository.
