//! Per-app encrypted runtime bundle: one `.acai` container per deployed app
//! instance, replacing `Config.toml` + `Overrides.toml` + a bare `.env` file
//! for apps on this scheme.
//!
//! Centralized here (rather than reimplemented in watchdog) so every
//! consumer -- watchdog's runtime lifecycle, and any future standalone
//! debugging tooling -- shares one packing/unpacking implementation.
//!
//! A bundle holds three plain files in its directory tree:
//! - [`FIXED_CONFIG_ENTRY`] -- [`Enviornment_V2`] serialized as TOML.
//! - [`CUSTOM_CONFIG_ENTRY`] -- a [`CustomConfig`] serialized as JSON.
//! - [`ENV_ENTRY`] -- arbitrary `KEY=value` secrets/env-var text.
//!
//! Tamper-evidence for this content comes from `acai_core`'s own per-chunk
//! hashing and (once encrypted) AEAD authentication on every read -- these
//! are plain files in the Chunk Data Area, not State Section TLVs, so the
//! format's `IMMUTABLE_SET_HASH` TLV mechanism doesn't apply here and needs
//! no extra bookkeeping from this module; a corrupted or tampered container
//! simply fails to read (see the `tests` module for a demonstration).

use std::fs;
use std::path::Path;

use dusa_collection_utils::core::errors::{ErrorArrayItem, Errors};

use crate::custom_config::CustomConfig;
use crate::enviornment::definitions::Enviornment_V2;

/// The fixed, structural config (`Enviornment_V2`), TOML-shaped.
pub const FIXED_CONFIG_ENTRY: &str = "runtime.toml";
/// The flexible, per-app custom config, JSON-shaped.
pub const CUSTOM_CONFIG_ENTRY: &str = "custom.json";
/// Arbitrary `KEY=value` secrets/env-var content.
pub const ENV_ENTRY: &str = ".env";

/// A per-app config bundle has no business reserving `acai_core`'s 50MiB
/// default index region -- this is small enough for many commits' worth of
/// history on three small files while staying a fraction of the default.
pub const DEFAULT_INDEX_REGION_SIZE: u64 = 1_048_576; // 1 MiB

fn acai_err(context: &str, err: acai_core::AcaiError) -> ErrorArrayItem {
    ErrorArrayItem::new(Errors::GeneralError, format!("{context}: {err}"))
}

fn io_err(context: &str, err: std::io::Error) -> ErrorArrayItem {
    ErrorArrayItem::new(Errors::GeneralError, format!("{context}: {err}"))
}

/// Builds a brand-new encrypted bundle at `output_path` from the three
/// pieces, using `passphrase` (the node's bundle-decryption passphrase).
/// Fails if `output_path` already exists -- callers migrating an existing
/// app must check for that themselves (idempotency is a caller concern, not
/// this function's, since "does a bundle already exist" and "should it be
/// rebuilt" are different questions with different answers per call site).
pub fn build_bundle(
    output_path: &Path,
    fixed: &Enviornment_V2,
    custom: &CustomConfig,
    env_content: &str,
    passphrase: &str,
) -> Result<(), ErrorArrayItem> {
    let staging = tempfile::tempdir().map_err(|e| io_err("creating staging directory", e))?;

    let fixed_toml = toml::to_string(fixed)
        .map_err(|e| ErrorArrayItem::new(Errors::ConfigParsing, e.to_string()))?;
    fs::write(staging.path().join(FIXED_CONFIG_ENTRY), fixed_toml)
        .map_err(|e| io_err("staging fixed config", e))?;

    let custom_json = custom.to_json()?;
    fs::write(staging.path().join(CUSTOM_CONFIG_ENTRY), custom_json)
        .map_err(|e| io_err("staging custom config", e))?;

    fs::write(staging.path().join(ENV_ENTRY), env_content)
        .map_err(|e| io_err("staging env content", e))?;

    let request = serde_json::json!({
        "flags": { "encrypted": true },
        "passphrase": passphrase,
        "index_region_size": DEFAULT_INDEX_REGION_SIZE,
    });
    let request_bytes = serde_json::to_vec(&request).map_err(ErrorArrayItem::from)?;

    acai_core::build_container_file_from_directory(staging.path(), &request_bytes, output_path)
        .map_err(|e| acai_err("building runtime bundle", e))
}

/// Reads and decrypts one named entry out of `bundle_path` as raw bytes.
fn read_entry(bundle_path: &Path, entry: &str, passphrase: &str) -> Result<Vec<u8>, ErrorArrayItem> {
    let container = fs::read(bundle_path).map_err(|e| io_err("reading bundle file", e))?;
    acai_core::read_file(&container, entry, Some(passphrase))
        .map_err(|e| acai_err(&format!("reading '{entry}' from bundle"), e))
}

pub fn read_fixed_config(bundle_path: &Path, passphrase: &str) -> Result<Enviornment_V2, ErrorArrayItem> {
    let bytes = read_entry(bundle_path, FIXED_CONFIG_ENTRY, passphrase)?;
    let text = String::from_utf8(bytes).map_err(ErrorArrayItem::from)?;
    toml::from_str(&text).map_err(|e| ErrorArrayItem::new(Errors::ConfigParsing, e.to_string()))
}

pub fn read_custom_config(bundle_path: &Path, passphrase: &str) -> Result<CustomConfig, ErrorArrayItem> {
    let bytes = read_entry(bundle_path, CUSTOM_CONFIG_ENTRY, passphrase)?;
    let text = String::from_utf8(bytes).map_err(ErrorArrayItem::from)?;
    CustomConfig::from_json(&text)
}

pub fn read_env(bundle_path: &Path, passphrase: &str) -> Result<String, ErrorArrayItem> {
    let bytes = read_entry(bundle_path, ENV_ENTRY, passphrase)?;
    String::from_utf8(bytes).map_err(ErrorArrayItem::from)
}

/// Commits `content` as `entry`'s new value inside `bundle_path`, upserting
/// it (acai's commit semantics replace an existing path, no separate remove
/// needed). Requires the same `passphrase` the container was built with.
fn commit_entry(
    bundle_path: &Path,
    entry: &str,
    content: &[u8],
    passphrase: &str,
) -> Result<(), ErrorArrayItem> {
    let staged = tempfile::NamedTempFile::new().map_err(|e| io_err("staging commit content", e))?;
    fs::write(staged.path(), content).map_err(|e| io_err("writing commit content", e))?;

    let changeset = acai_core::Changeset {
        add: vec![acai_core::AddEntry {
            source_path: staged.path().to_path_buf(),
            container_path: entry.to_owned(),
        }],
        remove: Vec::new(),
        passphrase: Some(passphrase.to_owned()),
        threads: None,
        compression_level: None,
        compression_extreme: None,
        compression_dict_size: None,
        fast_compression_level: None,
    };
    let request_bytes = serde_json::to_vec(&changeset).map_err(ErrorArrayItem::from)?;

    acai_core::commit_to_container(bundle_path, &request_bytes)
        .map_err(|e| acai_err(&format!("committing '{entry}' to bundle"), e))
}

pub fn commit_fixed_config(
    bundle_path: &Path,
    fixed: &Enviornment_V2,
    passphrase: &str,
) -> Result<(), ErrorArrayItem> {
    let toml_text = toml::to_string(fixed)
        .map_err(|e| ErrorArrayItem::new(Errors::ConfigParsing, e.to_string()))?;
    commit_entry(bundle_path, FIXED_CONFIG_ENTRY, toml_text.as_bytes(), passphrase)
}

pub fn commit_custom_config(
    bundle_path: &Path,
    custom: &CustomConfig,
    passphrase: &str,
) -> Result<(), ErrorArrayItem> {
    let json_text = custom.to_json()?;
    commit_entry(bundle_path, CUSTOM_CONFIG_ENTRY, json_text.as_bytes(), passphrase)
}

pub fn commit_env(bundle_path: &Path, env_content: &str, passphrase: &str) -> Result<(), ErrorArrayItem> {
    commit_entry(bundle_path, ENV_ENTRY, env_content.as_bytes(), passphrase)
}

/// Writes the two control-plane pieces (fixed TOML + custom JSON) out to
/// `dest_dir` as plain files, for a `generic_runner`-based app to read
/// while it's running. Deliberately does not unpack [`ENV_ENTRY`] --
/// env-var content stays on the `ais_secretserver` read/write path (see the
/// plan's Phase E, E10) and is delivered to a running app by a different
/// mechanism, not by unpacking it to disk here.
pub fn unpack_control_plane_config(
    bundle_path: &Path,
    passphrase: &str,
    dest_dir: &Path,
) -> Result<(), ErrorArrayItem> {
    fs::create_dir_all(dest_dir).map_err(|e| io_err("creating config directory", e))?;

    let fixed = read_entry(bundle_path, FIXED_CONFIG_ENTRY, passphrase)?;
    fs::write(dest_dir.join(FIXED_CONFIG_ENTRY), fixed)
        .map_err(|e| io_err("writing unpacked fixed config", e))?;

    let custom = read_entry(bundle_path, CUSTOM_CONFIG_ENTRY, passphrase)?;
    fs::write(dest_dir.join(CUSTOM_CONFIG_ENTRY), custom)
        .map_err(|e| io_err("writing unpacked custom config", e))?;

    Ok(())
}

/// Deletes the two unpacked control-plane files from `dest_dir`, if present.
/// Called the moment watchdog detects an app's process is no longer running,
/// for any reason -- the plaintext window is exactly "while the process is
/// running."
pub fn cleanup_unpacked_config(dest_dir: &Path) -> Result<(), ErrorArrayItem> {
    for entry in [FIXED_CONFIG_ENTRY, CUSTOM_CONFIG_ENTRY] {
        let path = dest_dir.join(entry);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| io_err("removing unpacked config file", e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dusa_collection_utils::core::{logger::LogLevel, types::stringy::Stringy};

    fn sample_fixed() -> Enviornment_V2 {
        Enviornment_V2 {
            app_name: Stringy::from("demo"),
            max_ram_usage: 512,
            max_cpu_usage: 0,
            environment: Stringy::from("production"),
            debug_mode: false,
            log_level: LogLevel::Info,
            git: None,
            database: None,
            aggregator: None,
            interval_seconds: 30,
            monitor_path: Stringy::from("/opt/artisan/src/demo"),
            project_path: Stringy::from("/opt/artisan/src/demo"),
            changes_needed: 1,
            ignored_subdirs: vec![Stringy::from(".git")],
            install_command: None,
            build_command: None,
            run_command: Stringy::from("node server.js"),
            application_type: None,
            execution_uid: Some(33),
            execution_gid: Some(33),
            primary_listening_port: Some(3000),
            path_modifier: None,
            pre_build_command: None,
        }
    }

    fn scratch_bundle_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "artisan_runtime_bundle_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("runtime.acai")
    }

    #[test]
    fn round_trips_all_three_pieces() {
        let path = scratch_bundle_path("round_trip");
        let mut custom = CustomConfig::new();
        custom.set("feature_flag", true).unwrap();

        build_bundle(&path, &sample_fixed(), &custom, "API_KEY=abc123\n", "hunter2").unwrap();

        let fixed = read_fixed_config(&path, "hunter2").unwrap();
        assert_eq!(fixed.app_name.to_string(), "demo");

        let custom_back = read_custom_config(&path, "hunter2").unwrap();
        assert_eq!(custom_back.get::<bool>("feature_flag"), Some(true));

        let env = read_env(&path, "hunter2").unwrap();
        assert_eq!(env, "API_KEY=abc123\n");
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let path = scratch_bundle_path("wrong_pass");
        build_bundle(&path, &sample_fixed(), &CustomConfig::new(), "", "correct-horse").unwrap();

        assert!(read_fixed_config(&path, "wrong-guess").is_err());
    }

    #[test]
    fn commit_updates_a_single_entry_without_disturbing_others() {
        let path = scratch_bundle_path("commit");
        let mut custom = CustomConfig::new();
        custom.set("version", 1u32).unwrap();
        build_bundle(&path, &sample_fixed(), &custom, "OLD=1\n", "pw").unwrap();

        commit_env(&path, "NEW=2\n", "pw").unwrap();

        assert_eq!(read_env(&path, "pw").unwrap(), "NEW=2\n");
        // The fixed and custom entries must survive the env-only commit untouched.
        assert_eq!(read_fixed_config(&path, "pw").unwrap().app_name.to_string(), "demo");
        assert_eq!(read_custom_config(&path, "pw").unwrap().get::<u32>("version"), Some(1));
    }

    #[test]
    fn tampering_with_the_bundle_bytes_is_detected() {
        let path = scratch_bundle_path("tamper");
        build_bundle(&path, &sample_fixed(), &CustomConfig::new(), "SECRET=1\n", "pw").unwrap();

        // Flip every 97th byte across the whole file (a single byte near
        // either end can land in the reserved-but-unused index region or
        // trailing chunk padding, which flipping doesn't corrupt anything
        // hash/AEAD-checked) -- scattering the flips reliably hits real
        // chunk content regardless of exact layout.
        let mut bytes = fs::read(&path).unwrap();
        for i in (0..bytes.len()).step_by(97) {
            bytes[i] ^= 0xFF;
        }
        fs::write(&path, bytes).unwrap();

        assert!(
            read_env(&path, "pw").is_err(),
            "a bit-flipped bundle must fail to read, not silently return corrupted content"
        );
    }

    #[test]
    fn unpack_writes_only_the_two_control_plane_files() {
        let path = scratch_bundle_path("unpack");
        build_bundle(&path, &sample_fixed(), &CustomConfig::new(), "SECRET=1\n", "pw").unwrap();

        let dest = std::env::temp_dir().join(format!("artisan_runtime_unpack_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);

        unpack_control_plane_config(&path, "pw", &dest).unwrap();
        assert!(dest.join(FIXED_CONFIG_ENTRY).exists());
        assert!(dest.join(CUSTOM_CONFIG_ENTRY).exists());
        assert!(!dest.join(ENV_ENTRY).exists(), "env content must not be unpacked to disk");

        cleanup_unpacked_config(&dest).unwrap();
        assert!(!dest.join(FIXED_CONFIG_ENTRY).exists());
        assert!(!dest.join(CUSTOM_CONFIG_ENTRY).exists());

        let _ = fs::remove_dir_all(&dest);
    }
}
