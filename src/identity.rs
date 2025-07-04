use dusa_collection_utils::{
    core::errors::{ErrorArrayItem, Errors},
    log,
    core::logger::LogLevel,
    core::types::{pathtype::PathType, stringy::Stringy},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    time::Duration,
};
use tokio::time::sleep;

use crate::{encryption::simple_encrypt, timestamp::current_timestamp};

#[cfg(target_os = "linux")]
use dusa_collection_utils::platform::functions::{create_hash, truncate};

/// The file path to store the `Identifier` object on disk.
pub const IDENTITYPATHSTR: &str = "/opt/artisan/identity";

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
        let datacenter_id = rand::thread_rng().gen_range(1..=5);
        let machine_id = rand::thread_rng().gen_range(1..=5);

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
        file.write_all(serialized_id.as_bytes())?;
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
