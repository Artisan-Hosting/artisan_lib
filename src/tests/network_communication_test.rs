const _MAJOR_VERSION: &str = env!("CARGO_PKG_VERSION_MAJOR");
const _MINOR_VERSION: &str = env!("CARGO_PKG_VERSION_MINOR");

#[cfg(test)]
mod tests {
    use crate::network_communication::{read_message, send_message};

    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::io::{Write, Read};

    #[test]
    fn test_send_message() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // Bind to an available port
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();

            // Read the length prefix (4 bytes)
            let mut length_buf = [0u8; 4];
            socket.read_exact(&mut length_buf).unwrap();
            let length = u32::from_be_bytes(length_buf);

            // Read the version fields (2 bytes)
            let mut version_buf = [0u8; 2];
            socket.read_exact(&mut version_buf).unwrap();
            let major_version = version_buf[0];
            let minor_version = version_buf[1];

            // Read the payload
            let payload_length = (length - 2) as usize;
            let mut payload = vec![0u8; payload_length];
            socket.read_exact(&mut payload).unwrap();

            // Assertions
            assert_eq!(length, 6); // 2 version bytes + 4 bytes payload
            assert_eq!(major_version, _MAJOR_VERSION.parse::<u8>().unwrap());
            assert_eq!(minor_version, _MINOR_VERSION.parse::<u8>().unwrap());
            assert_eq!(payload, b"test");
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        send_message(&mut stream, b"test").unwrap();

        handle.join().unwrap();
    }

    #[test]
    fn test_read_message() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // Bind to an available port
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();

            // Prepare a message with the length prefix, version, and payload
            let major_version = _MAJOR_VERSION.parse::<u8>().unwrap();
            let minor_version = _MINOR_VERSION.parse::<u8>().unwrap();
            let payload = b"test";

            // Length is 2 bytes of version + payload length
            let length = 2 + payload.len() as u32;

            // Write the message to the stream
            stream.write_all(&length.to_be_bytes()).unwrap();
            stream.write_all(&[major_version, minor_version]).unwrap();
            stream.write_all(payload).unwrap();
        });

        let (mut socket, _) = listener.accept().unwrap();
        let result = read_message(&mut socket);

        // Verify the result
        match result {
            Ok((major_version, minor_version, payload)) => {
                assert_eq!(major_version, _MAJOR_VERSION.parse::<u8>().unwrap());
                assert_eq!(minor_version, _MINOR_VERSION.parse::<u8>().unwrap());
                assert_eq!(payload, b"test");
            }
            Err(_) => panic!("Failed to read message"),
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_read_message_unsupported_version() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // Bind to an available port
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();

            // Prepare a message with the wrong major version, length prefix, and payload
            let wrong_major_version = _MAJOR_VERSION.parse::<u8>().unwrap() + 1; // Deliberately wrong version
            let minor_version = _MINOR_VERSION.parse::<u8>().unwrap();
            let payload = b"test";

            // Length is 2 bytes of version + payload length
            let length = 2 + payload.len() as u32;

            // Write the message to the stream
            stream.write_all(&length.to_be_bytes()).unwrap();
            stream.write_all(&[wrong_major_version, minor_version]).unwrap();
            stream.write_all(payload).unwrap();
        });

        let (mut socket, _) = listener.accept().unwrap();
        let result = read_message(&mut socket);

        // Verify that the function returns an error due to unsupported version
        assert!(result.is_err());

        handle.join().unwrap();
    }
}
