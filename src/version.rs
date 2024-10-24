use std::fmt;

use colored::Colorize;
use dusa_collection_utils::stringy::Stringy;
use serde::{Deserialize, Serialize};

/// Current version of the protocol, derived from the package version.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHANNEL: AisCode = AisCode::Beta;

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Clone)]
pub struct SoftwareVersion {
    pub application: Version,
    pub library: Version,
}

impl SoftwareVersion {
    pub fn new(cargo: &str) -> Self {
        Self {
            application: Version::from_crate(cargo),
            library: Version::get_raw(),
        }
    }
    pub fn dummy() -> Self {
        Self {
            application: Version::get_raw(),
            library: Version::get_raw(),
        }
    }
}

impl fmt::Display for SoftwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Application Version: {}, Library Version: {}",
            self.application, self.library
        )
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Clone)]
pub struct Version {
    pub number: Stringy,
    pub code: AisCode,
}

/// Enumeration representing different version codes.
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Clone)]
pub enum AisCode {
    /// Production version.
    Production,
    /// Production candidate version.
    ProductionCandidate,
    /// Beta version.
    Beta,
    /// Alpha version.
    Alpha,
    /// Patched
    Patched, // If a quick patch is issued before the platform is updated we can use this code
             // ! This code will ignore compatibility checks BE MINDFUL
}

impl fmt::Display for AisCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ais_code = match self {
            AisCode::Production => "P",
            AisCode::ProductionCandidate => "RC",
            AisCode::Beta => "b",
            AisCode::Alpha => "a",
            AisCode::Patched => "*",
        };
        write!(f, "{}", ais_code.bold().red())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data = self;
        write!(f, "{}{}", data.number.bold().green(), data.code)
    }
}

impl Version {
    /// Get the current version of the library as a filled struct
    pub fn get_raw() -> Self {
        Version {
            number: VERSION.into(),
            code: CHANNEL,
        }
    }

    /// Get the version from the provided application version string (crate version)
    pub fn from_crate(cargo: &str) -> Self {
        Version {
            number: cargo.into(),
            code: CHANNEL,
        }
    }

    /// Get the current version String
    pub fn get() -> Stringy {
        Stringy::new(&Self::get_raw().to_string())
    }

    /// Checks if a version number given is compatible with the current version
    pub fn comp_raw(&self, incoming: &Version) -> bool {
        match (&incoming.code, &self.code) {
            (AisCode::Alpha, AisCode::Alpha) => true,
            (AisCode::Beta, AisCode::Beta)
            | (AisCode::Beta, AisCode::Alpha)
            | (AisCode::Alpha, AisCode::Beta) => true,
            (AisCode::ProductionCandidate, AisCode::ProductionCandidate)
            | (AisCode::ProductionCandidate, AisCode::Beta)
            | (AisCode::Beta, AisCode::ProductionCandidate) => {
                let (inc_major, _) = Self::parse_version(&incoming.number).unwrap();
                let (ver_major, _) = Self::parse_version(VERSION).unwrap();
                inc_major == ver_major
            }
            (AisCode::Production, AisCode::ProductionCandidate)
            | (AisCode::ProductionCandidate, AisCode::Production)
            | (AisCode::Production, AisCode::Production) => {
                let (inc_major, inc_minor) = Self::parse_version(&incoming.number).unwrap();
                let (ver_major, ver_minor) = Self::parse_version(VERSION).unwrap();
                inc_major == ver_major && inc_minor == ver_minor
            }
            _ => false,
        }
    }

    pub fn comp(data: Stringy) -> bool {
        let version = match Self::from_stringy(data) {
            Some(d) => d,
            None => return false,
        };
        version.comp_raw(&version)
    }

    pub fn to_string(&self) -> String {
        format!("{}{}", self.number, self.code)
    }

    /// Converts a received string into a Version struct
    fn from_string(s: String) -> Option<Self> {
        let pos = s.chars().position(|c| !c.is_digit(10) && c != '.');
        if let Some(pos) = pos {
            let number = &s[..pos];
            let code_str = &s[pos..];
            let code = match code_str {
                "P" => AisCode::Production,
                "RC" => AisCode::ProductionCandidate,
                "b" => AisCode::Beta,
                "a" => AisCode::Alpha,
                "*" => AisCode::Patched,
                _ => return None,
            };
            Some(Version {
                number: Stringy::new(number),
                code,
            })
        } else {
            None
        }
    }

    pub fn from_stringy(s: Stringy) -> Option<Self> {
        Self::from_string(s.to_string())
    }

    fn parse_version(v: &str) -> Option<(u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let major: u32 = parts[0].parse::<u32>().ok()?;
        let minor: u32 = parts[1].parse::<u32>().ok()?;
        Some((major, minor))
    }
}

impl SoftwareVersion {
    /// Compare the application and library versions to the incoming SoftwareVersion
    pub fn comp_versions(&self, incoming: &SoftwareVersion) -> bool {
        let app_comp = Version::comp_raw(&self.application, &incoming.application);
        let lib_comp = Version::comp_raw(&self.library, &incoming.library);
        app_comp && lib_comp
    }
}
