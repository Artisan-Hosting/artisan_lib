use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit, Nonce};
use dusa_collection_utils::{log, core::logger::LogLevel, core::types::stringy::Stringy};
use rand::Rng;
use tokio::sync::Notify;

use dusa_collection_utils::core::errors::{ErrorArrayItem, Errors, UnifiedResult};
#[cfg(target_os = "linux")]
use recs::{decrypt_raw, encrypt_raw, house_keeping, initialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::sleep;

#[cfg(target_os = "linux")]
lazy_static::lazy_static! {
/// Indicates whether the legacy (RECS-based) encryption system has been initialized.
static ref initialized:  Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    /// Tracks if the cleaning loop used by the RECS system has been spawned.
    static ref cleaning_loop_initialized: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    /// A `Notify` instance used to trigger a "cleaning" operation within RECS.
    static ref cleaning_call: Arc<Notify> = Arc::new(Notify::new());

    /// Indicates whether the encryption/decryption operations are currently "locked" while cleaning.
    static ref cleaning_lock: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

// region: Legacy Encryption/Decryption

/// Encrypts text data using the legacy RECS-based encryption system.
///
/// # Deprecation
/// Marked as **deprecated** since version 4.3.0.
/// Please use [`simple_encrypt`] instead if possible.
///
/// # Arguments
/// - `data`: The [`Stringy`] text to encrypt.
///
/// # Returns
/// - `Ok(Stringy)`: The encrypted data as a `Stringy`.
/// - `Err(ErrorArrayItem)`: An error if encryption fails.
///
/// # Example
/// ```rust
/// # use dusa_collection_utils::types::stringy::Stringy;
/// # use tokio::runtime::Runtime;
/// # use std::time::Duration;
/// # use artisan_middleware::encryption::encrypt_text;
/// # let rt = Runtime::new().unwrap();
/// # let text = Stringy::from("sensitive information");
/// # rt.block_on(async {
///     
///     #[allow(deprecated)]
///     match encrypt_text(text).await {
///         Ok(encrypted) => println!("Encrypted data: {}", encrypted),
///         Err(err) => eprintln!("Encryption failed: {}", err),
///     }
///
///  # });
/// ```
#[allow(deprecated)]
#[cfg(target_os = "linux")]
#[deprecated(
    since = "4.3.0",
    note = "Currently unstable. Use `simple_encrypt` if possible."
)]
pub async fn encrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
    let data_bytes = data.as_bytes().to_vec();
    let plain_bytes = encrypt_data(&data_bytes).await.uf_unwrap()?;

    let text = Stringy::from(String::from_utf8(plain_bytes)?);
    Ok(text)
}

/// Decrypts text data using the legacy RECS-based decryption system.
///
/// # Deprecation
/// Marked as **deprecated** since version 4.3.0.
/// Please use [`simple_decrypt`] instead if possible.
///
/// # Arguments
/// - `data`: The [`Stringy`] text to decrypt.
///
/// # Returns
/// - `Ok(Stringy)`: The decrypted data as a `Stringy`.
/// - `Err(ErrorArrayItem)`: An error if decryption fails.
///
/// # Example
/// ```rust
/// # use dusa_collection_utils::types::stringy::Stringy;
/// # use tokio::runtime::Runtime;
/// # use std::time::Duration;
/// # use artisan_middleware::encryption::decrypt_text;
/// # use artisan_middleware::encryption::encrypt_text;
/// # let rt = Runtime::new().unwrap();
/// # let text = Stringy::from("sensitive information");
/// # rt.block_on(async {
///
///     #[allow(deprecated)]
///     let encrypted = encrypt_text(text).await.unwrap();
///
///     #[allow(deprecated)]
///     match decrypt_text(encrypted).await {
///         Ok(decrypted) => println!("Decrypted data: {}", decrypted),
///         Err(err) => eprintln!("Decryption failed: {}", err),
///     }
///
/// # });
/// ```
#[allow(deprecated)]
#[cfg(target_os = "linux")]
#[deprecated(
    since = "4.3.0",
    note = "Currently unstable. Use `simple_decrypt` if possible."
)]
pub async fn decrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
    let data_bytes: &[u8] = data.as_bytes();
    let decrypted_bytes: Vec<u8> = decrypt_data(&data_bytes).await.uf_unwrap()?;
    let decrypted_string: String = String::from_utf8(decrypted_bytes)?;
    let decrypted_stringy: Stringy = Stringy::Immutable(Arc::<str>::from(decrypted_string));

    Ok(decrypted_stringy)
}

/// Encrypts raw byte data using the legacy RECS-based encryption system, producing
/// a `UnifiedResult<Vec<u8>>` containing the cipher text (with key & other metadata).
///
/// # Deprecation
/// Marked as **deprecated** since version 4.3.0.
/// Please use [`simple_encrypt`] instead if possible.
///
/// # Arguments
/// - `data`: The byte slice to encrypt.
///
/// # Returns
/// - `UnifiedResult<Vec<u8>>`: On success, returns a byte vector containing the encrypted data.
///   On failure, returns an `ErrorArrayItem` describing what went wrong.
///
/// # Behavior
/// This function attempts multiple times (up to `attempts`) to acquire a lock if the
/// system is busy. If it remains locked, it returns an error.
#[deprecated(
    since = "4.3.0",
    note = "Currently unstable. Use `simple_encrypt` if possible."
)]
#[cfg(target_os = "linux")]
pub async fn encrypt_data(data: &[u8]) -> UnifiedResult<Vec<u8>> {
    if let Err(err) = initialize_locker().await {
        return UnifiedResult::new(Err(err));
    };

    let attempts: u8 = 10;
    let mut tries: u8 = 0;

    while tries <= attempts {
        if execution_locked().await {
            tries += 1;
            tokio::time::sleep(Duration::from_millis(700)).await;
            continue;
        }

        match encrypt_raw(unsafe { String::from_utf8_unchecked(data.to_vec()) })
            .await
            .uf_unwrap()
        {
            Ok((key, data, count)) => {
                call_clean().await;

                return UnifiedResult::new(Ok(format!("{}-{}-{}", data, key, count)
                    .as_bytes()
                    .to_vec()));
            }
            Err(e) => {
                log!(LogLevel::Error, "{}", e);
                call_clean().await;
                unimplemented!()
            }
        }
    }

    return UnifiedResult::new(Err(ErrorArrayItem::new(
        Errors::GeneralError,
        "Attempted too many times to access RECS; system busy".to_owned(),
    )));
}

/// Decrypts raw byte data using the legacy RECS-based decryption system. Expects
/// the data to contain key and count metadata (separated by '-').
///
/// # Deprecation
/// Marked as **deprecated** since version 4.3.0.
/// Please use [`simple_decrypt`] if possible.
///
/// # Arguments
/// - `data`: The byte slice to decrypt.  
///
/// # Returns
/// - `UnifiedResult<Vec<u8>>`: On success, returns a byte vector containing the decrypted data.
///   On failure, returns an `ErrorArrayItem` describing the error.
///
/// # Behavior
/// Repeatedly checks if the system is locked. If locked, it waits and retries.
/// Data must be in the format `[encrypted_data]-[key]-[count]`.
#[deprecated(
    since = "4.3.0",
    note = "Currently unstable. Use `simple_decrypt` if possible."
)]
#[cfg(target_os = "linux")]
pub async fn decrypt_data(data: &[u8]) -> UnifiedResult<Vec<u8>> {
    if let Err(err) = initialize_locker().await {
        return UnifiedResult::new(Err(err));
    };

    let attempts: u8 = 10;
    let mut tries: u8 = 0;

    while tries <= attempts {
        if execution_locked().await {
            tries += 1;
            tokio::time::sleep(Duration::from_millis(700)).await;
            continue;
        }

        let data_str = match std::str::from_utf8(data) {
            Ok(s) => s,
            Err(e) => {
                log!(LogLevel::Error, "Invalid UTF-8 sequence: {}", e);
                return UnifiedResult::new(Err(ErrorArrayItem::from(e)));
            }
        };

        let parts: Vec<&str> = data_str.split('-').collect();

        if parts.len() != 3 {
            log!(LogLevel::Error, "Invalid input data format");
            return UnifiedResult::new(Err(ErrorArrayItem::new(
                Errors::InvalidType,
                "Input data does not contain key, data, and count separated by '-'".to_string(),
            )));
        }

        let cleaned_parts: Vec<String> = parts.iter().map(|part| part.replace("-", "")).collect();

        let key = cleaned_parts[1].to_string();
        let encrypted_data = cleaned_parts[0].to_string();
        let count = match cleaned_parts[2].parse::<usize>() {
            Ok(c) => c,
            Err(e) => {
                log!(LogLevel::Error, "Invalid count value: {}", e);
                1
            }
        };

        match decrypt_raw(encrypted_data, key, count).uf_unwrap() {
            Ok(data) => return UnifiedResult::new(Ok(data)),
            Err(e) => return UnifiedResult::new(Err(e)),
        }
    }

    return UnifiedResult::new(Err(ErrorArrayItem::new(
        Errors::GeneralError,
        "Attempted too many times to access RECS; system busy".to_owned(),
    )));
}

/// Indicates whether the encryption/decryption process is currently locked
/// due to a housekeeping operation. Logs a warning if a lock is active.
///
/// # Returns
/// `true` if locked (housekeeping is in progress), otherwise `false`.
#[cfg(target_os = "linux")]
async fn execution_locked() -> bool {
    let lock = cleaning_lock.load(Ordering::Acquire);
    if lock {
        log!(LogLevel::Warn, "RECS locked for cleaning");
    }
    lock
}

/// Temporarily prevents the RECS cleaning operation from happening while
/// the provided `callback` is executed, to avoid clearing temporary data too soon.
///
/// # Safety
/// This function is marked as `unsafe` because it uses `unsafe` string
/// conversions internally. Only use it if you are certain the input data
/// can be safely converted to `String`. Also This function can lead to
/// unessacery filling of the /tmp dir if used too many times as the cleaning
/// loop looses its refrence to the tmp recs data called this way
///
/// # Deprecation
/// Marked as **deprecated** since version 4.3.0.  
/// Prefer using `simple_*` functions that do not rely on legacy RECS mechanics.
///
/// # Arguments
/// - `callback`: A function or closure that performs an encryption/decryption operation.
/// - `data`: The byte slice on which the operation acts.
///
/// # Returns
/// - `Ok(Vec<u8>)`: The operation’s successful output.
/// - `Err(ErrorArrayItem)`: An error if the operation or housekeeping fails.
#[cfg(target_os = "linux")]
#[deprecated(
    since = "4.3.0",
    note = "Currently unstable. Use `simple_*` if possible."
)]
pub async unsafe fn clean_override_op<'a, F, Fut>(
    callback: F,
    data: &'a [u8],
) -> Result<Vec<u8>, ErrorArrayItem>
where
    F: Fn(&'a [u8]) -> Fut,
    Fut: std::future::Future<Output = UnifiedResult<Vec<u8>>>,
{
    cleaning_loop_initialized.store(true, Ordering::Relaxed);
    let result: Vec<u8> = callback(&data).await.uf_unwrap()?;
    if let Err(err) = house_keeping().await {
        log!(LogLevel::Error, "HouseKeeping: {}", err);
    }
    Ok(result)
}

/// Triggers RECS cleanup, notifying the `clean_loop` to proceed.
#[cfg(target_os = "linux")]
async fn call_clean() {
    cleaning_call.notify_one();
    log!(LogLevel::Trace, "Recs clean called");
}

/// An asynchronous loop that waits for notifications to clean up RECS data.
/// Once triggered, it acquires a lock, performs housekeeping, and releases the lock.
#[cfg(target_os = "linux")]
async fn clean_loop() -> Result<(), ErrorArrayItem> {
    cleaning_loop_initialized.store(true, Ordering::Release);
    loop {
        tokio::select! {
            _ = cleaning_call.notified() => {
                cleaning_lock.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(300)).await;
                // * Anything less than 250 may start cleaning before operations have finished
                if let Err(err) = house_keeping().await {
                    log!(LogLevel::Error, "HouseKeeping: {}", err);
                }
                cleaning_lock.store(false, Ordering::SeqCst);
            }
        }
        sleep(Duration::from_secs(6)).await;
    }
}

/// Initializes the legacy RECS-based encryption system if it hasn't been
/// initialized yet. Also spawns the cleaning loop if not already running.
///
/// # Returns
/// - `Ok(())` on successful initialization.
/// - `Err(ErrorArrayItem)` if initialization fails.
#[cfg(target_os = "linux")]
async fn initialize_locker() -> Result<(), ErrorArrayItem> {
    match initialized.load(Ordering::Relaxed) {
        true => {
            if !cleaning_loop_initialized.load(Ordering::Relaxed) {
                tokio::spawn(clean_loop());
            }
            Ok(())
        }
        false => {
            initialize(true).await.uf_unwrap()?;
            sleep(Duration::from_nanos(100)).await;
            initialized.store(true, Ordering::Relaxed);
            tokio::spawn(clean_loop());
            cleaning_loop_initialized.store(true, Ordering::Relaxed);
            Ok(())
        }
    }
}

// endregion: Legacy Encryption/Decryption

// region: Modern Encryption/Decryption

/// The size (in bytes) of the GCM nonce. GCM requires a 96-bit (12-byte) nonce.
#[allow(unused_assignments)]
const NONCE_SIZE: usize = 12;

/// The size (in bytes) of the AES-256 key (256 bits → 32 bytes).
const KEY_SIZE: usize = 32;

pub fn generate_key(buffer: &mut [u8]) {
    let mut rng = rand::thread_rng(); // Create a random number generator
    for byte in buffer.iter_mut() {
        *byte = rng.gen(); // Fill each byte with random data
    }
}


/// Encrypts the provided data using AES-256 GCM encryption.
///
/// This modern approach is recommended over the legacy RECS-based system.
///
/// # Arguments
/// - `data`: Byte slice of the plaintext data to be encrypted.
///
/// # Returns
/// - `Ok(Stringy)`: A hex-encoded string containing the key, nonce, and ciphertext.
/// - `Err(ErrorArrayItem)`: An error if encryption fails.
pub fn simple_encrypt(data: &[u8]) -> Result<Stringy, ErrorArrayItem> {
    // Generate a random key and nonce
    let mut key: [u8; 32] = [0u8; 32];
    generate_key(&mut key);
    let cipher = Aes256Gcm::new(&key.into());
    let nonce_bytes = rand::thread_rng().gen::<[u8; NONCE_SIZE]>();
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the data
    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| ErrorArrayItem::new(Errors::InvalidBlockData, e.to_string()))?;

    // Combine the key, nonce, and ciphertext into a single byte stream
    let mut result = Vec::with_capacity(KEY_SIZE + NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(&key);
    result.extend_from_slice(nonce);
    result.extend_from_slice(&ciphertext);

    let cipher_text = Stringy::from(hex::encode(result));

    Ok(cipher_text)
}

/// Decrypts the provided data using AES-256 GCM decryption.
///
/// # Arguments
/// - `encrypted_cipher_data`: A hex-encoded string containing the key, nonce, and ciphertext.
///
/// # Returns
/// - `Ok(Vec<u8>)`: The decrypted plaintext data.
/// - `Err(ErrorArrayItem)`: An error if decryption fails or if data is malformed (too short).
pub fn simple_decrypt(encrypted_cipher_data: &[u8]) -> Result<Vec<u8>, ErrorArrayItem> {
    let encrypted_data: Vec<u8> =
        hex::decode(encrypted_cipher_data).map_err(ErrorArrayItem::from)?;

    // Extract the key, nonce, and ciphertext
    if encrypted_data.len() <= KEY_SIZE + NONCE_SIZE {
        return Err(ErrorArrayItem::new(
            Errors::InvalidBlockData,
            "Encrypted data is too short",
        ));
    }

    let key = Key::<Aes256Gcm>::from_slice(&encrypted_data[..KEY_SIZE]);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&encrypted_data[KEY_SIZE..KEY_SIZE + NONCE_SIZE]);
    let ciphertext = &encrypted_data[KEY_SIZE + NONCE_SIZE..];

    // Decrypt the data
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|err| ErrorArrayItem::new(Errors::InvalidBlockData, err.to_string()))
}
// endregion: Modern Encryption/Decryption
