use dusa_collection_utils::log;
use tokio::sync::Notify;
use {
    dusa_collection_utils::{
        errors::{ErrorArrayItem, Errors, UnifiedResult},
        log::LogLevel,
        stringy::Stringy,
    },
    recs::{decrypt_raw, encrypt_raw, house_keeping, initialize},
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::time::sleep,
};

lazy_static::lazy_static! {
    static ref initialized:  Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    static ref cleaning_loop_initialized: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    static ref cleaning_call: Arc<Notify> = Arc::new(Notify::new());
    static ref cleaning_lock: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

pub async fn encrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
    let data_bytes = data.as_bytes().to_vec();
    let plain_bytes = encrypt_data(&data_bytes).await.uf_unwrap()?;

    let text = Stringy::from(String::from_utf8(plain_bytes)?);
    Ok(text)
}

pub async fn decrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
    let data_bytes: &[u8] = data.as_bytes();
    let decrypted_bytes: Vec<u8> = decrypt_data(&data_bytes).await.uf_unwrap()?;
    let decrypted_string: String = String::from_utf8(decrypted_bytes)?;
    let decrypted_stringy: Stringy = Stringy::Immutable(Arc::<str>::from(decrypted_string));

    Ok(decrypted_stringy)
}

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
        "Attempted to many times to access recs, system busy".to_owned(),
    )));
}

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

        let parts: Vec<&str> = data_str.splitn(3, '-').collect();
        if parts.len() != 3 {
            log!(LogLevel::Error, "Invalid input data format");
            return UnifiedResult::new(Err(ErrorArrayItem::new(
                Errors::InvalidType,
                "Input data does not contain key, data, and count separated by '-'".to_string(),
            )));
        }
        
        let cleaned_parts: Vec<String> = parts.iter()
            .map(|part| part.replace("-", "")) 
            .collect();
        
        let key = cleaned_parts[1].to_string();
        let encrypted_data = cleaned_parts[0].to_string();
        let count = match cleaned_parts[2].parse::<usize>() {
            Ok(c) => c,
            Err(_e) => {
                // log!(LogLevel::Error, "Invalid count value: {}", e);
                1
                // return UnifiedResult::new(Err(ErrorArrayItem::from(e)));
            }
        };

        match decrypt_raw(encrypted_data, key, count).uf_unwrap() {
            Ok(data) => return UnifiedResult::new(Ok(data)),
            Err(e) => return UnifiedResult::new(Err(e)),
        }
    }

    return UnifiedResult::new(Err(ErrorArrayItem::new(
        Errors::GeneralError,
        "Attempted to many times to access recs, system busy".to_owned(),
    )));
}

async fn execution_locked() -> bool {
    let lock = cleaning_lock.load(Ordering::Acquire);
    if lock {
        log!(LogLevel::Warn, "RECS locked for cleaning");
    }
    lock
}

async fn call_clean() {
    cleaning_call.notify_one();
    log!(LogLevel::Trace, "Recs clean called");
}

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

async fn initialize_locker() -> Result<(), ErrorArrayItem> {
    match initialized.load(Ordering::Relaxed) {
        true => {
            if !cleaning_loop_initialized.load(Ordering::Relaxed) {
                tokio::spawn(clean_loop());
            }
            return Ok(());
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

// The worst case for these timings is a error after ~6.8 seconds
// let attempts: u8 = 10;
// let mut tries: u8 = 0;
//
// while tries <= attempts {
//     if execution_locked().await {
//         tries += 1;
//         tokio::time::sleep(Duration::from_millis(700)).await;
//         continue;
//     }
//
//     code()
//
// }
