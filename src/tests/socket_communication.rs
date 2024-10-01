#[cfg(test)]
mod tests {
    use crate::socket_communication::{
        get_socket_stream, receive_message, send_acknowledge, send_message, set_socket_ownership,
        GeneralMessage, MessageType,
    };

    use dusa_collection_utils::errors::Errors;
    use dusa_collection_utils::stringy::Stringy;
    use dusa_collection_utils::types::PathType;

    use nix::unistd::{Gid, Uid};
    use serde_json::json;
    use std::fs;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use tempfile::{tempdir, TempDir};
    use tokio::net::UnixStream;

    async fn setup_mock_unix_stream() -> (UnixStream, UnixStream) {
        UnixStream::pair().expect("Failed to create UnixStream pair")
    }

    async fn setup_mock_unix_socket(path: &Path) -> UnixListener {
        UnixListener::bind(path).expect("Failed to create mock Unix socket")
    }

    #[tokio::test]
    async fn test_send_message() {
        let (mut stream, mut mock_stream) = setup_mock_unix_stream().await;

        let message = GeneralMessage {
            version: Stringy::from_string(env!("CARGO_PKG_VERSION").to_string()),
            msg_type: MessageType::StatusUpdate,
            payload: json!({"test_key": "test_value"}),
            error: None,
        };

        let send_task = send_message(&mut stream, &message);
        let receive_task = async {
            let mut length_bytes = [0u8; 4];
            mock_stream
                .read_exact(&mut length_bytes)
                .await
                .expect("Failed to read length");
            let length = u32::from_be_bytes(length_bytes) as usize;

            let mut message_bytes = vec![0u8; length];
            mock_stream
                .read_exact(&mut message_bytes)
                .await
                .expect("Failed to read message");

            let received_message: GeneralMessage =
                serde_json::from_slice(&message_bytes).expect("Failed to deserialize message");

            assert_eq!(received_message.version, message.version);
            assert_eq!(received_message.msg_type, message.msg_type);
            assert_eq!(received_message.payload, message.payload);
        };

        let _ = tokio::join!(send_task, receive_task);
    }

    #[tokio::test]
    async fn test_receive_message() {
        let (mut mock_stream, mut stream) = setup_mock_unix_stream().await;

        let message = GeneralMessage {
            version: Stringy::from_string(env!("CARGO_PKG_VERSION").to_string()),
            msg_type: MessageType::StatusUpdate,
            payload: json!({"test_key": "test_value"}),
            error: None,
        };

        let send_task = async {
            let message_bytes = serde_json::to_vec(&message).expect("Failed to serialize message");
            let length_bytes = (message_bytes.len() as u32).to_be_bytes();

            mock_stream
                .write_all(&length_bytes)
                .await
                .expect("Failed to write length");
            mock_stream
                .write_all(&message_bytes)
                .await
                .expect("Failed to write message");
        };

        let receive_task = async {
            let received_message = receive_message(&mut stream)
                .await
                .expect("Failed to receive message");

            assert_eq!(received_message.version, message.version);
            assert_eq!(received_message.msg_type, message.msg_type);
            assert_eq!(received_message.payload, message.payload);
        };

        tokio::join!(send_task, receive_task);
    }

    #[tokio::test]
    async fn test_send_acknowledge() {
        let (mut stream, mut mock_stream) = setup_mock_unix_stream().await;

        let send_ack_task = send_acknowledge(&mut stream);

        let receive_task = async {
            let mut length_bytes = [0u8; 4];
            mock_stream
                .read_exact(&mut length_bytes)
                .await
                .expect("Failed to read length");
            let length = u32::from_be_bytes(length_bytes) as usize;

            let mut message_bytes = vec![0u8; length];
            mock_stream
                .read_exact(&mut message_bytes)
                .await
                .expect("Failed to read message");

            let received_message: GeneralMessage =
                serde_json::from_slice(&message_bytes).expect("Failed to deserialize message");

            assert_eq!(received_message.msg_type, MessageType::Acknowledgment);
            assert_eq!(received_message.payload, json!({"message_received": true}));
        };

        tokio::join!(send_ack_task, receive_task);
    }

    #[tokio::test]
    async fn test_get_socket_stream_valid_path() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let socket_path = temp_dir.path().join("test.sock");

        // Create a UnixListener for the socket to make it a valid socket
        let _listener = setup_mock_unix_socket(&socket_path).await;

        // Use PathType::PathBuf to test the function
        let path = PathType::PathBuf(socket_path.to_path_buf());

        // Call the function with a valid socket path
        let result = get_socket_stream(&path).await;

        assert!(
            result.is_ok(),
            "Expected to successfully connect to the socket"
        );
    }

    #[tokio::test]
    async fn test_get_socket_stream_invalid_path() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let invalid_socket_path = temp_dir.path().join("nonexistent.sock");

        // Use PathType::PathBuf with an invalid path
        let path = PathType::PathBuf(invalid_socket_path);

        // Call the function with an invalid socket path
        let result = get_socket_stream(&path).await;

        // Expecting an error
        assert!(
            result.is_err(),
            "Expected an error due to nonexistent socket file"
        );

        // Check the type of error
        if let Err(error) = result {
            assert_eq!(
                error.err_type,
                Errors::InvalidFile,
                "Expected InvalidFile error type"
            );
        }
    }

    #[test]
    fn test_set_socket_ownership() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let path = temp_dir.path().join("test.sock");
        fs::File::create(&path).expect("Failed to create socket file");

        let uid = Uid::current();
        let gid = Gid::current();

        let result = set_socket_ownership(&path, uid, gid);
        assert!(
            result.is_ok(),
            "Expected to set socket ownership successfully"
        );
    }

    // This would have to run as root
    // #[test]
    // fn test_set_socket_permission() {
    //     let temp_dir = TempDir::new().expect("Failed to create temp directory");
    //     let path = temp_dir.path().join("test.sock");
    //     fs::File::create(&path).expect("Failed to create socket file");

    //     let metadata = fs::metadata(path).expect("Failed to get file metadata");
    //     assert_eq!(metadata.permissions().mode() & 0o777, 0o660, "Expected permissions to be 660");
    // }
}
