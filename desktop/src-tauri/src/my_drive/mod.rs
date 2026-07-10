mod provider;
#[cfg(windows)]
mod sync;

pub use provider::{
    fetch_folder_contents, hydrate_file, is_under_my_drive, relative_path_from_sync_root,
    resolve_folder_id_for_fetch, resolve_my_drive_root_id, FolderIdSource, ROOT_FOLDER_CONFIG_KEY,
};

#[cfg(windows)]
pub use sync::{delete_my_drive_path, poll_my_drive, upload_my_drive_path};

#[cfg(not(windows))]
pub async fn poll_my_drive(
    _api: &crate::api::ApiClient,
    _db: &crate::db::DbHandle,
    _mirror: bool,
    _download_sem: std::sync::Arc<tokio::sync::Semaphore>,
) -> crate::error::AppResult<()> {
    Ok(())
}

#[cfg(not(windows))]
pub async fn upload_my_drive_path(
    _api: &crate::api::ApiClient,
    _db: &crate::db::DbHandle,
    _path: &std::path::Path,
) -> crate::error::AppResult<()> {
    Ok(())
}

#[cfg(not(windows))]
pub async fn delete_my_drive_path(
    _api: &crate::api::ApiClient,
    _db: &crate::db::DbHandle,
    _path: &std::path::Path,
) -> crate::error::AppResult<()> {
    Ok(())
}
