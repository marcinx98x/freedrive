use crate::api::types::{FileRecord, Folder};
use crate::cfapi::util::{file_fs_metadata, file_identity, folder_fs_metadata, path_to_wide};
use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Storage::CloudFilters::{
    CfCreatePlaceholders, CF_CREATE_FLAG_NONE, CF_PLACEHOLDER_CREATE_FLAGS,
    CF_PLACEHOLDER_CREATE_INFO,
};

pub const MY_DRIVE_FOLDER_NAME: &str = "My Drive";

#[derive(Debug, Default, Clone, Copy)]
pub struct PlaceholderCreateStats {
    pub created: u32,
    pub skipped_duplicates: u32,
}

pub fn is_duplicate_placeholder_error(error: &AppError) -> bool {
    let message = error.to_string();
    message.contains("0x800700B7")
        || message.contains("0x80070050")
        || message.to_ascii_lowercase().contains("already exists")
}

fn create_single_placeholder(
    parent_dir: &Path,
    name: &str,
    identity: &[u8],
    fs_metadata: windows::Win32::Storage::CloudFilters::CF_FS_METADATA,
) -> AppResult<()> {
    let parent_wide = path_to_wide(parent_dir);
    let wide_name = wide_filename(name);
    let mut info = CF_PLACEHOLDER_CREATE_INFO {
        RelativeFileName: PCWSTR(wide_name.as_ptr()),
        FsMetadata: fs_metadata,
        FileIdentity: identity.as_ptr() as *const _,
        FileIdentityLength: identity.len() as u32,
        Flags: CF_PLACEHOLDER_CREATE_FLAGS(0),
        Result: HRESULT(0),
        ..Default::default()
    };

    unsafe {
        CfCreatePlaceholders(
            PCWSTR(parent_wide.as_ptr()),
            std::slice::from_mut(&mut info),
            CF_CREATE_FLAG_NONE,
            None,
        )
        .map_err(|e| AppError::msg(format!("CfCreatePlaceholders failed for {}: {}", name, e)))?;
    }

    let _ = (parent_wide, wide_name);
    Ok(())
}

pub fn create_named_folder_placeholder(
    parent_dir: &Path,
    name: &str,
    remote_id: &str,
) -> AppResult<()> {
    let identity = file_identity("folder", remote_id);
    create_single_placeholder(parent_dir, name, &identity, folder_fs_metadata())
}

pub fn create_file_placeholder(parent_dir: &Path, file: &FileRecord) -> AppResult<()> {
    let name = file_display_name(&file.name);
    let identity = file_identity("file", &file.id);
    let updated = crate::cfapi::util::parse_rfc3339_unix(&file.updated_at);
    create_single_placeholder(parent_dir, &name, &identity, file_fs_metadata(file.size, updated))
}

pub fn create_placeholders(
    parent_dir: &Path,
    folders: &[Folder],
    files: &[FileRecord],
) -> AppResult<PlaceholderCreateStats> {
    let mut stats = PlaceholderCreateStats::default();
    if folders.is_empty() && files.is_empty() {
        return Ok(stats);
    }

    for folder in folders {
        match create_named_folder_placeholder(parent_dir, &folder.name, &folder.id) {
            Ok(()) => stats.created += 1,
            Err(e) if is_duplicate_placeholder_error(&e) => stats.skipped_duplicates += 1,
            Err(e) => return Err(e),
        }
    }

    for file in files {
        match create_file_placeholder(parent_dir, file) {
            Ok(()) => stats.created += 1,
            Err(e) if is_duplicate_placeholder_error(&e) => stats.skipped_duplicates += 1,
            Err(e) => return Err(e),
        }
    }

    Ok(stats)
}

fn file_display_name(name: &str) -> String {
    PathBuf::from(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name)
        .to_string()
}

fn wide_filename(name: &str) -> Vec<u16> {
    file_display_name(name)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_placeholder_error_detected() {
        let err = AppError::msg("CfCreatePlaceholders failed: already exists (0x800700B7)");
        assert!(is_duplicate_placeholder_error(&err));
        let err = AppError::msg("access denied");
        assert!(!is_duplicate_placeholder_error(&err));
    }
}
