use dusa_collection_utils::{
    core::errors::{ErrorArrayItem, Errors},
    core::logger::LogLevel,
    core::types::{pathtype::PathType, stringy::Stringy},
    log,
};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    time::Duration,
};
use tokio::time::sleep;

use crate::{encryption::simple_encrypt, timestamp::current_timestamp};

#[cfg(target_os = "linux")]
use dusa_collection_utils::platform::functions::{create_hash, truncate};

// TODO When we come in here to re-organize, I want to add a field  for a registration date in our identity value and have that contribute to that hash. 
// TODO I also want to rename the hash to shadow or something similar, I want to use it as a garuntee that a message came from a given node, idk how yet 


/// The file path to store the `Identifier` object on disk.
pub const IDENTITYPATHSTR: &str = "/opt/artisan/.identity";

/// The length to which cryptographic signatures (hashes) should be truncated.
pub const HASH_LENGTH: usize = 28;

/// A custom epoch used by the snowflake-based ID generator.  
/// This value represents an offset subtracted from the current Unix timestamp
/// to keep the resulting IDs relatively smaller.
pub const CUSTOM_EPOCH: u64 = 1_047_587_400;

/// A Snowflake-like ID generator for creating (generally) unique 64-bit IDs.
///
/// # Overview
/// Inspired by Twitter’s Snowflake algorithm, this generator splits the 64-bit ID as follows:
/// - **Bits 63..=22**: A timestamp offset from [`CUSTOM_EPOCH`].
/// - **Bits 21..=17**: Datacenter ID (5 bits).
/// - **Bits 16..=12**: Machine ID (5 bits).
/// - **Bits 11..=0**: Sequence number (12 bits).
///
/// The sequence number ensures uniqueness within the same millisecond and resets each time
/// the timestamp changes.
pub struct SnowflakeIDGenerator {
    /// The custom epoch offset from which we calculate the timestamp.
    custom_epoch: u64,
    /// A 5-bit identifier for the datacenter (0–31).
    datacenter_id: u8,
    /// A 5-bit identifier for the machine/host (0–31).
    machine_id: u8,
    /// Sequence counter that increments if multiple IDs are generated within the same millisecond.
    sequence: u16,
    /// The timestamp (milliseconds) for the last generated ID.
    last_timestamp: u64,
}

#[cfg(target_os = "linux")]
impl SnowflakeIDGenerator {
    /// Creates a new `SnowflakeIDGenerator` with the provided datacenter and machine IDs.
    ///
    /// # Errors
    /// Returns an `Err(())` if `datacenter_id` or `machine_id` is greater than 31 (i.e.,
    /// does not fit in 5 bits).
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::identity::SnowflakeIDGenerator;
    /// match SnowflakeIDGenerator::new(1, 2) {
    ///     Ok(generator) => {
    ///         // success
    ///     }
    ///     Err(_) => eprintln!("Invalid datacenter or machine ID"),
    /// }
    /// ```
    pub fn new(datacenter_id: u8, machine_id: u8) -> Result<Self, ()> {
        if datacenter_id > 31 {
            log!(LogLevel::Error, "Datacenter ID must be between 0 and 31");
            return Err(());
        }

        if machine_id > 31 {
            log!(LogLevel::Error, "Machine ID must be between 0 and 31");
            return Err(());
        }

        Ok(Self {
            custom_epoch: CUSTOM_EPOCH,
            datacenter_id,
            machine_id,
            sequence: 0,
            last_timestamp: 0,
        })
    }

    /// Waits until the system clock moves to the next millisecond if `last_timestamp`
    /// has not advanced.
    ///
    /// # Internal Behavior
    /// This repeatedly queries [`current_timestamp`] until it exceeds `last_timestamp`.
    fn wait_for_next_millis(last_timestamp: u64) -> u64 {
        let mut timestamp = current_timestamp();
        while timestamp <= last_timestamp {
            timestamp = current_timestamp();
        }
        timestamp
    }

    /// Asynchronously generates a new 64-bit Snowflake ID.
    ///
    /// # Details
    /// - If the current timestamp is behind the last generated timestamp (clock drift),
    ///   this method sleeps for 10ms to wait for the clock to catch up.
    /// - If the current timestamp matches the last timestamp, it increments the sequence number.
    ///   If the sequence number overflows (exceeds 4095), it blocks until the timestamp advances.
    /// - The final 64-bit ID is constructed with timestamp, datacenter ID, machine ID,
    ///   and sequence fields.
    ///
    /// # Return
    /// Returns a `u64` with the generated Snowflake ID.
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use artisan_middleware::identity::SnowflakeIDGenerator;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let mut generator = SnowflakeIDGenerator::new(1, 2).unwrap();
    ///     let new_id = generator.generate_id().await;
    ///     println!("Generated ID: {}", new_id);
    /// # });
    /// ```
    pub async fn generate_id(&mut self) -> u64 {
        let mut timestamp = current_timestamp();

        if timestamp < self.last_timestamp {
            sleep(Duration::from_millis(10)).await;
            if timestamp < self.last_timestamp {
                log!(
                    LogLevel::Error,
                    "Clock moved backwards. Refusing to generate ID."
                );
                return 0;
            }
        }

        if timestamp == self.last_timestamp {
            self.sequence = (self.sequence + 1) & 0xFFF; // 12 bits max
            if self.sequence == 0 {
                timestamp = Self::wait_for_next_millis(self.last_timestamp);
            }
        } else {
            self.sequence = 0;
        }

        self.last_timestamp = timestamp;

        // Construct the 64-bit ID
        ((timestamp - self.custom_epoch) << 22)
            | ((self.datacenter_id as u64) << 17)
            | ((self.machine_id as u64) << 12)
            | (self.sequence as u64)
    }
}

/// Represents a basic identifier that pairs a numeric `id` with a cryptographic signature.
///
/// # Fields
/// - `id`: A 64-bit integer (often generated via [`SnowflakeIDGenerator`]).
/// - `_signature`: A truncated hash of the ID used to verify integrity.
///
/// # Notes
/// This struct includes file I/O routines to persist or load an `Identifier` from disk.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier {
    /// The numeric identifier (64-bit).
    pub id: u64,
    /// A truncated hash of `id`. Used for verification of integrity.
    _signature: Stringy,
}

#[cfg(target_os = "linux")]
impl Identifier {
    /// Generates a truncated hash (`Stringy`) from the given `id`.
    ///
    /// # Internal Usage
    /// This function is used within [`Identifier::new`] and [`Identifier::verify`]
    /// to create or compare the internal `_signature`.
    fn generate_signature(id: u64) -> Stringy {
        truncate(&*create_hash(format!("{}", id)), HASH_LENGTH)
    }

    /// Creates a new [`Identifier`] by generating a random datacenter and machine ID (1–5),
    /// constructing a [`SnowflakeIDGenerator`], and producing a fresh snowflake `id`.
    ///
    /// # Returns
    /// - `Ok(Identifier)`: Successfully generated an ID with signature.
    /// - `Err(ErrorArrayItem)`: Failure generating the ID (e.g., if Snowflake generator fails).
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use artisan_middleware::identity::Identifier;
    /// let rt = Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     match Identifier::new().await {
    ///         Ok(ident) => println!("New ID: {}", ident.id),
    ///         Err(err) => eprintln!("Error generating Identifier: {}", err),
    ///     }
    /// });
    /// ```
    pub async fn new() -> Result<Self, ErrorArrayItem> {
        // ! Using the first 5 out of 31 bits (1..=5) for random datacenter/machine ID
        let datacenter_id = rand::rng().random_range(1..=5);
        let machine_id = rand::rng().random_range(1..=5);

        let mut big_id: SnowflakeIDGenerator = SnowflakeIDGenerator::new(datacenter_id, machine_id)
            .map_err(|_| {
                ErrorArrayItem::new(
                    Errors::GeneralError,
                    "Error generating system ID".to_owned(),
                )
            })?;

        let id = big_id.generate_id().await;

        Ok(Self {
            id,
            _signature: Self::generate_signature(id),
        })
    }

    /// Verifies the integrity of the `Identifier` by re-generating the signature from `id`
    /// and comparing it to the stored `_signature`.
    ///
    /// # Returns
    /// - `true` if the computed signature matches.
    /// - `false` otherwise.
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use artisan_middleware::identity::Identifier;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let ident = Identifier::new().await.unwrap();
    ///     assert!(ident.verify().await);
    /// # });
    /// ```
    pub async fn verify(&self) -> bool {
        let given_signature = self._signature.clone();
        let new_signature = Self::generate_signature(self.id);
        given_signature == new_signature
    }

    /// Loads an `Identifier` from the file system (at [`IDENTITYPATHSTR`]) if it exists.
    /// If the file is not found or loading fails, returns `Ok(None)`.
    ///
    /// # Returns
    /// - `Ok(Some(Identifier))` if successfully loaded.
    /// - `Ok(None)` if the file does not exist or is invalid.
    /// - `Err(ErrorArrayItem)` if a critical I/O or JSON parsing error occurs.
    pub async fn load() -> Result<Option<Self>, ErrorArrayItem> {
        let identifier_path: PathType = PathType::Str(IDENTITYPATHSTR.into());
        if identifier_path.exists() {
            match Self::load_from_file() {
                Ok(data) => return Ok(Some(data)),
                Err(err) => {
                    log!(LogLevel::Trace, "ERROR: Failed to load identity: {}", err);
                    return Ok(None);
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Saves the `Identifier` to a file at [`IDENTITYPATHSTR`], overwriting any previous data.
    ///
    /// # Returns
    /// - `Ok(())` if successful.
    /// - `Err(ErrorArrayItem)` if file creation or writing fails.
    pub fn save_to_file(&self) -> Result<(), ErrorArrayItem> {
        let serialized_id = serde_json::to_string_pretty(&self)?;
        let mut file = std::fs::File::create(PathType::Str(IDENTITYPATHSTR.into()))?;
        let mut flag = std::fs::File::create(PathType::Str("/opt/artisan/.system_ready".into()))?;
        file.write_all(serialized_id.as_bytes())?;
        flag.write_all(serialized_id.as_bytes())?;
        Ok(())
    }
    /// Loads an `Identifier` from the file at [`IDENTITYPATHSTR`].
    ///
    /// # Returns
    /// - `Ok(Identifier)` on success.
    /// - `Err(ErrorArrayItem)` if reading or deserialization fails.
    pub fn load_from_file() -> Result<Self, ErrorArrayItem> {
        let mut file = std::fs::File::open(PathType::Str(IDENTITYPATHSTR.into()))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        let identifier: Identifier = serde_json::from_str(&content)?;
        Ok(identifier)
    }

    /// Serializes the `Identifier` into a prettified JSON string.
    ///
    /// # Returns
    /// - `Ok(String)` containing JSON on success.
    /// - `Err(ErrorArrayItem)` if serialization fails.
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use artisan_middleware::identity::Identifier;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let ident = Identifier::new().await.unwrap();
    ///     match ident.to_json() {
    ///         Ok(json_str) => println!("JSON: {}", json_str),
    ///         Err(err) => eprintln!("Failed to serialize Identifier: {}", err),
    ///     }
    /// # });
    /// ```
    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        let json_representation = serde_json::to_string_pretty(self)?;
        Ok(json_representation)
    }

    /// Converts the `Identifier` into JSON and then encrypts the JSON using [`simple_encrypt`].
    ///
    /// # Returns
    /// - `Ok(Stringy)` containing the encrypted data on success.
    /// - `Err(ErrorArrayItem)` if JSON creation or encryption fails.
    ///
    /// # Example
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// # use artisan_middleware::identity::Identifier;
    /// # let rt = Runtime::new().unwrap();
    /// # rt.block_on(async {
    ///     let ident = Identifier::new().await.unwrap();
    ///     match ident.to_encrypted_json().await {
    ///         Ok(enc_str) => println!("Encrypted JSON: {}", enc_str),
    ///         Err(err) => eprintln!("Encryption failed: {}", err),
    ///     }
    /// # });
    /// ```
    pub async fn to_encrypted_json(&self) -> Result<Stringy, ErrorArrayItem> {
        let json_representation = self.to_json().map_err(|e| {
            ErrorArrayItem::new(
                dusa_collection_utils::core::errors::Errors::JsonCreation,
                e.to_string(),
            )
        })?;
        let encrypted_data = simple_encrypt(json_representation.as_bytes())?;
        Ok(encrypted_data)
    }

    /// Logs the numeric `id` at debug level.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::identity::Identifier;
    /// # use dusa_collection_utils::core::types::stringy::Stringy;
    /// # use tokio::runtime::Runtime;
    /// let rt = Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let ident = Identifier::new().await.unwrap();
    ///     ident.display_id(); // Logs "ID: 12345" at Debug level
    /// # });
    /// ```
    pub fn display_id(&self) {
        log!(LogLevel::Debug, "ID: {}", self.id);
    }

    /// Logs the `_signature` at debug level.
    ///
    /// # Example
    /// ```rust
    /// # use artisan_middleware::identity::Identifier;
    /// # use dusa_collection_utils::core::types::stringy::Stringy;
    /// # use tokio::runtime::Runtime;
    /// let rt = Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let ident = Identifier::new().await.unwrap();
    ///     ident.display_sig(); // Logs "SIG: sig" at Debug level
    /// # });
    /// ```
    pub fn display_sig(&self) {
        log!(LogLevel::Debug, "SIG: {}", self._signature);
    }
}

// =============================================================================
// Resource Taxonomy — canonical ID newtypes
//
// See `RESOURCE_TAXONOMY.md` at the repository root for the normative spec.
// These types exist so that "which flavor of identifier is this" is answered
// by the type system instead of by a field name convention that different
// services have historically spelled differently (`runner_id`/`app_id`/
// `project_id`, `org_id`/`organization_id`, etc.).
// =============================================================================

use std::fmt;

/// The prefix applied to a project's `project_id` when it is used as a
/// systemd/process name (e.g. `ais_63c35f4b`). This is a process-naming
/// convention, not part of a resource's identity -- never derive a
/// `project_id` from a process name (or vice versa) with ad hoc string
/// surgery like `.replace("ais_", "")`; that is not prefix-anchored and
/// corrupts any identifier that happens to contain the substring `ais_`
/// anywhere other than as a leading prefix. Use [`ais_name`] / [`strip_ais_prefix`].
pub const AIS_PREFIX: &str = "ais_";

/// Builds a process/systemd name from a bare component name.
///
/// Invertible via [`strip_ais_prefix`]: `strip_ais_prefix(&ais_name(x)) == Some(x)`
/// for every `x`.
pub fn ais_name(component: &str) -> String {
    format!("{AIS_PREFIX}{component}")
}

/// The inverse of [`ais_name`]. Returns `None` if `name` does not start with
/// [`AIS_PREFIX`] -- unlike `name.replace("ais_", "")`, this never mangles a
/// name that merely *contains* `ais_` somewhere other than as a prefix.
pub fn strip_ais_prefix(name: &str) -> Option<&str> {
    name.strip_prefix(AIS_PREFIX)
}

/// Error returned when a string does not have the expected shape for a
/// particular ID type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdParseError {
    pub expected: &'static str,
    pub got: String,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, got {:?}", self.expected, self.got)
    }
}

impl std::error::Error for IdParseError {}

fn is_lowercase_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Identifies a Project: the deployable unit a customer thinks of as "my app".
/// Backed by an 8-character lowercase-hex string, unchanged from the
/// pre-existing `sha256("{branch}-{repo}-{user}")[0..8]` algorithm -- see
/// [`generate_project_id`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(String);

impl ProjectId {
    /// Validates and wraps an existing 8-hex-character project id.
    pub fn parse(s: &str) -> Result<Self, IdParseError> {
        if is_lowercase_hex(s, 8) {
            Ok(Self(s.to_owned()))
        } else {
            Err(IdParseError {
                expected: "8 lowercase hex characters",
                got: s.to_owned(),
            })
        }
    }

    /// Computes the canonical project id from its git identity: `user`, `repo`,
    /// `branch`. This is the SAME algorithm as the pre-existing
    /// `GitAuth::generate_id`/`generate_git_project_id` (now removed in favor
    /// of this single implementation) -- idempotent, deterministic,
    /// collision-resistant (32 bits), URL-safe.
    pub fn from_parts(user: &str, repo: &str, branch: &str) -> Self {
        let hash_input = format!("{branch}-{repo}-{user}");
        let hash = dusa_collection_utils::platform::functions::create_hash(hash_input);
        let truncated = dusa_collection_utils::platform::functions::truncate(&*hash, 8);
        Self(truncated.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The systemd/process name for this project (`ais_<project_id>`).
    pub fn ais_name(&self) -> String {
        ais_name(&self.0)
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> Self {
        id.0
    }
}

/// Computes the canonical project id from a [`crate::git_actions::GitAuth`].
/// The single canonical replacement for the two prior duplicate
/// implementations (`GitAuth::generate_id`, the free function
/// `generate_git_project_id`).
pub fn generate_project_id(user: &str, repo: &str, branch: &str) -> ProjectId {
    ProjectId::from_parts(user, repo, branch)
}

/// A UUID-backed identifier. Used for [`OrganizationId`], [`InstanceId`], and
/// [`SessionId`] -- resources created without a central coordinator handing
/// out sequential IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UuidId(String);

impl UuidId {
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn parse(s: &str) -> Result<Self, IdParseError> {
        uuid::Uuid::parse_str(s)
            .map(|u| Self(u.to_string()))
            .map_err(|_| IdParseError {
                expected: "a UUID",
                got: s.to_owned(),
            })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UuidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

macro_rules! uuid_id_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(UuidId);

        impl $name {
            pub fn new_v4() -> Self {
                Self(UuidId::new_v4())
            }

            pub fn parse(s: &str) -> Result<Self, IdParseError> {
                UuidId::parse(s).map(Self)
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

uuid_id_newtype!(
    OrganizationId,
    "Identifies an Organization -- the tenancy boundary that owns every other resource."
);
uuid_id_newtype!(
    InstanceId,
    "Identifies one running copy of a Project on one Node."
);
uuid_id_newtype!(SessionId, "Identifies a Runpod GPU compute session.");

/// A `u64`-backed identifier that MUST be encoded as a decimal string on
/// HTTP/JSON boundaries (to avoid JavaScript's float-precision loss above
/// 2^53), while remaining a plain `u64` in Rust and `uint64` in proto. Used
/// for [`NodeId`], [`DomainId`], and [`VmId`].
macro_rules! wire_string_u64_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl $name {
            pub fn get(&self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name {
            fn from(v: u64) -> Self {
                Self(v)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.0.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct WireVisitor;
                impl<'de> serde::de::Visitor<'de> for WireVisitor {
                    type Value = u64;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, "a u64 or a decimal string")
                    }

                    fn visit_u64<E>(self, v: u64) -> Result<u64, E> {
                        Ok(v)
                    }

                    fn visit_i64<E>(self, v: i64) -> Result<u64, E>
                    where
                        E: serde::de::Error,
                    {
                        u64::try_from(v).map_err(|_| E::custom("negative value for u64 id"))
                    }

                    fn visit_str<E>(self, v: &str) -> Result<u64, E>
                    where
                        E: serde::de::Error,
                    {
                        v.parse::<u64>().map_err(E::custom)
                    }
                }
                deserializer.deserialize_any(WireVisitor).map($name)
            }
        }
    };
}

wire_string_u64_id!(NodeId, "Identifies a compute Node.");
wire_string_u64_id!(DomainId, "Identifies a Domain.");
wire_string_u64_id!(VmId, "Identifies a Proxmox-managed Vm.");

impl From<Identifier> for NodeId {
    fn from(identifier: Identifier) -> Self {
        NodeId(identifier.id)
    }
}

/// Identifies a named deployment scope (`prod`, `staging`, `dev`, ...) under a Project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnvironmentId(String);

impl EnvironmentId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EnvironmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EnvironmentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EnvironmentId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A Secret's composite identity: scoped to a Project and an Environment.
/// Deliberately not a bare scalar -- do not invent a synthetic `secret_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef {
    pub project_id: ProjectId,
    pub environment_id: EnvironmentId,
    pub key: String,
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.project_id, self.environment_id, self.key)
    }
}
