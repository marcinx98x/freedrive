//! Local decrypted thumbnail cache for Explorer ThumbnailProvider.
//! Encrypted thumbs live on the server; this cache is plaintext JPEG under AppData.

use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub fn thumbs_dir() -> AppResult<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| AppError::msg("no local app data dir"))?
        .join("FreeDrive")
        .join("thumbs");
    fs::create_dir_all(&base)?;
    Ok(base)
}

fn safe_id(file_id: &str) -> String {
    file_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn path_cache_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().to_lowercase().replace('/', "\\");
    let digest = Sha256::digest(normalized.as_bytes());
    hex::encode(digest)
}

pub fn cache_path_for_file_id(file_id: &str) -> AppResult<PathBuf> {
    Ok(thumbs_dir()?.join(format!("{}.jpg", safe_id(file_id))))
}

pub fn cache_path_for_local_path(path: &Path) -> AppResult<PathBuf> {
    Ok(thumbs_dir()?.join(format!("p_{}.jpg", path_cache_key(path))))
}

pub fn write_jpeg_cache(file_id: &str, jpeg: &[u8]) -> AppResult<PathBuf> {
    let path = cache_path_for_file_id(file_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, jpeg)?;
    Ok(path)
}

/// Write both file-id and absolute-path keyed caches (Explorer resolves by path).
pub fn write_jpeg_cache_for_path(file_id: &str, absolute_path: &Path, jpeg: &[u8]) -> AppResult<()> {
    write_jpeg_cache(file_id, jpeg)?;
    let by_path = cache_path_for_local_path(absolute_path)?;
    fs::write(by_path, jpeg)?;
    Ok(())
}

pub fn read_jpeg_cache(file_id: &str) -> Option<Vec<u8>> {
    let path = cache_path_for_file_id(file_id).ok()?;
    fs::read(path).ok()
}

pub fn read_jpeg_cache_by_path(path: &Path) -> Option<Vec<u8>> {
    let p = cache_path_for_local_path(path).ok()?;
    fs::read(p).ok()
}

pub fn cache_exists(file_id: &str) -> bool {
    cache_path_for_file_id(file_id)
        .map(|p| p.is_file())
        .unwrap_or(false)
}
