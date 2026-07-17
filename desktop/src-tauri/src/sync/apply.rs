use crate::api::client::ApiClient;
use crate::api::types::SyncChange;
use crate::db::{
    delete_folder_mapping, delete_sync_state_row, list_folder_mappings, list_sync_folders,
    set_folder_mapping, upsert_sync_state_with_version, DbHandle,
};
use crate::error::{AppError, AppResult};
use crate::sync::engine::SyncEngine;
use crate::sync::log::sync_log;
use crate::sync::suppress::WatcherSuppress;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub async fn apply_remote_change(
    engine: &SyncEngine,
    api: &ApiClient,
    db: &DbHandle,
    suppress: &WatcherSuppress,
    computer_root_id: &str,
    change: &SyncChange,
) -> AppResult<()> {
    if change.operation == "snapshot" {
        return Ok(());
    }

    match change.operation.as_str() {
        "create" | "update" | "restore" => {
            if change.entity_type == "file" {
                apply_remote_file(engine, api, db, suppress, computer_root_id, change).await
            } else {
                apply_remote_folder_create(db, suppress, computer_root_id, change)
            }
        }
        "rename" | "move" => {
            if change.entity_type == "file" {
                apply_remote_file_rename(db, suppress, computer_root_id, change)
            } else {
                apply_remote_folder_rename(db, suppress, computer_root_id, change)
            }
        }
        "trash" | "permanent_delete" => apply_remote_delete(db, suppress, change),
        _ => Ok(()),
    }
}

fn resolve_sync_context(
    conn: &rusqlite::Connection,
    computer_root_id: &str,
    parent_id: Option<&str>,
    name: &str,
) -> AppResult<Option<(i64, String, String)>> {
    let parent_id = match parent_id {
        Some(id) if !id.is_empty() => id,
        _ => computer_root_id,
    };

    let folders = list_sync_folders(conn)?;
    let mut remote_to_relative: HashMap<String, (i64, String)> = HashMap::new();
    for sf in &folders {
        if crate::db::is_pending_remote_folder(&sf.remote_folder_id) {
            continue;
        }
        remote_to_relative.insert(sf.remote_folder_id.clone(), (sf.id, String::new()));
        for mapping in list_folder_mappings(conn, sf.id)? {
            remote_to_relative.insert(mapping.remote_folder_id, (sf.id, mapping.relative_path));
        }
    }

    let (sync_folder_id, parent_relative) = match remote_to_relative.get(parent_id) {
        Some(v) => v.clone(),
        None => return Ok(None),
    };

    let relative = if parent_relative.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent_relative, name)
    };

    let local_root = folders
        .iter()
        .find(|f| f.id == sync_folder_id)
        .map(|f| f.local_path.clone())
        .unwrap_or_default();

    Ok(Some((sync_folder_id, local_root, relative)))
}

async fn apply_remote_file(
    engine: &SyncEngine,
    api: &ApiClient,
    db: &DbHandle,
    suppress: &WatcherSuppress,
    computer_root_id: &str,
    change: &SyncChange,
) -> AppResult<()> {
    let (sync_folder_id, local_root, relative) = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        match resolve_sync_context(
            &conn,
            computer_root_id,
            change.parent_id.as_deref(),
            &change.name,
        )? {
            Some(ctx) => ctx,
            None => return Ok(()),
        }
    };

    let local_path = PathBuf::from(&local_root).join(relative.replace('/', "\\"));
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let pending_local = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        crate::db::list_pending_journal(&conn, 1000)?
            .into_iter()
            .any(|e| e.relative_path == relative && e.sync_folder_id == sync_folder_id)
    };
    if pending_local {
        return Ok(());
    }

    suppress.run_suppressed(&local_path, || {});
    let plaintext = api.download_file(&change.entity_id, None).await?;
    let tmp = local_path.with_extension("freedrive.tmp");
    std::fs::write(&tmp, &plaintext)?;
    std::fs::rename(&tmp, &local_path)?;

    let mtime = local_path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    upsert_sync_state_with_version(
        &conn,
        sync_folder_id,
        &relative,
        &local_path.to_string_lossy(),
        Some(&change.entity_id),
        None,
        mtime,
        Some(&change.occurred_at),
        change.version,
        "synced",
    )?;

    sync_log(format!("applied remote file {} -> {}", change.entity_id, relative));
    let _ = engine;
    Ok(())
}

fn apply_remote_folder_create(
    db: &DbHandle,
    suppress: &WatcherSuppress,
    computer_root_id: &str,
    change: &SyncChange,
) -> AppResult<()> {
    let (sync_folder_id, local_root, relative) = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        match resolve_sync_context(
            &conn,
            computer_root_id,
            change.parent_id.as_deref(),
            &change.name,
        )? {
            Some(ctx) => ctx,
            None => return Ok(()),
        }
    };

    let local_path = PathBuf::from(&local_root).join(relative.replace('/', "\\"));
    suppress.run_suppressed(&local_path, || {
        std::fs::create_dir_all(&local_path).ok();
    });

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    set_folder_mapping(&conn, sync_folder_id, &relative, &change.entity_id)?;
    Ok(())
}

fn apply_remote_file_rename(
    db: &DbHandle,
    suppress: &WatcherSuppress,
    computer_root_id: &str,
    change: &SyncChange,
) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    if let Some((sync_folder_id, old_relative)) =
        crate::db::get_sync_state_by_remote_file_id(&conn, &change.entity_id)?
    {
        let sf = list_sync_folders(&conn)?
            .into_iter()
            .find(|f| f.id == sync_folder_id);
        if let Some(sf) = sf {
            let old_path = PathBuf::from(&sf.local_path).join(old_relative.replace('/', "\\"));
            let new_relative = if change.parent_id.is_some() {
                resolve_sync_context(
                    &conn,
                    computer_root_id,
                    change.parent_id.as_deref(),
                    &change.name,
                )?
                .map(|(_, _, r)| r)
                .unwrap_or_else(|| change.name.clone())
            } else {
                Path::new(&old_relative)
                    .parent()
                    .map(|p| {
                        let parent = p.to_string_lossy().replace('\\', "/");
                        if parent.is_empty() {
                            change.name.clone()
                        } else {
                            format!("{}/{}", parent, change.name)
                        }
                    })
                    .unwrap_or_else(|| change.name.clone())
            };
            let new_path = PathBuf::from(&sf.local_path).join(new_relative.replace('/', "\\"));
            suppress.run_suppressed(&new_path, || {
                if old_path.exists() {
                    if let Some(parent) = new_path.parent() {
                        std::fs::create_dir_all(parent).ok();
                    }
                    let _ = std::fs::rename(&old_path, &new_path);
                }
            });
            delete_sync_state_row(&conn, sync_folder_id, &old_relative)?;
            upsert_sync_state_with_version(
                &conn,
                sync_folder_id,
                &new_relative,
                &new_path.to_string_lossy(),
                Some(&change.entity_id),
                None,
                None,
                Some(&change.occurred_at),
                change.version,
                "synced",
            )?;
        }
    }
    Ok(())
}

fn apply_remote_folder_rename(
    db: &DbHandle,
    suppress: &WatcherSuppress,
    _computer_root_id: &str,
    change: &SyncChange,
) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    let folders = list_sync_folders(&conn)?;
    for sf in folders {
        if let Some(old_rel) = find_relative_for_remote_folder(&conn, sf.id, &change.entity_id)? {
            let old_path = PathBuf::from(&sf.local_path).join(old_rel.replace('/', "\\"));
            let parent_relative = Path::new(&old_rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let new_relative = if parent_relative.is_empty() {
                change.name.clone()
            } else {
                format!("{}/{}", parent_relative, change.name)
            };
            let new_path = PathBuf::from(&sf.local_path).join(new_relative.replace('/', "\\"));
            suppress.run_suppressed(&new_path, || {
                if old_path.exists() {
                    if let Some(parent) = new_path.parent() {
                        std::fs::create_dir_all(parent).ok();
                    }
                    let _ = std::fs::rename(&old_path, &new_path);
                }
            });
            delete_folder_mapping(&conn, sf.id, &old_rel)?;
            set_folder_mapping(&conn, sf.id, &new_relative, &change.entity_id)?;
            break;
        }
    }
    Ok(())
}

fn find_relative_for_remote_folder(
    conn: &rusqlite::Connection,
    sync_folder_id: i64,
    remote_folder_id: &str,
) -> AppResult<Option<String>> {
    let sf = list_sync_folders(conn)?
        .into_iter()
        .find(|f| f.id == sync_folder_id);
    if let Some(sf) = sf {
        if sf.remote_folder_id == remote_folder_id {
            return Ok(Some(String::new()));
        }
    }
    for mapping in list_folder_mappings(conn, sync_folder_id)? {
        if mapping.remote_folder_id == remote_folder_id {
            return Ok(Some(mapping.relative_path));
        }
    }
    Ok(None)
}

fn apply_remote_delete(
    db: &DbHandle,
    suppress: &WatcherSuppress,
    change: &SyncChange,
) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    if change.entity_type == "file" {
        if let Some((sync_folder_id, relative)) =
            crate::db::get_sync_state_by_remote_file_id(&conn, &change.entity_id)?
        {
            let sf = list_sync_folders(&conn)?
                .into_iter()
                .find(|f| f.id == sync_folder_id);
            if let Some(sf) = sf {
                let local_path = PathBuf::from(&sf.local_path).join(relative.replace('/', "\\"));
                suppress.run_suppressed(&local_path, || {
                    let _ = std::fs::remove_file(&local_path);
                });
                delete_sync_state_row(&conn, sync_folder_id, &relative)?;
            }
        }
    } else if change.entity_type == "folder" {
        let folders = list_sync_folders(&conn)?;
        for sf in folders {
            if let Some(rel) = find_relative_for_remote_folder(&conn, sf.id, &change.entity_id)? {
                let local_path = PathBuf::from(&sf.local_path).join(rel.replace('/', "\\"));
                suppress.run_suppressed(&local_path, || {
                    let _ = std::fs::remove_dir_all(&local_path);
                });
                delete_folder_mapping(&conn, sf.id, &rel)?;
                conn.execute(
                    "DELETE FROM sync_state WHERE sync_folder_id = ?1 AND (relative_path = ?2 OR relative_path LIKE ?3)",
                    rusqlite::params![sf.id, rel, format!("{}/%", rel)],
                )?;
            }
        }
    }
    Ok(())
}

pub async fn apply_snapshot(
    engine: &SyncEngine,
    api: &ApiClient,
    db: &DbHandle,
    suppress: &WatcherSuppress,
    computer_root_id: &str,
    snapshot: &crate::api::types::ComputerSnapshot,
) -> AppResult<()> {
    for folder in &snapshot.folders {
        if folder.id == computer_root_id {
            continue;
        }
        let change = SyncChange {
            seq: 0,
            entity_type: "folder".into(),
            entity_id: folder.id.clone(),
            parent_id: folder.parent_id.clone(),
            operation: "create".into(),
            name: folder.name.clone(),
            version: 0,
            occurred_at: String::new(),
            payload: None,
            is_tombstone: false,
        };
        apply_remote_folder_create(db, suppress, computer_root_id, &change)?;
    }
    for file in &snapshot.files {
        let change = SyncChange {
            seq: 0,
            entity_type: "file".into(),
            entity_id: file.id.clone(),
            parent_id: file.folder_id.clone(),
            operation: "create".into(),
            name: file.name.clone(),
            version: file.version,
            occurred_at: file.updated_at.clone(),
            payload: None,
            is_tombstone: false,
        };
        apply_remote_file(engine, api, db, suppress, computer_root_id, &change).await?;
    }
    Ok(())
}
