use crate::error::{AppError, AppResult};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;

pub fn generate_file_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

pub fn encrypt_file(plaintext: &[u8], key: &[u8; 32]) -> AppResult<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| AppError::msg(e.to_string()))?;
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext)
        .map_err(|e| AppError::msg(format!("encrypt failed: {}", e)))?;
    Ok((ciphertext, iv))
}

pub fn decrypt_file(ciphertext: &[u8], key: &[u8; 32], iv: &[u8]) -> AppResult<Vec<u8>> {
    if iv.len() != 12 {
        return Err(AppError::msg("invalid IV length"));
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| AppError::msg(e.to_string()))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(iv), ciphertext)
        .map_err(|e| AppError::msg(format!("decrypt failed: {}", e)))?;
    Ok(plaintext)
}

pub fn key_to_b64url(key: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(key)
}

pub fn key_from_b64url(s: &str) -> AppResult<[u8; 32]> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| AppError::msg(format!("invalid key encoding: {}", e)))?;
    if bytes.len() != 32 {
        return Err(AppError::msg("invalid key length"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

pub fn iv_to_base64(iv: &[u8; 12]) -> String {
    use base64::engine::general_purpose::STANDARD;
    STANDARD.encode(iv)
}

pub fn iv_from_base64(s: &str) -> AppResult<[u8; 12]> {
    use base64::engine::general_purpose::STANDARD;
    let bytes = STANDARD
        .decode(s.trim())
        .map_err(|e| AppError::msg(format!("invalid IV encoding: {}", e)))?;
    if bytes.len() != 12 {
        return Err(AppError::msg("invalid IV length"));
    }
    let mut iv = [0u8; 12];
    iv.copy_from_slice(&bytes);
    Ok(iv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = generate_file_key();
        let plaintext = b"hello freedrive desktop sync";
        let (ciphertext, iv) = encrypt_file(plaintext, &key).unwrap();
        assert_ne!(ciphertext, plaintext);
        let decrypted = decrypt_file(&ciphertext, &key, &iv).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn key_b64url_roundtrip() {
        let key = generate_file_key();
        let encoded = key_to_b64url(&key);
        let decoded = key_from_b64url(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn iv_base64_roundtrip() {
        let mut iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut iv);
        let encoded = iv_to_base64(&iv);
        let decoded = iv_from_base64(&encoded).unwrap();
        assert_eq!(decoded, iv);
    }
}
