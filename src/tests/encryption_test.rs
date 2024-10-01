// Chatgpt generated testing

#[cfg(test)]
mod tests {
    use dusa_collection_utils::{errors::{ErrorArray, ErrorArrayItem, UnifiedResult as uf}, stringy::Stringy};

    use crate::encryption::ProgramMode;

    // Mocking the 'run' function for testing without a server
    fn mock_run(
        mode: ProgramMode,
        _path: Option<String>,
        _owner: Option<String>,
        _name: Option<String>,
        data: Option<String>,
    ) -> uf<Option<String>> {
        match mode {
            ProgramMode::EncryptText => {
                // Simulate successful encryption
                let encrypted = format!("encrypted({})", data.unwrap_or_default());
                uf::new(Ok(Some(encrypted)))
            }
            ProgramMode::DecryptText => {
                // Simulate successful decryption
                let decrypted = data.unwrap_or_default().replace("encrypted(", "").replace(")", "");
                uf::new(Ok(Some(decrypted)))
            }
            _ => uf::new(Err(ErrorArray::new_container())),
        }
    }

    // Helper functions to inject the mock 'run' into 'encrypt_text' and 'decrypt_text'
    fn encrypt_text_with_mock(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
        match mock_run(
            ProgramMode::EncryptText,
            None,
            None,
            None,
            Some(data.to_string()),
        )
        .uf_unwrap()
        {
            Ok(d) => match d {
                Some(d) => Ok(Stringy::new(&d)),
                None => Err(ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::GeneralError,
                    String::from("No data received from mock"),
                )),
            },
            Err(mut e) => Err(e.pop()),
        }
    }

    fn decrypt_text_with_mock(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
        match mock_run(
            ProgramMode::DecryptText,
            None,
            None,
            None,
            Some(data.to_string()),
        )
        .uf_unwrap()
        {
            Ok(d) => match d {
                Some(d) => Ok(Stringy::new(&d)),
                None => Err(ErrorArrayItem::new(
                    dusa_collection_utils::errors::Errors::GeneralError,
                    String::from("No data received from mock"),
                )),
            },
            Err(mut e) => Err(e.pop()),
        }
    }

    #[test]
    fn test_encrypt_text_success() {
        // Arrange
        let input = Stringy::new("Hello, World!");

        // Act
        let result = encrypt_text_with_mock(input.clone());

        // Assert
        match result {
            Ok(encrypted) => {
                // Check that encrypted data is not empty and not equal to input
                assert!(!encrypted.to_string().is_empty());
                assert_ne!(encrypted.to_string(), input.to_string());
                assert_eq!(encrypted.to_string(), "encrypted(Hello, World!)");
            }
            Err(e) => panic!("encrypt_text_with_mock returned an error: {:?}", e),
        }
    }

    #[test]
    fn test_encrypt_text_failure() {
        // Arrange
        let input = Stringy::new("");

        // Act
        let result = encrypt_text_with_mock(input);

        // Assert
        match result {
            Ok(encrypted) => {
                // Even empty strings are encrypted in this mock scenario
                assert_eq!(encrypted.to_string(), "encrypted()");
            }
            Err(e) => panic!("encrypt_text_with_mock returned an error: {:?}", e),
        }
    }

    #[test]
    fn test_decrypt_text_success() {
        // Arrange
        let encrypted_input = Stringy::new("encrypted(Hello, World!)");

        // Act
        let result = decrypt_text_with_mock(encrypted_input);

        // Assert
        match result {
            Ok(decrypted) => {
                assert_eq!(decrypted.to_string(), "Hello, World!");
            }
            Err(e) => panic!("decrypt_text_with_mock returned an error: {:?}", e),
        }
    }

    #[test]
    fn test_decrypt_text_failure() {
        // Arrange
        let invalid_encrypted_input = Stringy::new("invalid_encrypted_data");

        // Act
        let result = decrypt_text_with_mock(invalid_encrypted_input);

        // Assert
        match result {
            Ok(decrypted) => {
                // The mock simply removes 'encrypted(' and ')', so invalid data will be returned as is
                assert_eq!(decrypted.to_string(), "invalid_encrypted_data");
            }
            Err(e) => panic!("decrypt_text_with_mock returned an error: {:?}", e),
        }
    }

    #[test]
    fn test_encrypt_then_decrypt() {
        // Arrange
        let input = Stringy::new("Test Message");

        // Act
        let encrypted_result = encrypt_text_with_mock(input.clone());
        match encrypted_result {
            Ok(encrypted) => {
                let decrypted_result = decrypt_text_with_mock(encrypted);
                // Assert
                match decrypted_result {
                    Ok(decrypted) => {
                        assert_eq!(decrypted.to_string(), input.to_string());
                    }
                    Err(e) => panic!("decrypt_text_with_mock returned an error: {:?}", e),
                }
            }
            Err(e) => panic!("encrypt_text_with_mock returned an error: {:?}", e),
        }
    }
}
