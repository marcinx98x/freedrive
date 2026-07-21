//! Local My Drive folder cleanup (logout / uninstall).

use crate::auth_store::{my_drive_path, sync_root_dir};
use crate::db::{my_drive_clear_placeholders, DbHandle};
use crate::error::AppResult;
use crate::sync::log::sync_log;
use std::fs;
use std::path::Path;

/// Remove all children inside `~/FreeDrive/My Drive` but keep the folder itself.
pub fn clear_my_drive_contents(db: &DbHandle) -> AppResult<()> {
    let dir = match my_drive_path(false) {
        Ok(p) => p,
        Err(e) => {
            sync_log(format!("clear My Drive: resolve path failed: {}", e));
            return Ok(());
        }
    };
    if !dir.is_dir() {
        clear_placeholders_best_effort(db);
        return Ok(());
    }

    remove_dir_children(&dir);
    clear_placeholders_best_effort(db);
    let _ = fs::create_dir_all(&dir);
    sync_log("cleared My Drive contents (folder kept)");
    Ok(())
}

/// Unregister CfAPI sync root (best-effort) and delete the entire My Drive folder.
pub fn uninstall_remove_my_drive(db: &DbHandle) -> AppResult<()> {
    #[cfg(windows)]
    crate::cfapi::unregister_for_uninstall(db);

    let my_drive = my_drive_path(false)?;
    if my_drive.exists() {
        if let Err(e) = remove_path_recursive(&my_drive) {
            sync_log(format!("uninstall remove My Drive failed: {}", e));
        } else {
            sync_log("removed My Drive folder on uninstall");
        }
    }

    clear_placeholders_best_effort(db);

    if let Ok(root) = sync_root_dir(false) {
        if root.is_dir() && dir_is_empty(&root) {
            let _ = fs::remove_dir(&root);
        }
    }
    Ok(())
}

fn clear_placeholders_best_effort(db: &DbHandle) {
    if let Ok(conn) = db.lock() {
        let _ = my_drive_clear_placeholders(&conn);
    }
}

fn remove_dir_children(dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            sync_log(format!("read My Drive children failed: {}", e));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Err(e) = remove_path_recursive(&path) {
            sync_log(format!("remove {} failed: {}", path.display(), e));
        }
    }
}

fn remove_path_recursive(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        if path.is_dir() {
            clear_readonly_recursive(path);
            fs::remove_dir_all(path)
        } else {
            clear_readonly_file(path);
            fs::remove_file(path).or_else(|_| fs::remove_dir_all(path))
        }
    }
    #[cfg(not(windows))]
    {
        if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }
}

fn dir_is_empty(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|mut it| it.next().is_none())
        .unwrap_or(false)
}

#[cfg(windows)]
fn clear_readonly_file(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(path, perms);
        }
    }
}

#[cfg(windows)]
fn clear_readonly_recursive(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                clear_readonly_recursive(&p);
            } else {
                clear_readonly_file(&p);
            }
        }
    }
    clear_readonly_file(dir);
}
