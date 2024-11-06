use {
    dusa_collection_utils::{
        errors::{ErrorArrayItem, Errors, UnifiedResult},
        log,
        log::LogLevel,
        stringy::Stringy,
    },
    recs::{decrypt_raw, encrypt_raw, initialize},
    std::sync::Arc,
};

pub trait Encryption {
    fn encrypt_text(&self, data: Stringy) -> Result<Stringy, ErrorArrayItem>;
    fn decrypt_text(&self, data: Stringy) -> Result<Stringy, ErrorArrayItem>;
}

pub async fn encrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
    let data_bytes = data.as_bytes().to_vec();
    let plain_bytes = encrypt_data(&data_bytes).await.uf_unwrap()?;

    let text = Stringy::from_string(String::from_utf8(plain_bytes)?);
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
    initialize(true).await;
    match encrypt_raw(unsafe { String::from_utf8_unchecked(data.to_vec()) })
        .await
        .uf_unwrap()
    {
        Ok((key, data, count)) => UnifiedResult::new(Ok(format!("{}-{}-{}", key, data, count)
            .as_bytes()
            .to_vec())),
        Err(e) => {
            log!(LogLevel::Error, "{}", e);

            unimplemented!()
        }
    }
}

pub async fn decrypt_data(data: &[u8]) -> UnifiedResult<Vec<u8>> {
    initialize(true).await;

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

    let key = parts[0].to_string();
    let encrypted_data = parts[1].to_string();
    let count = match parts[2].parse::<usize>() {
        Ok(c) => c,
        Err(e) => {
            log!(LogLevel::Error, "Invalid count value: {}", e);
            return UnifiedResult::new(Err(ErrorArrayItem::from(e)));
        }
    };

    match decrypt_raw(encrypted_data, key, count).uf_unwrap() {
        Ok(data) => UnifiedResult::new(Ok(data)),
        Err(e) => UnifiedResult::new(Err(e)),
    }
}
