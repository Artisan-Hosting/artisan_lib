use std::fmt;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::{Read, Write};
use std::pin::Pin;
use std::process::Output;

use colored::Colorize;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use dusa_collection_utils::{
    errors::{ErrorArrayItem, Errors},
    functions::{create_hash, truncate},
    stringy::Stringy,
    types::PathType,
};

use crate::encryption::{decrypt_text, encrypt_text};

pub const ARTISANCF: &str = "/opt/artisan/artisan.cf";

/// Represents the Git server to interact with.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone)]
pub enum GitServer {
    GitHub,
    GitLab,
    Custom(String), // Custom server URL
}

/// Represents Git authentication information for a repository.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct GitAuth {
    /// The username or owner of the repository.
    pub user: Stringy,
    /// The name of the repository.
    pub repo: Stringy,
    /// The branch of the repository.
    pub branch: Stringy,
    /// The service where the repo is located.
    pub server: GitServer,
    /// The authentication token (optional, remove if not used).
    pub token: Option<Stringy>, // Changed to Option to allow absence
}

/// Represents Git credentials, containing a list of authentication items.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd)]
pub struct GitCredentials {
    /// A vector of `GitAuth` items.
    pub auth_items: Vec<GitAuth>,
}

/// Represents various Git actions that can be performed.
#[derive(Debug)]
pub enum GitAction {
    Clone {
        repo_name: Stringy,
        repo_owner: Stringy,
        destination: PathType,
        repo_branch: Stringy,
        server: GitServer,
    },
    Pull {
        target_branch: Stringy,
        destination: PathType,
    },
    Push {
        directory: PathType,
    },
    Stage {
        directory: PathType,
        files: Vec<String>,
    },
    Commit {
        directory: PathType,
        message: Stringy,
    },
    CheckRemoteAhead {
        directory: PathType,
    },
    Switch {
        branch: Stringy,
        destination: PathType,
    },
    SetSafe {
        directory: PathType,
    },
    SetTrack {
        directory: PathType,
    },
    Branch {
        directory: PathType,
    },
    Fetch {
        destination: PathType,
    },
    RevList {
        base: String,
        target: String,
        destination: PathType,
    },
}

impl GitCredentials {
    /// Creates a new instance of `GitCredentials` by reading and decrypting the credentials file.
    ///
    /// # Returns
    ///
    /// Returns a `GitCredentials` instance if successful.
    ///
    /// # Errors
    ///
    /// Returns an `ErrorArrayItem` if reading, decrypting, or deserializing fails.
    pub async fn new(file: Option<&PathType>) -> Result<Self, ErrorArrayItem> {
        match file {
            Some(file) => {
                if file.exists() {
                    let encrypted_credentials = Self::read_file(file)?;
                    let decrypted_string =
                        decrypt_text(encrypted_credentials).await?.replace('\n', "");
                    let data: GitCredentials = serde_json::from_str(&decrypted_string)?;
                    Ok(data)
                } else {
                    Err(ErrorArrayItem::new(
                        Errors::InvalidFile,
                        "No such file or directory".to_owned(),
                    ))
                }
            }
            None => {
                let encrypted_credentials = Self::read_file(&PathType::Str(ARTISANCF.into()))?;
                let decrypted_string = decrypt_text(encrypted_credentials).await?.replace('\n', "");
                let data: GitCredentials = serde_json::from_str(&decrypted_string)?;
                Ok(data)
            }
        }
    }

    /// Creates a new vector of `GitAuth` items by loading the credentials.
    ///
    /// # Returns
    ///
    /// Returns a vector of `GitAuth` items if successful.
    ///
    /// # Errors
    ///
    /// Returns an `ErrorArrayItem` if loading the credentials fails.
    pub async fn new_vec(file: Option<&PathType>) -> Result<Vec<GitAuth>, ErrorArrayItem> {
        let git_credentials = Self::new(file).await?;
        Ok(git_credentials.auth_items.clone())
    }

    /// Converts `GitCredentials` into a vector of `GitAuth` items.
    pub fn to_vec(self) -> Vec<GitAuth> {
        self.auth_items
    }

    /// Saves the `GitCredentials` to the specified file path by serializing and encrypting the data.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file where credentials will be saved.
    ///
    /// # Errors
    ///
    /// Returns an `ErrorArrayItem` if serialization, encryption, or file writing fails.
    pub async fn save(&self, path: &PathType) -> Result<(), ErrorArrayItem> {
        // if the array is empty we delete and re-create the empty file
        if self.clone().to_vec().len() == 0 {
            path.delete()?;
        }

        // Serialize GitCredentials to JSON
        let json_data = serde_json::to_string(self).map_err(|e| {
            ErrorArrayItem::new(Errors::GeneralError, format!("Serialization error: {}", e))
        })?;

        // Encrypt the JSON data
        let encrypted_data = encrypt_text(Stringy::new(&json_data)).await?;

        // Write the encrypted data to the file
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true) // Ensure the file is truncated to avoid leftover data
            .open(path)
            .map_err(|e| {
                ErrorArrayItem::new(
                    Errors::InvalidFile,
                    format!(
                        "Unable to create or open the file: {:?}, error: {}",
                        path, e
                    ),
                )
            })?;

        file.write_all(encrypted_data.as_bytes()).map_err(|e| {
            ErrorArrayItem::new(
                Errors::ReadingFile,
                format!("Unable to write to the file: {:?}, error: {}", path, e),
            )
        })?;

        // Ensure data is flushed to disk
        file.sync_all().map_err(|e| {
            ErrorArrayItem::new(
                Errors::ReadingFile,
                format!("Unable to sync data to the file: {:?}, error: {}", path, e),
            )
        })?;

        Ok(())
    }

    /// Reads the contents of a file and returns it as a `Stringy`, removing any newline characters.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    ///
    /// Returns the file contents as a `Stringy`.
    ///
    /// # Errors
    ///
    /// Returns an `ErrorArrayItem` if reading the file fails.
    pub fn read_file(file_path: &PathType) -> Result<Stringy, ErrorArrayItem> {
        let mut file = File::open(file_path)?;
        let mut file_contents = String::new();
        file.read_to_string(&mut file_contents)?;
        Ok(Stringy::new(&file_contents.replace('\n', "")))
    }

    /// Adds a new `GitAuth` item to the credentials.
    ///
    /// # Arguments
    ///
    /// * `auth` - The `GitAuth` item to add.
    pub fn add_auth(&mut self, auth: GitAuth) {
        self.auth_items.push(auth);
    }

    /// Bootstraps Git credentials by attempting to load existing credentials or creating a new default set.
    ///
    /// # Returns
    ///
    /// Returns a `GitCredentials` instance.
    ///
    /// # Errors
    ///
    /// Can't panic.
    pub async fn bootstrap_git_credentials() -> Result<GitCredentials, ErrorArrayItem> {
        match GitCredentials::new(None).await {
            Ok(creds) => Ok(creds),
            Err(_) => {
                let default_creds = GitCredentials {
                    auth_items: Vec::new(),
                };
                Ok(default_creds)
            }
        }
    }

    /// Deletes the value at the 'index' in the git credentials array
    /// The caller is still responsible for saving the array using '.save(&PathType)' after calling
    pub async fn delete_item(&mut self, index: usize) -> Result<Self, ErrorArrayItem> {
        let mut vec = self.clone().to_vec();

        let vec_len = vec.len();

        if index > vec_len {
            return Err(ErrorArrayItem::new(
                Errors::Git,
                "The requested value to remove is out of bounds".to_owned(),
            ));
        }

        vec.remove(index);

        Ok(GitCredentials { auth_items: vec })
    }
}

impl GitAuth {
    /// Assembles the Git remote URL based on the provided information.
    ///
    /// # Returns
    ///
    /// A full Git remote URL string.
    pub fn assemble_remote_url(&self) -> String {
        let base_url = match &self.server {
            GitServer::GitHub => "https://github.com",
            GitServer::GitLab => "https://gitlab.com",
            GitServer::Custom(url) => url.trim_end_matches('/'),
        };

        // Construct the URL, adding token if available
        if let Some(token) = &self.token {
            format!(
                "https://{}@{}/{}/{}.git",
                token, base_url, self.user, self.repo
            )
        } else {
            format!("{}/{}/{}.git", base_url, self.user, self.repo)
        }
    }

    /// Generates the ais_id for a given project.
    pub fn generate_id(&self) -> Stringy {
        Stringy::from(truncate(
            &create_hash(format!("{}-{}-{}", self.branch, self.repo, self.user)),
            8,
        ))
    }
}

impl GitAction {
    /// Executes the specified Git action asynchronously.
    ///
    /// # Returns
    ///
    /// Returns an `Option<Output>` containing the output of the command if applicable.
    ///
    /// # Errors
    ///
    /// Returns an `ErrorArrayItem` if the action fails.
    pub fn execute(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Output>, ErrorArrayItem>> + '_>> {
        Box::pin(async move {
            check_git_installed().await?;

            match self {
                GitAction::Clone {
                    repo_name,
                    repo_owner,
                    destination,
                    repo_branch,
                    server,
                } => {
                    let url = match server {
                        GitServer::GitHub => {
                            format!("https://github.com/{}/{}.git", repo_owner, repo_name)
                        }
                        GitServer::GitLab => {
                            format!("https://gitlab.com/{}/{}.git", repo_owner, repo_name)
                        }
                        GitServer::Custom(base_url) => {
                            format!(
                                "{}/{}/{}.git",
                                base_url.trim_end_matches('/'),
                                repo_owner,
                                repo_name
                            )
                        }
                    };

                    execute_git_command(&[
                        "clone",
                        "-b",
                        repo_branch,
                        &url,
                        &destination.to_string(),
                    ])
                    .await
                    .map(Some)
                }
                GitAction::Pull {
                    target_branch,
                    destination,
                } => {
                    if destination.exists() {
                        execute_git_command(&["-C", &destination.to_string(), "pull"]).await?;
                        execute_git_command(&[
                            "-C",
                            &destination.to_string(),
                            "switch",
                            target_branch,
                        ])
                        .await
                        .map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::Push { directory } => {
                    if directory.exists() {
                        execute_git_command(&["-C", &directory.to_string(), "push"])
                            .await
                            .map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::Stage { directory, files } => {
                    let dir = directory.clone().to_string();
                    if directory.exists() {
                        let mut args = vec!["-C", &dir, "add"];
                        args.extend(files.iter().map(|s| s.as_str()));
                        execute_git_command(&args).await.map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::Commit { directory, message } => {
                    if directory.exists() {
                        execute_git_command(&[
                            "-C",
                            &directory.to_string(),
                            "commit",
                            "-m",
                            message,
                        ])
                        .await
                        .map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::CheckRemoteAhead { directory } => {
                    let is_ahead = check_remote_ahead(directory).await?;
                    if is_ahead {
                        Ok(Some(
                            Command::new("echo")
                                .arg("Remote is ahead")
                                .output()
                                .await
                                .map_err(ErrorArrayItem::from)?,
                        ))
                    } else {
                        Ok(None)
                    }
                }
                GitAction::Fetch { destination } => {
                    if destination.exists() {
                        execute_git_command(&["-C", &destination.to_string(), "fetch", "--all"])
                            .await
                            .map(|_| None)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::Switch {
                    branch,
                    destination,
                } => {
                    if destination.exists() {
                        execute_git_command(&["-C", &destination.to_string(), "switch", branch])
                            .await
                            .map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::SetSafe { directory } => execute_git_command(&[
                    "config",
                    "--global",
                    "--add",
                    "safe.directory",
                    &directory.to_string(),
                ])
                .await
                .map(Some),
                GitAction::SetTrack { directory } => {
                    if directory.exists() {
                        execute_git_command(&["-C", &directory.to_string(), "fetch"]).await?;
                        let branch_output = Self::Branch {
                            directory: directory.clone(),
                        }
                        .execute()
                        .await?;

                        if let Some(output) = branch_output {
                            let output_str = String::from_utf8_lossy(&output.stdout);
                            let branches: Vec<&str> = output_str
                                .lines()
                                .filter(|line| !line.contains("->"))
                                .map(|line| line.trim())
                                .collect();

                            for remote in branches {
                                let clean_remote = remote.replace("origin/", "");
                                if !clean_remote.is_empty() {
                                    execute_git_command(&[
                                        "-C",
                                        &directory.to_string(),
                                        "branch",
                                        "--track",
                                        &clean_remote,
                                        remote,
                                    ])
                                    .await?;
                                }
                            }
                            Ok(None)
                        } else {
                            Err(ErrorArrayItem::new(
                                Errors::Git,
                                "Invalid branch data from the current repository".to_string(),
                            ))
                        }
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::Branch { directory } => {
                    if directory.exists() {
                        execute_git_command(&["-C", &directory.to_string(), "branch", "-r"])
                            .await
                            .map(Some)
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
                GitAction::RevList {
                    base,
                    target,
                    destination,
                } => {
                    if destination.exists() {
                        // git rev-list <base>..<target> to get a list of commits on the base not in target
                        let rev_list_output = execute_git_command(&[
                            "-C",
                            &destination.to_string(),
                            "rev-list",
                            "--count",
                            &format!("{}..{}", base, target),
                        ])
                        .await?;

                        Ok(Some(rev_list_output))
                    } else {
                        Err(ErrorArrayItem::new(
                            Errors::InvalidFile,
                            "Repository path not found".to_string(),
                        ))
                    }
                }
            }
        })
    }
}

/// Checks if Git is installed on the system.
///
/// # Errors
///
/// Returns an `ErrorArrayItem` if Git is not installed or not found.
async fn check_git_installed() -> Result<(), ErrorArrayItem> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .await
        .map_err(ErrorArrayItem::from)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(ErrorArrayItem::new(
            Errors::GeneralError,
            "Git not installed or not found".to_string(),
        ))
    }
}

/// Executes a Git command with the provided arguments.
///
/// # Arguments
///
/// * `args` - A slice of command-line arguments to pass to Git.
///
/// # Returns
///
/// Returns the `Output` of the command if successful.
///
/// # Errors
///
/// Returns an `ErrorArrayItem` if the command execution fails.
async fn execute_git_command(args: &[&str]) -> Result<Output, ErrorArrayItem> {
    let output = Command::new("git")
        .args(args)
        .output()
        .await
        .map_err(|e| ErrorArrayItem::from(e))?;

    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(ErrorArrayItem::new(Errors::Git, stderr))
    }
}

/// Checks if the remote repository is ahead of the local repository.
///
/// # Arguments
///
/// * `directory` - The local repository directory to check.
///
/// # Returns
///
/// Returns `true` if the remote is ahead, `false` otherwise.
///
/// # Errors
///
/// Returns an `ErrorArrayItem` if the Git commands fail.
async fn check_remote_ahead(directory: &PathType) -> Result<bool, ErrorArrayItem> {
    execute_git_command(&["-C", &directory.to_string(), "fetch"]).await?;

    let local_hash =
        execute_git_hash_command(&["-C", &directory.to_string(), "rev-parse", "@"]).await?;
    let remote_hash =
        execute_git_hash_command(&["-C", &directory.to_string(), "rev-parse", "@{u}"]).await?;

    Ok(remote_hash != local_hash)
}

/// Executes a Git command that returns a hash.
///
/// # Arguments
///
/// * `args` - A slice of command-line arguments to pass to Git.
///
/// # Returns
///
/// Returns the hash as a `String` if successful.
///
/// # Errors
///
/// Returns an `ErrorArrayItem` if the command execution fails.
async fn execute_git_hash_command(args: &[&str]) -> Result<String, ErrorArrayItem> {
    let output = Command::new("git")
        .args(args)
        .output()
        .await
        .map_err(ErrorArrayItem::from)?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(ErrorArrayItem::new(Errors::Git, stderr))
    }
}

/// Generates the project path based on the Git authentication information.
///
/// # Arguments
///
/// * `auth` - A reference to `GitAuth` containing branch, repository, and user information.
///
/// # Returns
///
/// Returns a `PathType` representing the project path.
pub fn generate_git_project_path(auth: &GitAuth) -> PathType {
    PathType::Content(format!("/var/www/ais/{}", generate_git_project_id(auth)))
}

/// Generates a unique project ID based on the Git authentication information.
///
/// # Arguments
///
/// * `auth` - A reference to `GitAuth` containing branch, repository, and user information.
///
/// # Returns
///
/// Returns a `Stringy` representing the truncated hash of the project ID.
pub fn generate_git_project_id(auth: &GitAuth) -> Stringy {
    let hash_input = format!("{}-{}-{}", auth.branch, auth.repo, auth.user);
    let hash = create_hash(hash_input);
    let truncated_hash = truncate(&hash, 8);
    truncated_hash.into()
}

impl fmt::Display for GitServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitServer::GitHub => write!(f, "{}", "GitHub.com".bold().cyan()),
            GitServer::GitLab => write!(f, "{}", "GitLab.com".bold().cyan()),
            GitServer::Custom(url) => write!(
                f,
                "{}: {}",
                "Custom Server".bold().cyan(),
                url.bold().yellow()
            ),
        }
    }
}

impl fmt::Display for GitAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}:",
            "Git Authentication Information".bold().underline().purple()
        )?;
        writeln!(f, "  {}: {}", "User".bold().cyan(), self.user)?;
        writeln!(f, "  {}: {}", "Repository".bold().cyan(), self.repo)?;
        writeln!(f, "  {}: {}", "Branch".bold().cyan(), self.branch)?;
        writeln!(f, "  {}: {}", "Server".bold().cyan(), self.server)?;
        if let Some(token) = &self.token {
            writeln!(
                f,
                "  {}: {}",
                "Token (Deprecated stop using)".bold().cyan(),
                token
            )?;
        } else {
            writeln!(f, "  {}", "Token: None".italic().dimmed())?;
        }
        Ok(())
    }
}

impl fmt::Display for GitCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", "Git Credentials".bold().underline().purple())?;
        if self.auth_items.is_empty() {
            writeln!(
                f,
                "  {}",
                "No Git Authentication Items Available".italic().dimmed()
            )?;
        } else {
            for (i, auth) in self.auth_items.iter().enumerate() {
                writeln!(f, "  {}:", format!("Auth Item {}", i + 1).bold().yellow())?;
                writeln!(f, "{}", auth)?;
            }
        }
        Ok(())
    }
}
