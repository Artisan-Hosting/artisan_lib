#[cfg(test)]
mod tests {
    use crate::encryption::{simple_decrypt, simple_encrypt};

    #[test]
    fn test_encrypt_decrypt_cycle() {
        let data = b"hello world";
        let cipher = simple_encrypt(data).expect("encrypt");
        let plain = simple_decrypt(cipher.as_bytes()).expect("decrypt");
        assert_eq!(plain, data);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        let a = simple_encrypt(b"data").expect("encrypt");
        let b = simple_encrypt(b"data").expect("encrypt");
        assert_ne!(a, b, "encryption should be nondeterministic");
    }
}
