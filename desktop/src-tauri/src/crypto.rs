use crate::error::{AppError, AppResult};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use aes_gcm_stream::Aes256GcmStreamEncryptor;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Buffer size for streaming encrypt / hash (keeps RAM bounded).
const STREAM_BUF: usize = 1024 * 1024;

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

/// Stream-encrypt a file to `out_path` (ciphertext ‖ 16-byte tag), same wire format as
/// [`encrypt_file`] / WebCrypto / mobile. Also computes SHA-256 of plaintext.
///
/// Peak RAM ≈ `STREAM_BUF` + cipher state — not 2× file size.
pub fn encrypt_file_streaming(
    plaintext_path: &Path,
    key: &[u8; 32],
    out_path: &Path,
) -> AppResult<StreamEncryptResult> {
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);

    let mut input = File::open(plaintext_path)?;
    let mut output = File::create(out_path)?;
    let mut encryptor = Aes256GcmStreamEncryptor::new(*key, &iv);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUF];
    let mut original_size: u64 = 0;

    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        original_size += n as u64;
        hasher.update(&buf[..n]);
        let encrypted = encryptor.update(&buf[..n]);
        if !encrypted.is_empty() {
            output.write_all(&encrypted)?;
        }
    }

    let (last_block, tag) = encryptor.finalize();
    if !last_block.is_empty() {
        output.write_all(&last_block)?;
    }
    output.write_all(&tag)?;
    output.flush()?;

    let encrypted_size = std::fs::metadata(out_path)?.len();
    Ok(StreamEncryptResult {
        iv,
        content_hash: hex::encode(hasher.finalize()),
        original_size,
        encrypted_size,
    })
}

#[derive(Debug, Clone)]
pub struct StreamEncryptResult {
    pub iv: [u8; 12],
    pub content_hash: String,
    pub original_size: u64,
    pub encrypted_size: u64,
}

/// SHA-256 of file contents without loading the whole file into RAM.
pub fn content_hash_file(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; STREAM_BUF];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
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

pub const PBKDF2_ITERATIONS: u32 = 310_000;

pub fn derive_kek(password: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    use pbkdf2::pbkdf2_hmac_array;
    use sha2::Sha256;
    Ok(pbkdf2_hmac_array::<Sha256, 32>(
        password.as_bytes(),
        salt,
        PBKDF2_ITERATIONS,
    ))
}

pub fn wrap_bytes(plaintext: &[u8], key: &[u8; 32]) -> AppResult<String> {
    let (ciphertext, iv) = encrypt_file(plaintext, key)?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&iv);
    combined.extend_from_slice(&ciphertext);
    Ok(URL_SAFE_NO_PAD.encode(combined))
}

pub fn unwrap_bytes(wrapped_b64: &str, key: &[u8; 32]) -> AppResult<Vec<u8>> {
    let combined = URL_SAFE_NO_PAD
        .decode(wrapped_b64)
        .map_err(|e| AppError::msg(format!("invalid wrapped key: {}", e)))?;
    if combined.len() < 13 {
        return Err(AppError::msg("invalid wrapped key length"));
    }
    let mut iv = [0u8; 12];
    iv.copy_from_slice(&combined[..12]);
    decrypt_file(&combined[12..], key, &iv)
}

pub fn format_recovery_code(raw: &[u8; 32]) -> String {
    raw.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
        .as_bytes()
        .chunks(8)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("-")
}

pub fn parse_recovery_code(code: &str) -> AppResult<[u8; 32]> {
    let hex: String = code.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 64 {
        return Err(AppError::msg("invalid recovery code format"));
    }
    let bytes = hex::decode(hex).map_err(|e| AppError::msg(e.to_string()))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
    fn stream_encrypt_decrypt_roundtrip_small() {
        let dir = std::env::temp_dir();
        let plain_path = dir.join(format!("fd-stream-plain-{}.bin", std::process::id()));
        let enc_path = dir.join(format!("fd-stream-enc-{}.bin", std::process::id()));
        let plaintext = b"stream encrypt small payload for freedrive";
        std::fs::write(&plain_path, plaintext).unwrap();

        let key = generate_file_key();
        let result = encrypt_file_streaming(&plain_path, &key, &enc_path).unwrap();
        assert_eq!(result.original_size, plaintext.len() as u64);
        assert_eq!(result.encrypted_size, plaintext.len() as u64 + 16);
        assert_eq!(
            result.content_hash,
            hex::encode(Sha256::digest(plaintext))
        );

        let ciphertext = std::fs::read(&enc_path).unwrap();
        let decrypted = decrypt_file(&ciphertext, &key, &result.iv).unwrap();
        assert_eq!(decrypted, plaintext);

        let _ = std::fs::remove_file(&plain_path);
        let _ = std::fs::remove_file(&enc_path);
    }

    #[test]
    fn stream_encrypt_decrypt_roundtrip_over_1mib() {
        let dir = std::env::temp_dir();
        let plain_path = dir.join(format!("fd-stream-plain-big-{}.bin", std::process::id()));
        let enc_path = dir.join(format!("fd-stream-enc-big-{}.bin", std::process::id()));

        // > 1 MiB so encrypt uses multiple update() blocks.
        let mut plaintext = vec![0u8; STREAM_BUF + 12345];
        rand::thread_rng().fill_bytes(&mut plaintext);
        {
            let mut f = File::create(&plain_path).unwrap();
            f.write_all(&plaintext).unwrap();
        }

        let key = generate_file_key();
        let result = encrypt_file_streaming(&plain_path, &key, &enc_path).unwrap();
        assert_eq!(result.original_size, plaintext.len() as u64);
        assert_eq!(result.encrypted_size, (plaintext.len() + 16) as u64);

        let ciphertext = std::fs::read(&enc_path).unwrap();
        let decrypted = decrypt_file(&ciphertext, &key, &result.iv).unwrap();
        assert_eq!(decrypted, plaintext);

        let _ = std::fs::remove_file(&plain_path);
        let _ = std::fs::remove_file(&enc_path);
    }

    #[test]
    fn stream_matches_oneshot_encrypt() {
        use aes_gcm_stream::Aes256GcmStreamEncryptor;

        let key = generate_file_key();
        let mut iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut iv);
        let plaintext = b"compare stream vs oneshot aes-gcm output";

        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let oneshot = cipher
            .encrypt(Nonce::from_slice(&iv), plaintext.as_ref())
            .unwrap();

        let mut enc = Aes256GcmStreamEncryptor::new(key, &iv);
        let mut streamed = enc.update(plaintext);
        let (last, tag) = enc.finalize();
        streamed.extend_from_slice(&last);
        streamed.extend_from_slice(&tag);

        assert_eq!(streamed, oneshot);
    }

    #[test]
    fn content_hash_file_matches_bytes() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("fd-hash-{}.bin", std::process::id()));
        let data = b"hash me without full buffer API";
        std::fs::write(&path, data).unwrap();
        let from_file = content_hash_file(&path).unwrap();
        let from_bytes = hex::encode(Sha256::digest(data));
        assert_eq!(from_file, from_bytes);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn key_b64url_roundtrip() {
        let key = generate_file_key();
        let encoded = key_to_b64url(&key);
        let decoded = key_from_b64url(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let key = generate_file_key();
        let wrapping = generate_file_key();
        let wrapped = wrap_bytes(&key, &wrapping).unwrap();
        let unwrapped = unwrap_bytes(&wrapped, &wrapping).unwrap();
        assert_eq!(unwrapped, key);
    }

    #[test]
    fn recovery_code_roundtrip() {
        let raw = generate_file_key();
        let formatted = format_recovery_code(&raw);
        let parsed = parse_recovery_code(&formatted).unwrap();
        assert_eq!(parsed, raw);
    }
}
