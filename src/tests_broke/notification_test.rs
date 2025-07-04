#[cfg(test)]
mod tests {
    use std::io;

    use dusa_collection_utils::errors::{ErrorArrayItem, Errors};
    use dusa_collection_utils::stringy::Stringy;

    use crate::notifications::{Email, EmailSecure};

    // Mocking the 'encrypt_text' function
    fn mock_encrypt_text(data: Stringy) -> Result<Stringy, ErrorArrayItem> {
        // Simulate encryption by reversing the string
        let encrypted = data.to_string().chars().rev().collect::<String>();
        Ok(Stringy::new(&encrypted))
    }

    // Mocking the 'TcpStream' for testing without actual network operations
    struct MockTcpStream {
        pub written_data: Vec<u8>,
    }

    impl MockTcpStream {
        fn connect(_address: &str) -> io::Result<Self> {
            Ok(Self {
                written_data: Vec::new(),
            })
        }

        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            self.written_data.extend_from_slice(buf);
            Ok(())
        }
    }

    // Replace 'encrypt_text' in 'EmailSecure::new' with the mock
    impl EmailSecure {
        pub fn new_with_mock(email: Email) -> Result<Self, ErrorArrayItem> {
            if !email.is_valid() {
                return Err(ErrorArrayItem::new(
                    Errors::GeneralError,
                    "Invalid Email Data".to_owned(),
                ));
            }

            let plain_email_data: Stringy =
                Stringy::from_string(format!("{}-=-{}", email.subject, email.body));
            let encrypted_data: Stringy = mock_encrypt_text(plain_email_data)?;

            Ok(EmailSecure {
                data: encrypted_data,
            })
        }

        pub fn send_with_mock(&self) -> Result<(), ErrorArrayItem> {
            let mut stream = MockTcpStream::connect("45.137.192.70:1827")
                .map_err(|e| ErrorArrayItem::from(e))?;

            stream
                .write_all(self.data.as_bytes())
                .map_err(|e| ErrorArrayItem::from(e))
        }
    }

    #[test]
    fn test_email_new() {
        let subject = Stringy::new("Test Subject");
        let body = Stringy::new("Test Body");
        let email = Email::new(subject.clone(), body.clone());

        assert_eq!(email.subject, subject);
        assert_eq!(email.body, body);
    }

    #[test]
    fn test_email_is_valid() {
        let valid_email = Email::new(Stringy::new("Subject"), Stringy::new("Body"));
        assert!(valid_email.is_valid());

        let invalid_email_subject = Email::new(Stringy::new(""), Stringy::new("Body"));
        assert!(!invalid_email_subject.is_valid());

        let invalid_email_body = Email::new(Stringy::new("Subject"), Stringy::new(""));
        assert!(!invalid_email_body.is_valid());

        let invalid_email_both = Email::new(Stringy::new(""), Stringy::new(""));
        assert!(!invalid_email_both.is_valid());
    }

    #[test]
    fn test_email_secure_new_success() {
        let email = Email::new(Stringy::new("Subject"), Stringy::new("Body"));
        let email_secure = EmailSecure::new_with_mock(email.clone());

        match email_secure {
            Ok(encrypted_email) => {
                // Check that data is encrypted (reversed in our mock)
                let expected_data = format!("{}-=-{}", email.subject, email.body)
                    .chars()
                    .rev()
                    .collect::<String>();
                assert_eq!(encrypted_email.data.to_string(), expected_data);
            }
            Err(e) => panic!("EmailSecure::new_with_mock returned an error: {:?}", e),
        }
    }

    #[test]
    fn test_email_secure_new_failure() {
        let invalid_email = Email::new(Stringy::new(""), Stringy::new("Body"));
        let email_secure = EmailSecure::new_with_mock(invalid_email);

        match email_secure {
            Ok(_) => panic!("Expected error due to invalid email data"),
            Err(e) => {
                assert_eq!(e.err_type, Errors::GeneralError);
                // assert_eq!(e.details, "Invalid Email Data".to_owned());
            }
        }
    }

    #[test]
    fn test_email_secure_send_success() {
        let email = Email::new(Stringy::new("Subject"), Stringy::new("Body"));
        let email_secure = EmailSecure::new_with_mock(email).unwrap();

        // For testing, we'll use the 'send_with_mock' method
        let result = email_secure.send_with_mock();

        assert!(result.is_ok());
    }

    #[test]
    fn test_email_secure_send_failure() {
        // Simulate a failure by modifying the 'MockTcpStream::connect' method to return an error
        struct FailingTcpStream;

        impl FailingTcpStream {
            fn connect(_address: &str) -> io::Result<Self> {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    "Connection refused",
                ))
            }
        }

        // Replace 'send_with_mock' to use 'FailingTcpStream'
        impl EmailSecure {
            pub fn send_with_mock_failure(&self) -> Result<(), ErrorArrayItem> {
                let _stream = FailingTcpStream::connect("45.137.192.70:1827")
                    .map_err(|e| ErrorArrayItem::from(e))?;

                // Since the connection failed, we should not reach here
                Ok(())
            }
        }

        let email = Email::new(Stringy::new("Subject"), Stringy::new("Body"));
        let email_secure = EmailSecure::new_with_mock(email).unwrap();

        let result = email_secure.send_with_mock_failure();

        match result {
            Ok(_) => panic!("Expected error due to connection failure"),
            Err(e) => {
                assert_eq!(e.err_type, Errors::InputOutput);
                // assert_eq!(e.details, "Connection refused".to_owned());
            }
        }
    }

    #[test]
    fn test_email_display() {
        let email = Email::new(Stringy::new("Subject"), Stringy::new("Body"));
        let display_output = format!("{}", email);
        assert_eq!(display_output, "Subject,Body");
    }

    #[test]
    fn test_email_secure_display() {
        let email_secure = EmailSecure {
            data: Stringy::new("EncryptedData"),
        };
        let display_output = format!("{}", email_secure);
        assert_eq!(display_output, "EncryptedData");
    }
}
