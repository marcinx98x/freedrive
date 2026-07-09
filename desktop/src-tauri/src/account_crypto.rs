use crate::api::ApiClient;
use crate::crypto::{self, format_recovery_code, generate_file_key, key_from_b64url, key_to_b64url};
use crate::db::{config_get, config_set, list_all_file_keys, store_file_key, DbHandle};
use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use rand::RngCore;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

static UEK: LazyLock<Mutex<Option<[u8; 32]>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Deserialize)]
struct CryptoAccountResponse {
    has_crypto: bool,
    has_recovery: Option<bool>,
    key_salt: Option<Vec<u8>>,
    wrapped_uek: Option<String>,
    wrapped_uek_recovery: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EncryptionKeyEntry {
    file_id: String,
    wrapped_file_key: String,
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EncryptionKeysListResponse {
    keys: Vec<EncryptionKeyEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEncryptionKeyResponse {
    wrapped_file_key: String,
}

#[derive(Debug, Deserialize)]
struct BulkImportResponse {
    imported: usize,
}

fn keyring_service() -> &'static str {
    "freedrive-desktop"
}

fn keyring_user(user_id: &str) -> String {
    format!("uek_{user_id}")
}

pub fn set_uek(user_id: &str, uek: [u8; 32]) -> AppResult<()> {
    *UEK.lock() = Some(uek);
    save_uek_keyring(user_id, &uek)?;
    Ok(())
}

pub fn get_uek(user_id: &str) -> Option<[u8; 32]> {
    if let Some(uek) = *UEK.lock() {
        return Some(uek);
    }
    if let Ok(Some(uek)) = load_uek_keyring(user_id) {
        *UEK.lock() = Some(uek);
        return Some(uek);
    }
    None
}

pub fn clear_uek(user_id: &str) {
    *UEK.lock() = None;
    let _ = keyring::Entry::new(keyring_service(), &keyring_user(user_id)).and_then(|e| e.delete_credential());
}

fn save_uek_keyring(user_id: &str, uek: &[u8; 32]) -> AppResult<()> {
    let entry = keyring::Entry::new(keyring_service(), &keyring_user(user_id))
        .map_err(|e| AppError::msg(e.to_string()))?;
    entry
        .set_password(&hex::encode(uek))
        .map_err(|e| AppError::msg(e.to_string()))
}

fn load_uek_keyring(user_id: &str) -> AppResult<Option<[u8; 32]>> {
    let entry = keyring::Entry::new(keyring_service(), &keyring_user(user_id))
        .map_err(|e| AppError::msg(e.to_string()))?;
    match entry.get_password() {
        Ok(hex_str) => {
            let bytes = hex::decode(hex_str).map_err(|e| AppError::msg(e.to_string()))?;
            if bytes.len() != 32 {
                return Err(AppError::msg("invalid keyring UEK length"));
            }
            let mut uek = [0u8; 32];
            uek.copy_from_slice(&bytes);
            Ok(Some(uek))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::msg(e.to_string())),
    }
}

pub struct UnlockResult {
    pub setup: bool,
    pub recovery_code: Option<String>,
}

pub async fn unlock_after_login(
    client: &ApiClient,
    db: &DbHandle,
    user_id: &str,
    password: &str,
) -> AppResult<UnlockResult> {
    let account: CryptoAccountResponse =
        serde_json::from_value(client.get_crypto_account().await?)?;
    if !account.has_crypto {
        let uek = generate_file_key();
        let recovery_key = generate_file_key();
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let kek = crypto::derive_kek(password, &salt)?;
        let wrapped_uek = crypto::wrap_bytes(&uek, &kek)?;
        let wrapped_recovery = crypto::wrap_bytes(&uek, &recovery_key)?;
        client
            .setup_crypto_account(&salt, &wrapped_uek, Some(&wrapped_recovery))
            .await?;
        set_uek(user_id, uek)?;
        sync_all_keys(client, db, user_id, &uek).await?;
        return Ok(UnlockResult {
            setup: true,
            recovery_code: Some(format_recovery_code(&recovery_key)),
        });
    }

    let salt = account
        .key_salt
        .ok_or_else(|| AppError::msg("missing key salt"))?;
    let wrapped = account
        .wrapped_uek
        .ok_or_else(|| AppError::msg("missing wrapped UEK"))?;
    let kek = crypto::derive_kek(password, &salt)?;
    let uek_bytes = crypto::unwrap_bytes(&wrapped, &kek)?;
    if uek_bytes.len() != 32 {
        return Err(AppError::msg("invalid UEK length"));
    }
    let mut uek = [0u8; 32];
    uek.copy_from_slice(&uek_bytes);
    set_uek(user_id, uek)?;
    sync_all_keys(client, db, user_id, &uek).await?;
    Ok(UnlockResult {
        setup: false,
        recovery_code: None,
    })
}

pub async fn try_unlock_from_keyring(
    client: &ApiClient,
    db: &DbHandle,
    user_id: &str,
) -> AppResult<bool> {
    let Some(uek) = get_uek(user_id) else {
        return Ok(false);
    };
    sync_all_keys(client, db, user_id, &uek).await?;
    Ok(true)
}

async fn sync_all_keys(
    client: &ApiClient,
    db: &DbHandle,
    user_id: &str,
    uek: &[u8; 32],
) -> AppResult<()> {
    pull_keys_from_server(client, db, uek).await?;
    push_local_keys_to_server(client, db, uek).await?;
    pull_keys_from_server(client, db, uek).await?;
    let _ = user_id;
    Ok(())
}

async fn pull_keys_from_server(
    client: &ApiClient,
    db: &DbHandle,
    uek: &[u8; 32],
) -> AppResult<()> {
    let since = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        config_get(&conn, "crypto_sync_since").ok().flatten()
    };
    let mut cursor = since.unwrap_or_default();
    loop {
        let resp: EncryptionKeysListResponse =
            serde_json::from_value(client.list_encryption_keys(&cursor).await?)?;
        if resp.keys.is_empty() {
            break;
        }
        let count = resp.keys.len();
        {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            for entry in &resp.keys {
                if let Ok(raw) = crypto::unwrap_bytes(&entry.wrapped_file_key, uek) {
                    if raw.len() == 32 {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&raw);
                        let _ = store_file_key(&conn, &entry.file_id, &key_to_b64url(&key));
                    }
                }
                if let Some(ref ts) = entry.updated_at {
                    cursor = ts.clone();
                }
            }
            if !cursor.is_empty() {
                let _ = config_set(&conn, "crypto_sync_since", &cursor);
            }
        }
        if count < 5000 {
            break;
        }
    }
    Ok(())
}

async fn push_local_keys_to_server(
    client: &ApiClient,
    db: &DbHandle,
    uek: &[u8; 32],
) -> AppResult<()> {
    let local_keys = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        list_all_file_keys(&conn)?
    };
    if local_keys.is_empty() {
        return Ok(());
    }
    let mut wrapped = HashMap::new();
    for (file_id, key_b64) in local_keys {
        if let Ok(key) = key_from_b64url(&key_b64) {
            if let Ok(w) = crypto::wrap_bytes(&key, uek) {
                wrapped.insert(file_id, w);
            }
        }
    }
    if wrapped.is_empty() {
        return Ok(());
    }
    let _: BulkImportResponse =
        serde_json::from_value(client.bulk_put_encryption_keys(wrapped).await?)?;
    Ok(())
}

pub async fn push_file_key(
    client: &ApiClient,
    uek: &[u8; 32],
    file_id: &str,
    key_b64: &str,
) -> AppResult<()> {
    let key = key_from_b64url(key_b64)?;
    let wrapped = crypto::wrap_bytes(&key, uek)?;
    client.put_file_encryption_key(file_id, &wrapped).await
}

pub async fn fetch_and_store_file_key(
    client: &ApiClient,
    db: &DbHandle,
    uek: &[u8; 32],
    file_id: &str,
) -> AppResult<Option<String>> {
    let resp: FileEncryptionKeyResponse = match client.get_file_encryption_key(file_id).await {
        Ok(raw) => serde_json::from_value(raw).map_err(AppError::from)?,
        Err(e) if e.to_string().to_lowercase().contains("not found") => return Ok(None),
        Err(e) => return Err(e),
    };
    let raw = crypto::unwrap_bytes(&resp.wrapped_file_key, uek)?;
    if raw.len() != 32 {
        return Err(AppError::msg("invalid file key length"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&raw);
    let key_b64 = key_to_b64url(&key);
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    store_file_key(&conn, file_id, &key_b64)?;
    Ok(Some(key_b64))
}

pub async fn resolve_file_key(
    client: &ApiClient,
    db: &DbHandle,
    user_id: &str,
    file_id: &str,
) -> AppResult<String> {
    use crate::db::{
        config_get, get_file_key, get_pending_file_key, has_any_pending_file_key,
        my_drive_get_placeholder_by_remote_id,
    };
    use crate::my_drive::ROOT_FOLDER_CONFIG_KEY;
    use std::path::Path;

    {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        if let Some(key) = get_file_key(&conn, file_id)? {
            return Ok(key);
        }
    }

    let mut pending_file_name: Option<String> = None;
    {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        if let Some((rel_path, item_type, parent_remote_id)) =
            my_drive_get_placeholder_by_remote_id(&conn, file_id)?
        {
            if item_type == "file" {
                let file_name = Path::new(&rel_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| AppError::msg("invalid file path in placeholder"))?;
                pending_file_name = Some(file_name.to_string());
                let folder_id = parent_remote_id.unwrap_or_else(|| {
                    config_get(&conn, ROOT_FOLDER_CONFIG_KEY)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "root".to_string())
                });
                if let Some(key) = get_pending_file_key(&conn, &folder_id, file_name)? {
                    return Ok(key);
                }
            }
        }
    }

    if let Some(ref name) = pending_file_name {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        if has_any_pending_file_key(&conn, name)? {
            return Err(AppError::msg("file is still uploading"));
        }
    }

    if let Some(uek) = get_uek(user_id) {
        if let Some(key_b64) = fetch_and_store_file_key(client, db, &uek, file_id).await? {
            return Ok(key_b64);
        }
    }

    Err(AppError::msg(
        "encryption key not available on this device (uploaded via web?)",
    ))
}
