//! Loads mTLS certificate material (certificate, key, and CA chain) from disk as raw PEM bytes.
//!
//! This crate intentionally imposes NO hard dependency on `tonic`, `rustls`, or any TLS library.
//! Individual consumer services each pin their own version of `tonic` independently, and are
//! responsible for converting the raw bytes returned here into their respective TLS identities
//! (e.g. `tonic::Identity::from_pem(cert, key)`), preserving version compatibility across the platform.

use std::path::PathBuf;
use dusa_collection_utils::core::errors::{ErrorArrayItem, Errors};

/// Contains raw PEM-encoded mTLS material as byte vectors.
pub struct MtlsMaterial {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub ca_pem: Vec<u8>,
}

/// Configuration for where to locate mTLS certificate material on disk.
pub struct MtlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub ca_cert_path: PathBuf,
}

/// Reads mTLS certificate material from disk as raw bytes.
///
/// # Arguments
/// * `cfg` - Configuration specifying paths to the cert, key, and CA PEM files.
///
/// # Returns
/// * `Ok(MtlsMaterial)` containing the PEM contents for cert, key, and CA as `Vec<u8>`.
/// * `Err(ErrorArrayItem)` if any file fails to read.
pub fn load_mtls_material(cfg: &MtlsConfig) -> Result<MtlsMaterial, ErrorArrayItem> {
    let cert_pem = std::fs::read(&cfg.cert_path)
        .map_err(|e| ErrorArrayItem::new(
            Errors::GeneralError,
            format!("failed to read cert at {:?}: {}", cfg.cert_path, e)
        ))?;

    let key_pem = std::fs::read(&cfg.key_path)
        .map_err(|e| ErrorArrayItem::new(
            Errors::GeneralError,
            format!("failed to read key at {:?}: {}", cfg.key_path, e)
        ))?;

    let ca_pem = std::fs::read(&cfg.ca_cert_path)
        .map_err(|e| ErrorArrayItem::new(
            Errors::GeneralError,
            format!("failed to read CA cert at {:?}: {}", cfg.ca_cert_path, e)
        ))?;

    Ok(MtlsMaterial { cert_pem, key_pem, ca_pem })
}
