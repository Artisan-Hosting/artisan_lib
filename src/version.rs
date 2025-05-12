use dusa_collection_utils::{
    core::types::stringy::Stringy,
    core::version::{Version, VersionCode},
};

use crate::RELEASEINFO;

pub fn aml_version() -> Version {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts = version.split('.');

    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    let patch = parts.next().unwrap_or("0");

    Version {
        number: Stringy::from(format!("{}.{}.{}", major, minor, patch)),
        code: RELEASEINFO,
    }
}

pub fn str_to_version(cargo_pkg_version: &str, release_code: Option<VersionCode>) -> Version {
    let mut parts = cargo_pkg_version.split('.');

    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    let patch = parts.next().unwrap_or("0");

    let code: VersionCode = match release_code {
        Some(code) => code,
        None => RELEASEINFO,
    };

    Version {
        number: Stringy::from(format!("{}.{}.{}", major, minor, patch)),
        code,
    }
}
