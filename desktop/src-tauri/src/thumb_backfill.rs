//! Idle backfill: populate local thumb cache from hydrated files or server thumbs.
//! For online-only files without a server thumb, download once into the hydrate cache
//! (no Explorer FETCH_DATA), generate JPEG, PUT thumbnail, then keep only the small cache.

use crate::api::client::ApiClient;
use crate::auth_store::my_drive_path;
use crate::cfapi::is_cloud_placeholder;
use crate::crypto::key_from_b64url;
use crate::db::{get_file_key, list_my_drive_media_placeholders, DbHandle};
use crate::my_drive::ensure_hydrated_plaintext;
use crate::sync::log::sync_log;
use crate::thumb_cache;
use crate::thumbnail;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static BACKFILL_STARTED: AtomicBool = AtomicBool::new(false);

const MAX_BYTES: u64 = 200 * 1024 * 1024;

/// Start a low-priority background task once per process.
pub fn start_thumb_backfill(db: DbHandle, api: ApiClient) {
    if BACKFILL_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        loop {
            if let Err(e) = run_pass(&db, &api) {
                sync_log(format!("thumb backfill: {e}"));
            }
            std::thread::sleep(Duration::from_secs(300));
        }
    });
}

fn run_pass(db: &DbHandle, api: &ApiClient) -> Result<(), String> {
    let root = my_drive_path(false).map_err(|e| e.to_string())?;
    if let Some(base) = dirs::data_local_dir() {
        let dir = base.join("FreeDrive");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("my_drive_root.txt"), root.to_string_lossy().as_bytes());
    }

    let candidates = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        list_my_drive_media_placeholders(&conn, 12).map_err(|e| e.to_string())?
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    for (remote_id, rel) in candidates {
        if thumb_cache::cache_exists(&remote_id) {
            continue;
        }
        let rel_trim = rel
            .strip_prefix("My Drive\\")
            .or_else(|| rel.strip_prefix("My Drive/"))
            .unwrap_or(&rel);
        let local = root.join(rel_trim);
        if !local.exists() {
            continue;
        }
        let meta = match std::fs::metadata(&local) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > MAX_BYTES {
            continue;
        }

        // Hydrated on disk: generate + upload + cache.
        if !is_cloud_placeholder(&local) {
            if let Some(jpeg) = thumbnail::generate_jpeg_thumbnail(&local) {
                let _ = thumb_cache::write_jpeg_cache_for_path(&remote_id, &local, &jpeg);
                if let Ok(conn) = db.lock() {
                    if let Ok(Some(k)) = get_file_key(&conn, &remote_id) {
                        if let Ok(key) = key_from_b64url(&k) {
                            let _ = rt.block_on(api.upload_file_thumbnail(&local, &remote_id, &key));
                        }
                    }
                }
                continue;
            }
        }

        let key = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            match get_file_key(&conn, &remote_id).ok().flatten() {
                Some(k) => key_from_b64url(&k).map_err(|e| e.to_string())?,
                None => continue,
            }
        };

        // Prefer existing encrypted server thumb (no full download).
        if let Ok(jpeg) = rt.block_on(api.download_file_thumbnail(&remote_id, &key)) {
            let _ = thumb_cache::write_jpeg_cache_for_path(&remote_id, &local, &jpeg);
            sync_log(format!("thumb backfill cached {}", local.display()));
            continue;
        }

        // Online-only without server thumb: silent hydrate to cache → gen → PUT.
        match rt.block_on(ensure_hydrated_plaintext(api, db, &remote_id)) {
            Ok(cache_path) => {
                if let Some(jpeg) = thumbnail::generate_jpeg_thumbnail(&cache_path) {
                    let _ = thumb_cache::write_jpeg_cache_for_path(&remote_id, &local, &jpeg);
                    let _ = rt.block_on(api.upload_file_thumbnail(&cache_path, &remote_id, &key));
                    sync_log(format!(
                        "thumb backfill generated+uploaded {}",
                        local.display()
                    ));
                }
            }
            Err(e) => {
                sync_log(format!(
                    "thumb backfill hydrate skip {}: {e}",
                    local.display()
                ));
            }
        }
    }
    Ok(())
}
