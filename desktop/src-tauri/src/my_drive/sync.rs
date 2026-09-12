use crate::api::ApiClient;
use crate::auth_store::sync_root_dir;
use crate::cfapi::{
    clear_explicit_pin_state, convert_file_to_placeholder, create_file_placeholder,
    create_named_folder_placeholder, dehydrate_placeholder_file, ensure_cloud_placeholder,
    finalize_hydrated_file, finalize_stream_placeholder, is_cloud_placeholder,
    is_dehydrated_placeholder, is_duplicate_placeholder_error, is_not_cloud_file_error,
    is_unpinned, mark_directory_partially_populated, mark_hydrated_available,
    notify_directory_updated, on_disk_allocated_bytes, read_placeholder_identity,
    refresh_placeholder_status, MY_DRIVE_FOLDER_NAME,
};
use crate::crypto::key_to_b64url;
use crate::db::{
    get_file_key, insert_activity, my_drive_delete_placeholder,
    my_drive_delete_placeholders_under_prefix, my_drive_get_placeholder,
    my_drive_get_placeholder_by_remote_id, my_drive_list_placeholders,
    my_drive_relocate_placeholder, my_drive_reparent_direct_children, my_drive_upsert_placeholder,
    store_file_key, DbHandle, MyDrivePlaceholderRow,
};
use crate::error::{AppError, AppResult};
use crate::my_drive::{
    api_folder_parent_id, clear_all_hydrate_cache, clear_hydrate_cache_for_file,
    ensure_hydrated_plaintext, fetch_folder_contents, is_under_my_drive,
    relative_path_from_sync_root, resolve_my_drive_root_id,
};
use crate::sync::log::sync_log;
use crate::sync::suppress::WatcherSuppress;
use crate::sync::{DOWNLOAD_CONCURRENCY, UPLOAD_CONCURRENCY};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Per-file ceiling so one stuck CfDehydratePlaceholder cannot kill the whole Free up walk.
const FREE_UP_FILE_TIMEOUT: Duration = Duration::from_secs(45);
/// Cap silent auto-resumes after incomplete/cancelled Free up jobs.
const FREE_UP_MAX_AUTO_RESUME: u32 = 3;
/// Do not upload-first during Free up above this size (blocks the walk for minutes/hours).
const FREE_UP_MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// Serialize ensure_my_drive_folder_relative per path (watcher + poll race).
fn folder_ensure_lock(relative: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let key = relative.replace('/', "\\").to_ascii_lowercase();
    let map = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

static MY_DRIVE_UPLOAD_IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn my_drive_upload_in_flight() -> &'static Mutex<HashSet<String>> {
    MY_DRIVE_UPLOAD_IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

/// RAII claim so close+watcher+poll cannot encrypt the same path concurrently.
pub struct MyDriveUploadInFlightGuard {
    key: String,
}

impl Drop for MyDriveUploadInFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = my_drive_upload_in_flight().lock() {
            set.remove(&self.key);
        }
    }
}

/// Returns `None` when this path is already being uploaded.
pub fn try_claim_my_drive_upload(path: &Path) -> Option<MyDriveUploadInFlightGuard> {
    let key = path.to_string_lossy().to_ascii_lowercase();
    let Ok(mut set) = my_drive_upload_in_flight().lock() else {
        return Some(MyDriveUploadInFlightGuard { key });
    };
    if !set.insert(key.clone()) {
        return None;
    }
    Some(MyDriveUploadInFlightGuard { key })
}

/// At most one free-up tree walk at a time so FETCH_DATA downloads are not starved.
static FREE_UP_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
/// Root path of the in-progress free-up (NOTIFY_FILE_CLOSE must ignore under this tree).
static FREE_UP_ACTIVE_ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
/// Cleared on begin; set true only when `free_up_my_drive_path` returns normally.
static FREE_UP_COMPLETED: OnceLock<AtomicBool> = OnceLock::new();
/// Live progress for incomplete-drop diagnostics.
static FREE_UP_RUN: OnceLock<Mutex<Option<FreeUpRunState>>> = OnceLock::new();
/// Resume counts keyed by lowercase root path.
static FREE_UP_RESUME_COUNT: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
/// Optional fallback queue when no auto-resume hook is registered yet.
static FREE_UP_PENDING_RESUME: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
/// Engine installs this to re-queue after a silent incomplete end.
static FREE_UP_AUTO_RESUME: OnceLock<Box<dyn Fn(PathBuf) + Send + Sync>> = OnceLock::new();

struct FreeUpRunState {
    last_file: Option<String>,
    processed: u64,
    total: u64,
    freed: u32,
    failed: u32,
}

fn free_up_semaphore() -> &'static Semaphore {
    FREE_UP_SEMAPHORE.get_or_init(|| Semaphore::new(1))
}

fn free_up_active_root() -> &'static Mutex<Option<PathBuf>> {
    FREE_UP_ACTIVE_ROOT.get_or_init(|| Mutex::new(None))
}

fn free_up_run() -> &'static Mutex<Option<FreeUpRunState>> {
    FREE_UP_RUN.get_or_init(|| Mutex::new(None))
}

fn free_up_completed_flag() -> &'static AtomicBool {
    FREE_UP_COMPLETED.get_or_init(|| AtomicBool::new(false))
}

/// Called by SyncEngine so incomplete Free up can re-queue after a short delay.
pub fn register_free_up_auto_resume(f: impl Fn(PathBuf) + Send + Sync + 'static) {
    let _ = FREE_UP_AUTO_RESUME.set(Box::new(f));
}

/// Poll fallback if the auto-resume hook was not installed.
pub fn take_pending_free_up_resume() -> Option<PathBuf> {
    FREE_UP_PENDING_RESUME
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|mut g| g.take()))
}

struct FreeUpActiveGuard;

impl Drop for FreeUpActiveGuard {
    fn drop(&mut self) {
        let root = free_up_active_root()
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        let completed = free_up_completed_flag().load(Ordering::SeqCst);
        let run = free_up_run().lock().ok().and_then(|mut g| g.take());
        if completed {
            return;
        }
        let (last, processed, total, freed, failed) = match &run {
            Some(r) => (
                r.last_file
                    .clone()
                    .unwrap_or_else(|| "(unknown)".to_string()),
                r.processed,
                r.total,
                r.freed,
                r.failed,
            ),
            None => ("(unknown)".to_string(), 0, 0, 0, 0),
        };
        let root_disp = root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".to_string());
        sync_log(format!(
            "My Drive free-up ended incomplete — root={} last={} processed={}/{} freed={} failed={}",
            root_disp, last, processed, total, freed, failed
        ));
        if let Some(root) = root {
            maybe_schedule_free_up_resume(root);
        }
    }
}

fn maybe_schedule_free_up_resume(root: PathBuf) {
    let key = root.to_string_lossy().to_ascii_lowercase();
    let count = {
        let map = FREE_UP_RESUME_COUNT.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(key).or_insert(0);
        *entry += 1;
        *entry
    };
    if count > FREE_UP_MAX_AUTO_RESUME {
        sync_log(format!(
            "My Drive free-up auto-resume exhausted ({count}) — {}",
            root.display()
        ));
        return;
    }
    sync_log(format!(
        "My Drive free-up auto-resume scheduled ({count}/{FREE_UP_MAX_AUTO_RESUME}) — {}",
        root.display()
    ));
    if let Some(hook) = FREE_UP_AUTO_RESUME.get() {
        hook(root);
        return;
    }
    let slot = FREE_UP_PENDING_RESUME.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(root);
    }
}

fn mark_free_up_completed() {
    free_up_completed_flag().store(true, Ordering::SeqCst);
    if let Ok(guard) = free_up_active_root().lock() {
        if let Some(root) = guard.as_ref() {
            let key = root.to_string_lossy().to_ascii_lowercase();
            if let Some(map) = FREE_UP_RESUME_COUNT.get() {
                if let Ok(mut g) = map.lock() {
                    g.remove(&key);
                }
            }
        }
    }
}

fn begin_free_up_active(path: &Path) -> FreeUpActiveGuard {
    free_up_completed_flag().store(false, Ordering::SeqCst);
    if let Ok(mut guard) = free_up_active_root().lock() {
        *guard = Some(path.to_path_buf());
    }
    if let Ok(mut run) = free_up_run().lock() {
        *run = Some(FreeUpRunState {
            last_file: None,
            processed: 0,
            total: 0,
            freed: 0,
            failed: 0,
        });
    }
    FreeUpActiveGuard
}

fn update_free_up_progress(
    last: &Path,
    processed: u64,
    total: u64,
    freed: u32,
    failed: u32,
) {
    if let Ok(mut run) = free_up_run().lock() {
        if let Some(state) = run.as_mut() {
            state.last_file = Some(last.display().to_string());
            state.processed = processed;
            state.total = total;
            state.freed = freed;
            state.failed = failed;
        }
    }
}

fn set_free_up_total(total: u64) {
    if let Ok(mut run) = free_up_run().lock() {
        if let Some(state) = run.as_mut() {
            state.total = total;
        }
    }
}

/// Run CfAPI dehydrate off the async worker so a stuck call cannot stall the runtime.
async fn dehydrate_placeholder_file_async(path: &Path) -> AppResult<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || dehydrate_placeholder_file(&path))
        .await
        .map_err(|e| AppError::msg(format!("dehydrate task join: {e}")))?
}

async fn convert_file_to_placeholder_async(path: &Path, remote_id: &str) -> AppResult<()> {
    let path = path.to_path_buf();
    let remote_id = remote_id.to_string();
    tokio::task::spawn_blocking(move || convert_file_to_placeholder(&path, &remote_id))
        .await
        .map_err(|e| AppError::msg(format!("convert task join: {e}")))?
}

/// True while Free up space is walking `path` or an ancestor of it.
pub fn is_path_under_active_free_up(path: &Path) -> bool {
    let Ok(guard) = free_up_active_root().lock() else {
        return false;
    };
    let Some(root) = guard.as_ref() else {
        return false;
    };
    path_is_under_prefix(path, root)
}

/// True while any Free up space operation is in progress.
pub fn is_free_up_in_progress() -> bool {
    free_up_active_root()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|_| ()))
        .is_some()
}

/// How long a delete-in-flight mark suppresses CLOSE upload / re-upload (Explorer delete race).
const DELETE_IN_FLIGHT_TTL: Duration = Duration::from_secs(60);
/// Cap concurrent My Drive soft-trash HTTP so large folder deletes cannot melt the API.
const MY_DRIVE_DELETE_CONCURRENCY: usize = 3;

/// Paths (files or folder prefixes) with soft-trash in flight after NOTIFY_DELETE ACK.
static DELETE_IN_FLIGHT: OnceLock<Mutex<HashMap<PathBuf, Instant>>> = OnceLock::new();
static MY_DRIVE_DELETE_SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
static FOLDER_DELETE_CLAIMED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn delete_in_flight() -> &'static Mutex<HashMap<PathBuf, Instant>> {
    DELETE_IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn my_drive_delete_semaphore() -> &'static Semaphore {
    MY_DRIVE_DELETE_SEMAPHORE.get_or_init(|| Semaphore::new(MY_DRIVE_DELETE_CONCURRENCY))
}

fn folder_delete_claimed() -> &'static Mutex<HashSet<String>> {
    FOLDER_DELETE_CLAIMED.get_or_init(|| Mutex::new(HashSet::new()))
}

struct FolderDeleteClaim {
    remote_id: String,
}

impl Drop for FolderDeleteClaim {
    fn drop(&mut self) {
        if let Ok(mut set) = folder_delete_claimed().lock() {
            set.remove(&self.remote_id);
        }
    }
}

fn try_claim_folder_delete(remote_id: &str) -> Option<FolderDeleteClaim> {
    if remote_id.is_empty() {
        return None;
    }
    let Ok(mut set) = folder_delete_claimed().lock() else {
        return Some(FolderDeleteClaim {
            remote_id: remote_id.to_string(),
        });
    };
    if !set.insert(remote_id.to_string()) {
        return None;
    }
    Some(FolderDeleteClaim {
        remote_id: remote_id.to_string(),
    })
}

/// Mark `path` so NOTIFY_FILE_CLOSE / watcher uploads skip it (and descendants for folders).
pub fn mark_delete_in_flight(path: &Path) {
    let Ok(mut map) = delete_in_flight().lock() else {
        return;
    };
    let now = Instant::now();
    map.retain(|_, at| now.duration_since(*at) < DELETE_IN_FLIGHT_TTL);
    map.insert(path.to_path_buf(), now);
}

/// Clear the mark for `path` after soft-trash finishes (success or error).
pub fn clear_delete_in_flight(path: &Path) {
    let Ok(mut map) = delete_in_flight().lock() else {
        return;
    };
    map.remove(path);
}

/// True while Explorer delete soft-trash is in flight for `path` or an ancestor.
pub fn is_path_under_active_delete(path: &Path) -> bool {
    let Ok(mut map) = delete_in_flight().lock() else {
        return false;
    };
    let now = Instant::now();
    map.retain(|_, at| now.duration_since(*at) < DELETE_IN_FLIGHT_TTL);
    map.keys().any(|root| path_is_under_prefix(path, root))
}

/// True when an *ancestor* (not `path` itself) is already marked delete-in-flight.
/// Used to skip per-child HTTP after a folder soft-trash was started.
pub fn is_path_under_active_delete_ancestor(path: &Path) -> bool {
    let Ok(mut map) = delete_in_flight().lock() else {
        return false;
    };
    let now = Instant::now();
    map.retain(|_, at| now.duration_since(*at) < DELETE_IN_FLIGHT_TTL);
    let path_s = path.to_string_lossy().to_ascii_lowercase();
    map.keys().any(|root| {
        let root_s = root.to_string_lossy().to_ascii_lowercase();
        if path_s == root_s {
            return false;
        }
        path_is_under_prefix(path, root)
    })
}

fn path_is_under_prefix(path: &Path, root: &Path) -> bool {
    let path_s = path.to_string_lossy().to_ascii_lowercase();
    let root_s = root.to_string_lossy().to_ascii_lowercase();
    if path_s == root_s {
        return true;
    }
    let root_prefix = if root_s.ends_with('\\') {
        root_s
    } else {
        format!("{root_s}\\")
    };
    path_s.starts_with(&root_prefix)
}

fn is_blob_missing_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("failed to read file")
        || lower.contains("blob missing")
        || lower.contains("blob unreadable")
}

/// Callback when My Drive starts transferring (upload/download/folder create).
pub type MyDriveBusyCb = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Debug, Default, Clone)]
pub struct MyDrivePollStats {
    pub folders_created: u32,
    pub files_uploaded: u32,
    pub files_mirrored: u32,
    pub errors: u32,
}

impl MyDrivePollStats {
    pub fn did_work(&self) -> bool {
        self.folders_created > 0 || self.files_uploaded > 0 || self.files_mirrored > 0
    }
}

fn notify_my_drive_busy(on_busy: &Option<MyDriveBusyCb>) {
    if let Some(cb) = on_busy {
        cb("Syncing My Drive…");
    }
}

pub async fn poll_my_drive(
    api: &ApiClient,
    db: &DbHandle,
    mirror: bool,
    download_sem: Arc<Semaphore>,
    upload_sem: Arc<Semaphore>,
    suppress: Option<&WatcherSuppress>,
    on_busy: Option<MyDriveBusyCb>,
) -> AppResult<MyDrivePollStats> {
    let sync_root = sync_root_dir(false)?;
    sync_log(&format!("poll My Drive started (mirror={})", mirror));
    let mut stats = MyDrivePollStats::default();
    // Push offline local deletes/moves before recreating placeholders from the server.
    if let Err(e) = reconcile_my_drive_offline_changes(api, db, &sync_root, &on_busy).await {
        sync_log(format!("My Drive offline reconcile skipped: {e}"));
        stats.errors = stats.errors.saturating_add(1);
    }
    poll_my_drive_folder(
        api,
        db,
        &sync_root,
        MY_DRIVE_FOLDER_NAME,
        None,
        mirror,
        download_sem,
        upload_sem,
        suppress,
        &on_busy,
        &mut stats,
    )
    .await?;
    notify_directory_updated(&local_dir_for_relative(&sync_root, MY_DRIVE_FOLDER_NAME));
    sync_log(format!(
        "poll My Drive finished (folders={} uploaded={} mirrored={} errors={})",
        stats.folders_created, stats.files_uploaded, stats.files_mirrored, stats.errors
    ));
    Ok(stats)
}

/// Index on-disk CfAPI placeholders under My Drive by remote id.
fn index_my_drive_identities(sync_root: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let root = local_dir_for_relative(sync_root, MY_DRIVE_FOLDER_NAME);
    if !root.is_dir() {
        return map;
    }
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some((ty, id)) = read_placeholder_identity(&path) {
                    if (ty == "folder" || ty == "file") && !id.is_empty() {
                        map.entry(id).or_insert(path.clone());
                    }
                }
                stack.push(path);
            } else if path.is_file() {
                if let Some((ty, id)) = read_placeholder_identity(&path) {
                    if ty == "file" && !id.is_empty() {
                        map.entry(id).or_insert(path);
                    }
                }
            }
        }
    }
    map
}

fn path_exists_for_placeholder(sync_root: &Path, relative: &str, item_type: &str) -> bool {
    let path = local_dir_for_relative(sync_root, relative);
    if item_type == "folder" {
        path.is_dir()
    } else {
        path.is_file()
    }
}

/// Drive Stream parity: confirm remote object by ID before any destructive sync action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemotePresence {
    Exists,
    Missing,
    Unknown,
}

async fn probe_my_drive_remote_exists(
    api: &ApiClient,
    item_type: &str,
    remote_id: &str,
) -> RemotePresence {
    if remote_id.is_empty() {
        return RemotePresence::Missing;
    }
    let result = if item_type == "folder" {
        // Lightweight listing probe (one page) — 404 means gone.
        api.probe_folder(remote_id).await
    } else {
        api.get_file(remote_id).await.map(|_| ())
    };
    match result {
        Ok(()) => RemotePresence::Exists,
        Err(e) if e.is_not_found() => RemotePresence::Missing,
        Err(e) => {
            let lower = e.to_string().to_ascii_lowercase();
            if lower.contains("not found") || lower.contains("(404)") {
                RemotePresence::Missing
            } else {
                RemotePresence::Unknown
            }
        }
    }
}

fn paths_equal_ci(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

fn is_under_relative_prefix(relative: &str, prefix: &str) -> bool {
    let rel = relative.replace('/', "\\").to_ascii_lowercase();
    let pre = prefix.replace('/', "\\").to_ascii_lowercase();
    if rel == pre {
        return true;
    }
    let pre = pre.trim_end_matches('\\');
    rel.starts_with(&format!("{pre}\\"))
}

/// Infer new folder path after offline move when the folder placeholder lost FileIdentity
/// but children still carry theirs under a shared parent directory.
fn infer_folder_dest_from_children(
    folder: &MyDrivePlaceholderRow,
    all_rows: &[MyDrivePlaceholderRow],
    identity_index: &HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    let prefix = format!(
        "{}\\",
        folder.relative_path.trim_end_matches(['\\', '/'])
    );
    let prefix_l = prefix.to_ascii_lowercase();
    let mut parent_votes: HashMap<String, (PathBuf, u32)> = HashMap::new();
    for row in all_rows {
        if row.remote_id.is_empty() || row.remote_id == folder.remote_id {
            continue;
        }
        let under = row
            .relative_path
            .replace('/', "\\")
            .to_ascii_lowercase()
            .starts_with(&prefix_l);
        let by_parent = row.parent_remote_id.as_deref() == Some(folder.remote_id.as_str());
        if !under && !by_parent {
            continue;
        }
        let Some(found) = identity_index.get(&row.remote_id) else {
            continue;
        };
        let Some(parent) = found.parent() else {
            continue;
        };
        let key = parent.to_string_lossy().to_ascii_lowercase();
        let entry = parent_votes.entry(key).or_insert_with(|| (parent.to_path_buf(), 0));
        entry.1 += 1;
    }
    parent_votes
        .into_values()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map(|(path, _)| path)
}

fn folder_has_any_child_identity_on_disk(
    folder: &MyDrivePlaceholderRow,
    all_rows: &[MyDrivePlaceholderRow],
    identity_index: &HashMap<String, PathBuf>,
) -> bool {
    let prefix = format!(
        "{}\\",
        folder.relative_path.trim_end_matches(['\\', '/'])
    );
    let prefix_l = prefix.to_ascii_lowercase();
    all_rows.iter().any(|row| {
        if row.remote_id.is_empty() || row.remote_id == folder.remote_id {
            return false;
        }
        let under = row
            .relative_path
            .replace('/', "\\")
            .to_ascii_lowercase()
            .starts_with(&prefix_l);
        let by_parent = row.parent_remote_id.as_deref() == Some(folder.remote_id.as_str());
        (under || by_parent) && identity_index.contains_key(&row.remote_id)
    })
}

/// True if path is a cloud placeholder, reparse point, or has FileIdentity.
fn path_looks_like_cloud_placeholder(path: &Path) -> bool {
    path_has_reparse_point(path)
        || is_cloud_placeholder(path)
        || read_placeholder_identity(path).is_some()
}

/// Recursive: any cloud/reparse placeholder under `dir` (incl. `dir` itself).
fn tree_has_cloud_or_reparse(dir: &Path) -> bool {
    if path_looks_like_cloud_placeholder(dir) {
        return true;
    }
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0u32;
    const MAX_NODES: u32 = 50_000;
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_NODES {
                // Treat oversized trees conservatively as cloud-ish to avoid empty create.
                return true;
            }
            let path = entry.path();
            if path_looks_like_cloud_placeholder(&path) {
                return true;
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    false
}

/// Regular local file that can be uploaded (not placeholder / reparse / skip-name).
fn path_is_uploadable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if name.eq_ignore_ascii_case("desktop.ini")
        || name.starts_with('.')
        || crate::sync::should_skip_file(name)
    {
        return false;
    }
    if path_looks_like_cloud_placeholder(path) {
        return false;
    }
    true
}

/// True if tree contains at least one uploadable local file (early-exit DFS).
fn tree_has_uploadable_file(dir: &Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0u32;
    const MAX_NODES: u32 = 50_000;
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_NODES {
                return false;
            }
            let path = entry.path();
            if path.is_dir() {
                if path_looks_like_cloud_placeholder(&path) {
                    continue;
                }
                stack.push(path);
            } else if path_is_uploadable_file(&path) {
                return true;
            }
        }
    }
    false
}

/// Vote for folder remote id from children's DB `parent_remote_id` (folder identity lost on move).
fn infer_folder_remote_id_from_disk_children(dir: &Path, db: &DbHandle) -> Option<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut votes: HashMap<String, u32> = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some((_, remote_id)) = read_placeholder_identity(&path) else {
            continue;
        };
        if remote_id.is_empty() {
            continue;
        }
        let Ok(conn) = db.lock() else {
            continue;
        };
        if let Ok(Some((_, _, Some(parent_id)))) =
            my_drive_get_placeholder_by_remote_id(&conn, &remote_id)
        {
            if !parent_id.is_empty() {
                *votes.entry(parent_id).or_insert(0) += 1;
            }
        }
    }
    votes
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map(|(id, _)| id)
}

/// PATCH + DB relocate for a cloud item found at a new local path (offline move).
async fn relocate_my_drive_item_from_scan(
    api: &ApiClient,
    db: &DbHandle,
    sync_root: &Path,
    path: &Path,
    child_relative: &str,
    name: &str,
    remote_id: &str,
    item_type: &str,
    parent_folder_id: &str,
) -> AppResult<()> {
    let parent_opt = api_folder_parent_id(parent_folder_id);
    if item_type == "folder" {
        api.patch_folder(remote_id, Some(name), parent_opt, None)
            .await
            .map(|_| ())?;
    } else {
        api.patch_file(remote_id, Some(name), parent_opt, None)
            .await
            .map(|_| ())?;
    }
    if let Ok(conn) = db.lock() {
        if let Ok(Some((old_rel, _, _))) = my_drive_get_placeholder_by_remote_id(&conn, remote_id) {
            if !old_rel.eq_ignore_ascii_case(child_relative) {
                my_drive_relocate_placeholder(&conn, &old_rel, child_relative, parent_opt)?;
            } else {
                my_drive_upsert_placeholder(
                    &conn,
                    child_relative,
                    remote_id,
                    item_type,
                    parent_opt,
                    None,
                )?;
            }
        } else {
            my_drive_upsert_placeholder(
                &conn,
                child_relative,
                remote_id,
                item_type,
                parent_opt,
                None,
            )?;
        }
    }
    let _ = ensure_cloud_placeholder(path, item_type, remote_id);
    if item_type == "folder" {
        relink_orphan_placeholders_under(db, sync_root, path, remote_id);
    }
    Ok(())
}

/// Relink orphan placeholders under `dir` into DB using FileIdentity (after offline folder move).
fn relink_orphan_placeholders_under(
    db: &DbHandle,
    sync_root: &Path,
    dir: &Path,
    parent_remote_id: &str,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some((ty, remote_id)) = read_placeholder_identity(&path) else {
            continue;
        };
        if remote_id.is_empty() {
            continue;
        }
        let Some(rel) = relative_path_from_sync_root(sync_root, &path) else {
            continue;
        };
        let item_type = if ty == "folder" { "folder" } else { "file" };
        if let Ok(conn) = db.lock() {
            let existing = my_drive_get_placeholder_by_remote_id(&conn, &remote_id)
                .ok()
                .flatten();
            if let Some((old_rel, _, _)) = existing {
                if !old_rel.eq_ignore_ascii_case(&rel) {
                    let _ = my_drive_relocate_placeholder(
                        &conn,
                        &old_rel,
                        &rel,
                        Some(parent_remote_id),
                    );
                } else {
                    let _ = my_drive_upsert_placeholder(
                        &conn,
                        &rel,
                        &remote_id,
                        item_type,
                        Some(parent_remote_id),
                        None,
                    );
                }
            } else {
                let _ = my_drive_upsert_placeholder(
                    &conn,
                    &rel,
                    &remote_id,
                    item_type,
                    Some(parent_remote_id),
                    None,
                );
            }
            sync_log(format!("My Drive relinked orphan — {rel}"));
        }
    }
}

async fn resolve_parent_remote_id_for_relative(
    db: &DbHandle,
    sync_root: &Path,
    relative: &str,
) -> AppResult<String> {
    let parent_relative = Path::new(relative)
        .parent()
        .map(|p| p.to_string_lossy().replace('/', "\\"))
        .unwrap_or_else(|| MY_DRIVE_FOLDER_NAME.to_string());
    if parent_relative.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) {
        return resolve_my_drive_root_id(db);
    }
    {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        if let Some((id, ty, _)) = my_drive_get_placeholder(&conn, &parent_relative)? {
            if ty == "folder" && !id.is_empty() {
                return Ok(id);
            }
        }
    }
    // Parent may have been moved offline — DB path stale; read identity from disk.
    let parent_path = local_dir_for_relative(sync_root, &parent_relative);
    if let Some((ty, id)) = read_placeholder_identity(&parent_path) {
        if ty == "folder" && !id.is_empty() {
            return Ok(id);
        }
    }
    Err(AppError::msg(format!(
        "parent placeholder missing for {relative}"
    )))
}

/// Drive-like: after offline Explorer cleanup, push local deletes/moves before restore.
async fn reconcile_my_drive_offline_changes(
    api: &ApiClient,
    db: &DbHandle,
    sync_root: &Path,
    on_busy: &Option<MyDriveBusyCb>,
) -> AppResult<()> {
    let my_drive = local_dir_for_relative(sync_root, MY_DRIVE_FOLDER_NAME);
    if !my_drive.is_dir() {
        // Unavailable tree — do not mass soft-trash (same guard as computer sync folders).
        return Ok(());
    }

    let rows = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_list_placeholders(&conn)?
    };
    if rows.is_empty() {
        return Ok(());
    }

    if let Some(cb) = on_busy {
        cb("Reconciling My Drive…");
    }

    let identity_index = index_my_drive_identities(sync_root);
    let mut moves: Vec<(MyDrivePlaceholderRow, PathBuf)> = Vec::new();
    let mut missing: Vec<MyDrivePlaceholderRow> = Vec::new();

    for row in rows.iter() {
        if row.relative_path.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) {
            continue;
        }
        if row.remote_id.is_empty() {
            continue;
        }
        let expected = local_dir_for_relative(sync_root, &row.relative_path);
        if path_exists_for_placeholder(sync_root, &row.relative_path, &row.item_type) {
            continue;
        }
        if let Some(found) = identity_index.get(&row.remote_id) {
            if !paths_equal_ci(found, &expected) {
                moves.push((row.clone(), found.clone()));
            }
            continue;
        }
        // Folder identity often lost on Explorer move; infer destination from children.
        if row.item_type == "folder" {
            if let Some(dest) =
                infer_folder_dest_from_children(row, &rows, &identity_index)
            {
                moves.push((row.clone(), dest));
                continue;
            }
            // Children still on disk as placeholders somewhere — never soft-trash.
            if folder_has_any_child_identity_on_disk(row, &rows, &identity_index) {
                sync_log(format!(
                    "My Drive offline folder missing without dest — skipped delete {}",
                    row.relative_path
                ));
                continue;
            }
        }
        missing.push(row.clone());
    }

    let mut moved = 0u32;
    // Shallow paths first so folder relocate covers descendants.
    moves.sort_by(|a, b| a.0.relative_path.len().cmp(&b.0.relative_path.len()));
    let mut moved_old_roots: Vec<String> = Vec::new();
    for (row, new_path) in moves {
        if moved_old_roots
            .iter()
            .any(|r| is_under_relative_prefix(&row.relative_path, r))
        {
            continue;
        }
        let Some(new_rel) = relative_path_from_sync_root(sync_root, &new_path) else {
            continue;
        };
        if !is_under_my_drive(&new_rel) {
            continue;
        }
        let name = new_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&row.relative_path)
            .to_string();
        let parent_id = match resolve_parent_remote_id_for_relative(db, sync_root, &new_rel).await {
            Ok(id) => id,
            Err(e) => {
                sync_log(format!(
                    "My Drive offline move skipped {} → {}: {e}",
                    row.relative_path, new_rel
                ));
                continue;
            }
        };
        let patch = if row.item_type == "folder" {
            api.patch_folder(&row.remote_id, Some(&name), Some(&parent_id), None)
                .await
                .map(|_| ())
        } else {
            api.patch_file(&row.remote_id, Some(&name), Some(&parent_id), None)
                .await
                .map(|_| ())
        };
        match patch {
            Ok(()) => {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_relocate_placeholder(
                    &conn,
                    &row.relative_path,
                    &new_rel,
                    Some(&parent_id),
                )?;
                if row.item_type == "folder" {
                    moved_old_roots.push(row.relative_path.clone());
                    // Ensure folder placeholder identity on disk at the new path when possible.
                    drop(conn);
                    let _ = ensure_cloud_placeholder(&new_path, "folder", &row.remote_id);
                    relink_orphan_placeholders_under(db, sync_root, &new_path, &row.remote_id);
                }
                moved += 1;
                sync_log(format!(
                    "My Drive offline move — {} → {}",
                    row.relative_path, new_rel
                ));
            }
            Err(e) => sync_log(format!(
                "My Drive offline move failed {} → {}: {e}",
                row.relative_path, new_rel
            )),
        }
    }

    // Coalesce missing folders: topmost missing folder covers descendants.
    let mut missing_folders: Vec<_> = missing
        .iter()
        .filter(|r| r.item_type == "folder")
        .cloned()
        .collect();
    missing_folders.sort_by(|a, b| a.relative_path.len().cmp(&b.relative_path.len()));
    let mut delete_roots: Vec<String> = Vec::new();
    let mut deleted = 0u32;

    for folder in missing_folders {
        if delete_roots
            .iter()
            .any(|r| is_under_relative_prefix(&folder.relative_path, r))
        {
            continue;
        }
        {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            if my_drive_get_placeholder(&conn, &folder.relative_path)?.is_none() {
                // Already relocated as part of a parent offline move.
                continue;
            }
        }
        match probe_my_drive_remote_exists(api, "folder", &folder.remote_id).await {
            RemotePresence::Unknown => {
                // Network/API uncertainty — never soft-trash on a guess (Drive Stream).
                sync_log(format!(
                    "My Drive offline skip destructive — probe failed {}",
                    folder.relative_path
                ));
                continue;
            }
            RemotePresence::Missing => {
                // Already gone on server — clear mapping only.
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholders_under_prefix(&conn, &folder.relative_path)?;
                delete_roots.push(folder.relative_path.clone());
                deleted += 1;
                sync_log(format!(
                    "My Drive offline cleared gone folder — {}",
                    folder.relative_path
                ));
                continue;
            }
            RemotePresence::Exists => {
                // Local path gone, identity not found → treat as offline delete (Drive-like).
            }
        }
        match api
            .delete_folder_with_mutation(&folder.remote_id, None)
            .await
        {
            Ok(()) => {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholders_under_prefix(&conn, &folder.relative_path)?;
                delete_roots.push(folder.relative_path.clone());
                deleted += 1;
                let name = Path::new(&folder.relative_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("folder");
                sync_log(format!("My Drive offline deleted folder — {name}"));
            }
            Err(e) if e.is_not_found()
                || e.to_string().to_ascii_lowercase().contains("not found") =>
            {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholders_under_prefix(&conn, &folder.relative_path)?;
                delete_roots.push(folder.relative_path.clone());
                deleted += 1;
                sync_log(format!(
                    "My Drive offline cleared gone folder — {}",
                    folder.relative_path
                ));
            }
            Err(e) => sync_log(format!(
                "My Drive offline folder delete failed {}: {e}",
                folder.relative_path
            )),
        }
    }

    for file in missing.into_iter().filter(|r| r.item_type == "file") {
        if delete_roots
            .iter()
            .any(|r| is_under_relative_prefix(&file.relative_path, r))
        {
            continue;
        }
        {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            if my_drive_get_placeholder(&conn, &file.relative_path)?.is_none() {
                continue;
            }
        }
        match probe_my_drive_remote_exists(api, "file", &file.remote_id).await {
            RemotePresence::Unknown => {
                sync_log(format!(
                    "My Drive offline skip destructive — probe failed {}",
                    file.relative_path
                ));
                continue;
            }
            RemotePresence::Missing => {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholder(&conn, &file.relative_path)?;
                deleted += 1;
                sync_log(format!(
                    "My Drive offline cleared gone file — {}",
                    file.relative_path
                ));
                continue;
            }
            RemotePresence::Exists => {}
        }
        match api.delete_file(&file.remote_id).await {
            Ok(()) => {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholder(&conn, &file.relative_path)?;
                deleted += 1;
                let name = Path::new(&file.relative_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                sync_log(format!("My Drive offline deleted — {name}"));
            }
            Err(e) if e.is_not_found()
                || e.to_string().to_ascii_lowercase().contains("not found") =>
            {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_delete_placeholder(&conn, &file.relative_path)?;
                deleted += 1;
                sync_log(format!(
                    "My Drive offline cleared gone file — {}",
                    file.relative_path
                ));
            }
            Err(e) => sync_log(format!(
                "My Drive offline file delete failed {}: {e}",
                file.relative_path
            )),
        }
    }

    if moved > 0 || deleted > 0 {
        sync_log(format!(
            "My Drive offline reconcile — {moved} move(s), {deleted} delete(s)"
        ));
    }
    Ok(())
}

/// Live Explorer rename/move inside My Drive while the app is running.
pub async fn rename_my_drive_path(
    api: &ApiClient,
    db: &DbHandle,
    from: &Path,
    to: &Path,
) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let old_rel = relative_path_from_sync_root(&sync_root, from)
        .ok_or_else(|| AppError::msg("rename source outside sync root"))?;
    let new_rel = relative_path_from_sync_root(&sync_root, to)
        .ok_or_else(|| AppError::msg("rename destination outside sync root"))?;
    if !is_under_my_drive(&old_rel) || !is_under_my_drive(&new_rel) {
        return Ok(());
    }
    if old_rel.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME)
        || new_rel.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME)
    {
        return Ok(());
    }
    if old_rel.eq_ignore_ascii_case(&new_rel) {
        return Ok(());
    }

    let placeholder = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_get_placeholder(&conn, &old_rel)?
    };
    let Some((remote_id, item_type, _)) = placeholder else {
        // Untracked — next poll / CLOSE upload will pick it up.
        return Ok(());
    };
    if remote_id.is_empty() {
        return Ok(());
    }

    let name = to
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::msg("rename destination has no file name"))?
        .to_string();
    let parent_id = resolve_parent_remote_id_for_relative(db, &sync_root, &new_rel).await?;

    if item_type == "folder" {
        api.patch_folder(&remote_id, Some(&name), Some(&parent_id), None)
            .await?;
    } else {
        api.patch_file(&remote_id, Some(&name), Some(&parent_id), None)
            .await?;
    }

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    my_drive_relocate_placeholder(&conn, &old_rel, &new_rel, Some(&parent_id))?;
    sync_log(format!("My Drive renamed — {old_rel} → {new_rel}"));
    Ok(())
}

async fn poll_my_drive_folder(
    api: &ApiClient,
    db: &DbHandle,
    sync_root: &Path,
    parent_relative: &str,
    folder_id: Option<&str>,
    mirror: bool,
    download_sem: Arc<Semaphore>,
    upload_sem: Arc<Semaphore>,
    suppress: Option<&WatcherSuppress>,
    on_busy: &Option<MyDriveBusyCb>,
    stats: &mut MyDrivePollStats,
) -> AppResult<()> {
    let contents =
        fetch_folder_contents(api, db, sync_root, parent_relative, folder_id).await?;
    let local_dir = local_dir_for_relative(sync_root, parent_relative);
    let mut local_only_folders = Vec::new();
    if std::fs::create_dir_all(&local_dir).is_ok() {
        apply_remote_children(db, parent_relative, &local_dir, &contents, suppress);
        reconcile_local_against_remote(api, db, parent_relative, &local_dir, &contents, suppress)
            .await;
        refresh_files_when_remote_newer(
            api,
            db,
            parent_relative,
            &local_dir,
            &contents.files,
            mirror,
            suppress,
            stats,
        )
        .await;
        let parent_id = match folder_id {
            Some(id) => id.to_string(),
            None => resolve_my_drive_root_id(db)?,
        };
        local_only_folders = upload_local_only_children(
            api,
            db,
            parent_relative,
            &parent_id,
            &local_dir,
            &contents,
            upload_sem.clone(),
            on_busy,
            stats,
        )
        .await;
        heal_hydrated_unpinned_status(db, parent_relative, &local_dir);
        notify_directory_updated(&local_dir);
    }

    if mirror {
        mirror_files_parallel(
            api,
            db,
            &local_dir,
            &contents.files,
            download_sem.clone(),
            on_busy,
            stats,
        )
        .await;
    }

    for folder in unique_remote_folders_for_poll(db, parent_relative, &contents.folders) {
        let sub_rel = join_my_drive_relative(parent_relative, &folder.name);
        Box::pin(poll_my_drive_folder(
            api,
            db,
            sync_root,
            &sub_rel,
            Some(&folder.id),
            mirror,
            download_sem.clone(),
            upload_sem.clone(),
            suppress,
            on_busy,
            stats,
        ))
        .await?;
    }

    for (sub_rel, folder_remote_id) in local_only_folders {
        Box::pin(poll_my_drive_folder(
            api,
            db,
            sync_root,
            &sub_rel,
            Some(&folder_remote_id),
            mirror,
            download_sem.clone(),
            upload_sem.clone(),
            suppress,
            on_busy,
            stats,
        ))
        .await?;
    }

    Ok(())
}

/// Clear leftover Explorer UNPINNED / reconvert demoted hydrated files (stuck sync arrows).
fn heal_hydrated_unpinned_status(db: &DbHandle, parent_relative: &str, local_dir: &Path) {
    if is_free_up_in_progress() {
        return;
    }
    if is_unpinned(local_dir) && !is_path_under_active_free_up(local_dir) {
        refresh_placeholder_status(local_dir);
        sync_log(format!(
            "My Drive heal UNPINNED folder — {}",
            local_dir.display()
        ));
    }
    let Ok(entries) = std::fs::read_dir(local_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if crate::sync::should_skip_file(&name) {
            continue;
        }
        if is_dehydrated_placeholder(&path) {
            continue;
        }
        if is_path_under_active_free_up(&path) {
            continue;
        }
        let child_relative = join_my_drive_relative(parent_relative, &name);
        let remote_id = {
            let Ok(conn) = db.lock() else {
                continue;
            };
            my_drive_get_placeholder(&conn, &child_relative)
                .ok()
                .flatten()
                .filter(|(_, ty, _)| ty == "file")
                .map(|(id, _, _)| id)
        };
        let Some(remote_id) = remote_id else {
            continue;
        };
        // fs::copy demotion: plain local file still PINNED / no In-Sync → arrows.
        if !is_cloud_placeholder(&path) {
            finalize_hydrated_file(&path, &remote_id);
            sync_log(format!(
                "My Drive heal demoted hydrate — {}",
                path.display()
            ));
            continue;
        }
        if is_unpinned(&path) {
            refresh_placeholder_status(&path);
            sync_log(format!(
                "My Drive heal UNPINNED status — {}",
                path.display()
            ));
        }
    }
}

/// Upload local-only files and register local-only folders under one My Drive directory.
/// Returns newly registered folders `(relative, remote_id)` for recursion in the same poll.
async fn upload_local_only_children(
    api: &ApiClient,
    db: &DbHandle,
    parent_relative: &str,
    parent_folder_id: &str,
    local_dir: &Path,
    contents: &crate::api::types::FolderContents,
    upload_sem: Arc<Semaphore>,
    on_busy: &Option<MyDriveBusyCb>,
    stats: &mut MyDrivePollStats,
) -> Vec<(String, String)> {
    let mut remote_names: HashSet<String> = HashSet::new();
    for folder in &contents.folders {
        remote_names.insert(sanitize_name(&folder.name).to_ascii_lowercase());
    }
    for file in &contents.files {
        remote_names.insert(sanitize_name(&file.name).to_ascii_lowercase());
    }

    let entries = match std::fs::read_dir(local_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut folders_to_register: Vec<(String, PathBuf, String)> = Vec::new();
    let mut files_to_upload: Vec<PathBuf> = Vec::new();
    let mut items_to_relocate: Vec<(String, PathBuf, String, String, String)> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if remote_names.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        let child_relative = join_my_drive_relative(parent_relative, &name);
        let path = entry.path();
        let tracked = {
            let Ok(conn) = db.lock() else {
                continue;
            };
            my_drive_get_placeholder(&conn, &child_relative)
                .ok()
                .flatten()
                .is_some()
        };
        if tracked {
            continue;
        }
        if path.is_dir() {
            if let Some((ty, remote_id)) = read_placeholder_identity(&path) {
                if ty == "folder" && !remote_id.is_empty() {
                    items_to_relocate.push((
                        name,
                        path,
                        child_relative,
                        remote_id,
                        "folder".to_string(),
                    ));
                    continue;
                }
            }
            if tree_has_cloud_or_reparse(&path) {
                if let Some(folder_id) = infer_folder_remote_id_from_disk_children(&path, db) {
                    items_to_relocate.push((
                        name,
                        path,
                        child_relative,
                        folder_id,
                        "folder".to_string(),
                    ));
                } else {
                    // Placeholder tree without recoverable folder id — never create empty remote.
                    sync_log(format!(
                        "My Drive local-scan skip empty create (cloud children) — {}",
                        child_relative
                    ));
                }
                continue;
            }
            // Drive Stream parity: never invent empty remote folders from leftover dirs.
            // Watcher/ensure still creates empty folders when the user makes them live.
            if !tree_has_uploadable_file(&path) {
                sync_log(format!(
                    "My Drive local-scan skip empty create — {}",
                    child_relative
                ));
                continue;
            }
            folders_to_register.push((name, path, child_relative));
        } else if path.is_file() {
            if crate::sync::should_skip_file(&name) {
                continue;
            }
            if let Some((ty, remote_id)) = read_placeholder_identity(&path) {
                if ty == "file" && !remote_id.is_empty() {
                    items_to_relocate.push((
                        name,
                        path,
                        child_relative,
                        remote_id,
                        "file".to_string(),
                    ));
                    continue;
                }
            }
            // Skip orphan CfAPI dehydrate placeholders (reparse) without a DB row / identity.
            if path_has_reparse_point(&path) {
                continue;
            }
            files_to_upload.push(path);
        }
    }

    if !folders_to_register.is_empty()
        || !files_to_upload.is_empty()
        || !items_to_relocate.is_empty()
    {
        notify_my_drive_busy(on_busy);
    }

    let sync_root = match sync_root_dir(false) {
        Ok(p) => p,
        Err(_) => local_dir
            .ancestors()
            .nth(parent_relative.matches('\\').count() + parent_relative.matches('/').count())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| local_dir.to_path_buf()),
    };

    let mut registered_folders = Vec::new();
    for (name, path, child_relative, remote_id, item_type) in items_to_relocate {
        match relocate_my_drive_item_from_scan(
            api,
            db,
            &sync_root,
            &path,
            &child_relative,
            &name,
            &remote_id,
            &item_type,
            parent_folder_id,
        )
        .await
        {
            Ok(()) => {
                sync_log(format!(
                    "My Drive local-scan relocate — {} ({})",
                    child_relative, item_type
                ));
                if item_type == "folder" {
                    stats.folders_created += 1;
                    registered_folders.push((child_relative, remote_id));
                } else {
                    stats.files_uploaded += 1;
                }
            }
            Err(e) => {
                // Dead remote id after server wipe — clear mapping, never create empty replacement.
                if e.is_not_found()
                    || e.to_string().to_ascii_lowercase().contains("not found")
                {
                    if let Ok(conn) = db.lock() {
                        if let Ok(Some((old_rel, _, _))) =
                            my_drive_get_placeholder_by_remote_id(&conn, &remote_id)
                        {
                            let _ = my_drive_delete_placeholders_under_prefix(&conn, &old_rel);
                        }
                    }
                    sync_log(format!(
                        "My Drive local-scan dead remote id cleared — {} ({})",
                        child_relative, remote_id
                    ));
                } else {
                    stats.errors += 1;
                    sync_log(format!(
                        "My Drive local-scan relocate failed {}: {}",
                        child_relative, e
                    ));
                }
            }
        }
    }

    for (name, path, child_relative) in folders_to_register {
        match api
            .create_or_resolve_folder(&name, api_folder_parent_id(parent_folder_id))
            .await
        {
            Ok(folder) => {
                if let Ok(conn) = db.lock() {
                    let _ = my_drive_upsert_placeholder(
                        &conn,
                        &child_relative,
                        &folder.id,
                        "folder",
                        api_folder_parent_id(parent_folder_id),
                        None,
                    );
                }
                if let Err(e) = ensure_cloud_placeholder(&path, "folder", &folder.id) {
                    sync_log(format!(
                        "My Drive local-scan folder placeholder {}: {}",
                        path.display(),
                        e
                    ));
                }
                sync_log(format!(
                    "My Drive folder created (local scan) — {}",
                    child_relative
                ));
                stats.folders_created += 1;
                registered_folders.push((child_relative, folder.id));
            }
            Err(e) => {
                stats.errors += 1;
                sync_log(format!(
                    "My Drive local-scan folder failed {}\\{}: {}",
                    parent_relative, name, e
                ));
            }
        }
    }

    let uploaded = Arc::new(AtomicU32::new(0));
    let upload_errors = Arc::new(AtomicU32::new(0));
    let mut join_set = JoinSet::new();
    for path in files_to_upload {
        let Some(claim) = try_claim_my_drive_upload(&path) else {
            sync_log(format!(
                "My Drive local-scan upload skipped (already in flight) — {}",
                path.display()
            ));
            continue;
        };
        while join_set.len() >= UPLOAD_CONCURRENCY {
            if let Some(res) = join_set.join_next().await {
                if let Err(e) = res {
                    upload_errors.fetch_add(1, Ordering::Relaxed);
                    sync_log(format!("My Drive local-scan upload join error — {}", e));
                }
            }
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        sync_log(format!(
            "my drive upload waiting for permit — {} ({} bytes)",
            path.display(),
            size
        ));
        let permit = match upload_sem.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => break,
        };
        let api = api.clone();
        let db = db.clone();
        let uploaded = Arc::clone(&uploaded);
        let upload_errors = Arc::clone(&upload_errors);
        join_set.spawn(async move {
            let _claim = claim;
            let _permit = permit;
            sync_log(format!(
                "my drive upload started — {} ({} bytes)",
                path.display(),
                size
            ));
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            match upload_my_drive_path(&api, &db, &path).await {
                Ok(true) => {
                    uploaded.fetch_add(1, Ordering::Relaxed);
                    sync_log(format!("My Drive uploaded (local scan) — {}", name));
                }
                Ok(false) => {}
                Err(e) => {
                    upload_errors.fetch_add(1, Ordering::Relaxed);
                    sync_log(format!(
                        "My Drive local-scan upload failed {}: {}",
                        path.display(),
                        e
                    ));
                }
            }
        });
    }
    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            upload_errors.fetch_add(1, Ordering::Relaxed);
            sync_log(format!("My Drive local-scan upload join error — {}", e));
        }
    }
    stats.files_uploaded += uploaded.load(Ordering::Relaxed);
    stats.errors += upload_errors.load(Ordering::Relaxed);

    registered_folders
}

fn path_has_reparse_point(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        std::fs::metadata(path)
            .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// When the server still has duplicate same-name folders under one parent, poll only one.
fn unique_remote_folders_for_poll<'a>(
    db: &DbHandle,
    parent_relative: &str,
    folders: &'a [crate::api::types::Folder],
) -> Vec<&'a crate::api::types::Folder> {
    let mut by_name: HashMap<String, &crate::api::types::Folder> = HashMap::new();
    let mut skipped = 0u32;
    for folder in folders {
        let key = sanitize_name(&folder.name).to_ascii_lowercase();
        let child_rel = join_my_drive_relative(parent_relative, &sanitize_name(&folder.name));
        let mapped_id = db
            .lock()
            .ok()
            .and_then(|conn| my_drive_get_placeholder(&conn, &child_rel).ok().flatten())
            .map(|(id, _, _)| id);
        match by_name.get(&key) {
            None => {
                by_name.insert(key, folder);
            }
            Some(existing) => {
                skipped += 1;
                if mapped_id.as_deref() == Some(folder.id.as_str())
                    && mapped_id.as_deref() != Some(existing.id.as_str())
                {
                    by_name.insert(key, folder);
                }
            }
        }
    }
    if skipped > 0 {
        sync_log(format!(
            "My Drive poll skipped {} duplicate same-name folder(s) under {}",
            skipped, parent_relative
        ));
    }
    by_name.into_values().collect()
}

/// Create missing local placeholders for remote children (e.g. after Trash→Restore).
/// Re-enables on-demand FETCH when the folder was previously marked fully populated empty.
fn apply_remote_children(
    db: &DbHandle,
    parent_relative: &str,
    local_dir: &Path,
    contents: &crate::api::types::FolderContents,
    suppress: Option<&WatcherSuppress>,
) {
    let (missing_folders, missing_files) = missing_remote_children(local_dir, contents);
    let had_missing = !missing_folders.is_empty() || !missing_files.is_empty();

    if had_missing {
        if let Err(e) = mark_directory_partially_populated(local_dir) {
            sync_log(format!(
                "My Drive enable on-demand failed {}: {}",
                local_dir.display(),
                e
            ));
        } else {
            sync_log(format!(
                "My Drive re-enabled on-demand population — {}",
                parent_relative
            ));
        }
    }

    let mut created = 0u32;
    let mut skipped = 0u32;

    for folder in &contents.folders {
        let name = sanitize_name(&folder.name);
        let folder_path = local_dir.join(&name);
        match create_named_folder_placeholder(local_dir, &name, &folder.id) {
            Ok(()) => created += 1,
            Err(e)
                if is_duplicate_placeholder_error(&e)
                    || e.to_string().contains("0x8007017C")
                    || folder_path.is_dir() =>
            {
                skipped += 1;
                ensure_or_replace_folder_placeholder(
                    local_dir,
                    &folder_path,
                    &name,
                    &folder.id,
                    suppress,
                );
            }
            Err(e) => {
                sync_log(format!(
                    "My Drive folder placeholder failed {}\\{}: {}",
                    parent_relative, name, e
                ));
            }
        }
    }

    // Ensure parent is a cloud placeholder before creating file children.
    if let Ok(conn) = db.lock() {
        if let Ok(Some((remote_id, _, _))) = my_drive_get_placeholder(&conn, parent_relative) {
            if let Err(e) = ensure_cloud_placeholder(local_dir, "folder", &remote_id) {
                sync_log(format!(
                    "My Drive ensure parent cloud placeholder {}: {}",
                    local_dir.display(),
                    e
                ));
            }
        }
    }

    for file in &contents.files {
        match create_file_placeholder(local_dir, file) {
            Ok(()) => created += 1,
            Err(e) if is_duplicate_placeholder_error(&e) => skipped += 1,
            Err(e) => {
                sync_log(format!(
                    "My Drive file placeholder failed {}\\{}: {}",
                    parent_relative, file.name, e
                ));
            }
        }
    }

    if created > 0 || skipped > 0 {
        sync_log(format!(
            "My Drive placeholders under {} — created={} skipped={}",
            parent_relative, created, skipped
        ));
    }

    if had_missing {
        if let Ok(conn) = db.lock() {
            for folder in &missing_folders {
                let _ = insert_activity(&conn, &folder.name, "Restored from cloud", 0, "synced");
            }
            for file in &missing_files {
                let _ = insert_activity(
                    &conn,
                    &file.name,
                    "Restored from cloud",
                    file.size,
                    "synced",
                );
            }
        }
        for folder in &missing_folders {
            sync_log(format!(
                "My Drive restored folder — {}\\{}",
                parent_relative, folder.name
            ));
        }
        for file in &missing_files {
            sync_log(format!(
                "My Drive restored file — {}\\{}",
                parent_relative, file.name
            ));
        }
    }
}

/// When a leftover plain directory blocks CfCreatePlaceholders, convert it to a
/// cloud placeholder — or replace an empty leftover if convert fails.
fn ensure_or_replace_folder_placeholder(
    parent_dir: &Path,
    folder_path: &Path,
    name: &str,
    remote_id: &str,
    suppress: Option<&WatcherSuppress>,
) {
    if let Err(e) = ensure_cloud_placeholder(folder_path, "folder", remote_id) {
        sync_log(format!(
            "My Drive ensure_cloud_placeholder {}: {}",
            folder_path.display(),
            e
        ));
    }

    // Already a usable cloud folder?
    if mark_directory_partially_populated(folder_path).is_ok() {
        return;
    }

    let is_empty = folder_path.is_dir()
        && std::fs::read_dir(folder_path)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
    if !is_empty {
        sync_log(format!(
            "My Drive leftover folder not cloud and not empty — {}",
            folder_path.display()
        ));
        return;
    }

    let remove_ok = if let Some(suppress) = suppress {
        suppress.run_suppressed(folder_path, || std::fs::remove_dir_all(folder_path).is_ok())
    } else {
        std::fs::remove_dir_all(folder_path).is_ok()
    };
    if !remove_ok {
        sync_log(format!(
            "My Drive failed to remove leftover folder — {}",
            folder_path.display()
        ));
        return;
    }
    match create_named_folder_placeholder(parent_dir, name, remote_id) {
        Ok(()) => sync_log(format!(
            "My Drive replaced leftover folder — {}",
            folder_path.display()
        )),
        Err(e) => sync_log(format!(
            "My Drive recreate folder after replace failed {}: {}",
            folder_path.display(),
            e
        )),
    }
}

fn missing_remote_children(
    local_dir: &Path,
    contents: &crate::api::types::FolderContents,
) -> (Vec<crate::api::types::Folder>, Vec<crate::api::types::FileRecord>) {
    let mut missing_folders = Vec::new();
    let mut missing_files = Vec::new();
    for folder in &contents.folders {
        let name = sanitize_name(&folder.name);
        if !local_dir.join(&name).exists() {
            missing_folders.push(folder.clone());
        }
    }
    for file in &contents.files {
        let name = sanitize_name(&file.name);
        if !local_dir.join(&name).exists() {
            missing_files.push(file.clone());
        }
    }
    (missing_folders, missing_files)
}

/// Remove local My Drive placeholders that are no longer in the remote listing
/// (e.g. soft-deleted from mobile). Drive Stream parity: missing from a parent
/// listing is not enough — probe remote ID; only remove local when the object
/// is confirmed gone (404). Network errors skip destructive work.
/// Orphan CfAPI placeholders with FileIdentity are **relinked** (0.1.74+).
async fn reconcile_local_against_remote(
    api: &ApiClient,
    db: &DbHandle,
    parent_relative: &str,
    local_dir: &Path,
    contents: &crate::api::types::FolderContents,
    suppress: Option<&WatcherSuppress>,
) {
    let mut remote_names: HashSet<String> = HashSet::new();
    for folder in &contents.folders {
        remote_names.insert(sanitize_name(&folder.name).to_ascii_lowercase());
    }
    for file in &contents.files {
        remote_names.insert(sanitize_name(&file.name).to_ascii_lowercase());
    }

    let parent_remote_id = {
        let Ok(conn) = db.lock() else {
            return;
        };
        my_drive_get_placeholder(&conn, parent_relative)
            .ok()
            .flatten()
            .and_then(|(id, ty, _)| {
                if ty == "folder" && !id.is_empty() {
                    Some(id)
                } else {
                    None
                }
            })
    };

    let entries = match std::fs::read_dir(local_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if remote_names.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        let child_relative = join_my_drive_relative(parent_relative, &name);
        let path = entry.path();
        let tracked = {
            let Ok(conn) = db.lock() else {
                continue;
            };
            my_drive_get_placeholder(&conn, &child_relative)
                .ok()
                .flatten()
        };
        let Some((remote_id, item_type, _)) = tracked else {
            // Relink orphan FileIdentity → DB; do not delete cloud placeholders.
            if let Some((ty, remote_id)) = read_placeholder_identity(&path) {
                if !remote_id.is_empty() {
                    let item_type = if ty == "folder" { "folder" } else { "file" };
                    let parent_opt = parent_remote_id.as_deref();
                    if let Ok(conn) = db.lock() {
                        if let Ok(Some((old_rel, _, _))) =
                            my_drive_get_placeholder_by_remote_id(&conn, &remote_id)
                        {
                            if !old_rel.eq_ignore_ascii_case(&child_relative) {
                                let _ = my_drive_relocate_placeholder(
                                    &conn,
                                    &old_rel,
                                    &child_relative,
                                    parent_opt,
                                );
                            } else {
                                let _ = my_drive_upsert_placeholder(
                                    &conn,
                                    &child_relative,
                                    &remote_id,
                                    item_type,
                                    parent_opt,
                                    None,
                                );
                            }
                        } else {
                            let _ = my_drive_upsert_placeholder(
                                &conn,
                                &child_relative,
                                &remote_id,
                                item_type,
                                parent_opt,
                                None,
                            );
                        }
                    }
                    sync_log(format!(
                        "My Drive reconcile relinked orphan — {}",
                        child_relative
                    ));
                }
            }
            continue;
        };

        // Tracked but missing from parent listing — confirm remote ID before wipe.
        match probe_my_drive_remote_exists(api, &item_type, &remote_id).await {
            RemotePresence::Exists => {
                sync_log(format!(
                    "My Drive reconcile keep local — still on server {}",
                    child_relative
                ));
                continue;
            }
            RemotePresence::Unknown => {
                sync_log(format!(
                    "My Drive reconcile skip destructive — probe failed {}",
                    child_relative
                ));
                continue;
            }
            RemotePresence::Missing => {}
        }

        if let Ok(conn) = db.lock() {
            let _ = my_drive_delete_placeholders_under_prefix(&conn, &child_relative);
        }
        let remove_ok = if let Some(suppress) = suppress {
            suppress.run_suppressed(&path, || {
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).is_ok()
                } else {
                    std::fs::remove_file(&path).is_ok()
                }
            })
        } else if path.is_dir() {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        if remove_ok {
            sync_log(format!("My Drive reconcile removed — {}", child_relative));
        } else {
            sync_log(format!(
                "My Drive reconcile failed to remove disk path — {}",
                child_relative
            ));
        }
    }
}

async fn mirror_files_parallel(
    api: &ApiClient,
    db: &DbHandle,
    local_dir: &Path,
    files: &[crate::api::types::FileRecord],
    download_sem: Arc<Semaphore>,
    on_busy: &Option<MyDriveBusyCb>,
    stats: &mut MyDrivePollStats,
) {
    let needs_any = files.iter().any(|file| {
        let local_path = local_dir.join(sanitize_name(&file.name));
        match std::fs::metadata(&local_path) {
            Ok(meta) => meta.len() < file.size.max(0) as u64,
            Err(_) => true,
        }
    });
    if needs_any {
        notify_my_drive_busy(on_busy);
    }

    let mirrored = Arc::new(AtomicU32::new(0));
    let mirror_errors = Arc::new(AtomicU32::new(0));
    let mut join_set = JoinSet::new();

    for file in files {
        while join_set.len() >= DOWNLOAD_CONCURRENCY {
            if let Some(res) = join_set.join_next().await {
                if let Err(e) = res {
                    mirror_errors.fetch_add(1, Ordering::Relaxed);
                    sync_log(format!("mirror task join error — {}", e));
                }
            }
        }

        let permit = match download_sem.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => break,
        };
        let api = api.clone();
        let db = db.clone();
        let local_dir = local_dir.to_path_buf();
        let file = file.clone();
        let mirrored = Arc::clone(&mirrored);
        let mirror_errors = Arc::clone(&mirror_errors);

        join_set.spawn(async move {
            let _permit = permit;
            match mirror_file_if_needed(&api, &db, &local_dir, &file).await {
                Ok(true) => {
                    mirrored.fetch_add(1, Ordering::Relaxed);
                }
                Ok(false) => {}
                Err(e) => {
                    mirror_errors.fetch_add(1, Ordering::Relaxed);
                    sync_log(format!("mirror {} failed: {}", file.name, e));
                }
            }
        });
    }

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res {
            mirror_errors.fetch_add(1, Ordering::Relaxed);
            sync_log(format!("mirror task join error — {}", e));
        }
    }
    stats.files_mirrored += mirrored.load(Ordering::Relaxed);
    stats.errors += mirror_errors.load(Ordering::Relaxed);
}

/// Returns `true` when plaintext was copied into the local My Drive path.
async fn mirror_file_if_needed(
    api: &ApiClient,
    db: &DbHandle,
    local_dir: &Path,
    file: &crate::api::types::FileRecord,
) -> AppResult<bool> {
    let local_path = local_dir.join(sanitize_name(&file.name));
    let expected = file.size.max(0) as u64;
    let known_version = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        crate::db::my_drive_known_remote_version(&conn, &file.id).unwrap_or(0)
    };
    let size_mismatch = match std::fs::metadata(&local_path) {
        Ok(meta) => meta.len() != expected,
        Err(_) => true,
    };
    let version_newer = file.version > known_version;
    if !size_mismatch && !version_newer {
        return Ok(false);
    }
    let cached = ensure_hydrated_plaintext(api, db, &file.id).await?;
    if let Some(parent) = local_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::copy(&cached, &local_path)?;
    set_path_mtime_from_remote(&local_path, &file.updated_at);
    if let Ok(conn) = db.lock() {
        if let Ok(Some((rel, _, parent))) =
            crate::db::my_drive_get_placeholder_by_remote_id(&conn, &file.id)
        {
            let _ = my_drive_upsert_placeholder(
                &conn,
                &rel,
                &file.id,
                "file",
                parent.as_deref(),
                Some(file.version),
            );
        }
    }
    Ok(true)
}

pub async fn upload_my_drive_path(api: &ApiClient, db: &DbHandle, path: &Path) -> AppResult<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let sync_root = sync_root_dir(false)?;
    let relative = relative_path_from_sync_root(&sync_root, path)
        .ok_or_else(|| AppError::msg("path outside sync root"))?;
    if !is_under_my_drive(&relative) {
        return Ok(false);
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    if crate::sync::should_skip_file(&file_name) {
        return Ok(false);
    }

    let existing_remote = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_get_placeholder(&conn, &relative)?
            .filter(|(_, item_type, _)| item_type == "file")
            .map(|(id, _, known_ver)| (id, known_ver))
    };

    if let Some((remote_id, known_ver)) = existing_remote {
        // Google Drive style: remote version is source of truth — never overwrite a newer restore/edit.
        match api.get_file(&remote_id).await {
            Ok(remote) if remote.version > known_ver => {
                sync_log(format!(
                    "My Drive skip upload (remote newer v{} > known v{}) — {}",
                    remote.version, known_ver, file_name
                ));
                pull_remote_file_over_local(api, db, path, &relative, &remote, None).await?;
                return Ok(false);
            }
            Ok(_) => {}
            Err(e) => {
                return Err(AppError::msg(format!(
                    "My Drive skip upload (could not verify remote version for {}): {}",
                    file_name, e
                )));
            }
        }

        // Open / thumbnail must not create a version: skip when local bytes match last hydrate.
        if let Ok(local_hash) = crate::my_drive::hash_local_file(path) {
            let known_hash = {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                crate::db::my_drive_known_content_hash(&conn, &remote_id).unwrap_or_default()
            };
            if !known_hash.is_empty() && known_hash == local_hash {
                sync_log(format!(
                    "My Drive skip upload (unchanged hash) — {}",
                    file_name
                ));
                maybe_finalize_stream_after_upload(db, path, &remote_id);
                return Ok(false);
            }
        }

        let existing_key = {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            get_file_key(&conn, &remote_id)?
                .and_then(|k| crate::crypto::key_from_b64url(&k).ok())
        };
        let (rec, key) = api
            .update_file_content(&remote_id, path, &file_name, existing_key, None)
            .await?;
        let local_hash = crate::my_drive::hash_local_file(path).unwrap_or_default();
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        store_file_key(&conn, &rec.id, &key_to_b64url(&key))?;
        my_drive_upsert_placeholder(&conn, &relative, &rec.id, "file", None, Some(rec.version))?;
        if !local_hash.is_empty() {
            let _ = crate::db::my_drive_set_content_hash(&conn, &rec.id, &local_hash);
        }
        if rec.version <= known_ver {
            sync_log(format!(
                "My Drive skip upload (content unchanged) — {}",
                file_name
            ));
            maybe_finalize_stream_after_upload(db, path, &remote_id);
            return Ok(false);
        }
        sync_log(format!("My Drive updated — {}", file_name));
        maybe_finalize_stream_after_upload(db, path, &rec.id);
        return Ok(true);
    }

    let parent_folder_id = ensure_my_drive_parent_folder(api, db, &relative).await?;
    let api_parent = api_folder_parent_id(&parent_folder_id);
    let (rec, key) = api
        .upload_file(db, path, &file_name, api_parent, None)
        .await?;
    let local_hash = crate::my_drive::hash_local_file(path).unwrap_or_default();
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    store_file_key(&conn, &rec.id, &key_to_b64url(&key))?;
    my_drive_upsert_placeholder(&conn, &relative, &rec.id, "file", api_parent, Some(rec.version))?;
    if !local_hash.is_empty() {
        let _ = crate::db::my_drive_set_content_hash(&conn, &rec.id, &local_hash);
    }
    sync_log(format!("My Drive uploaded — {}", file_name));
    drop(conn);
    maybe_finalize_stream_after_upload(db, path, &rec.id);
    Ok(true)
}

/// Stream: convert+In-Sync after upload (keep local bytes — Free up dehydrates).
fn maybe_finalize_stream_after_upload(db: &DbHandle, path: &Path, remote_id: &str) {
    if !crate::sync::engine::sync_mode_is_stream(db) {
        return;
    }
    if is_path_under_active_free_up(path) {
        return;
    }
    match finalize_stream_placeholder(path, remote_id) {
        Ok(()) => {}
        Err(e) => sync_log(format!(
            "My Drive finalize stream placeholder skipped {}: {}",
            path.display(),
            e
        )),
    }
}

pub async fn delete_my_drive_path(api: &ApiClient, db: &DbHandle, path: &Path) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let relative = relative_path_from_sync_root(&sync_root, path)
        .ok_or_else(|| AppError::msg("path outside sync root"))?;
    if !is_under_my_drive(&relative) {
        return Ok(());
    }
    // Never soft-delete the My Drive root itself.
    if relative.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) {
        return Ok(());
    }

    // Another task already soft-trashed an ancestor folder covering this path.
    if is_path_under_active_delete_ancestor(path) {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        let _ = my_drive_delete_placeholder(&conn, &relative);
        return Ok(());
    }

    let _permit = my_drive_delete_semaphore()
        .acquire()
        .await
        .map_err(|e| AppError::msg(format!("delete semaphore closed: {e}")))?;

    // Re-check after waiting — a coalesced folder delete may have finished.
    if is_path_under_active_delete_ancestor(path) {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        let _ = my_drive_delete_placeholder(&conn, &relative);
        return Ok(());
    }
    {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        if my_drive_get_placeholder(&conn, &relative)?.is_none() {
            return Ok(());
        }
    }

    // Explorer often deletes children first; if a tracked ancestor folder is already
    // gone from disk, soft-trash that folder once instead of N file DELETEs.
    if let Some((folder_path, folder_rel, folder_remote_id, folder_name)) =
        highest_missing_folder_ancestor(db, &sync_root, &relative)?
    {
        let Some(_claim) = try_claim_folder_delete(&folder_remote_id) else {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            let _ = my_drive_delete_placeholder(&conn, &relative);
            return Ok(());
        };
        mark_delete_in_flight(&folder_path);
        let result =
            soft_trash_my_drive_folder(api, db, &folder_rel, &folder_remote_id, &folder_name).await;
        clear_delete_in_flight(&folder_path);
        return result;
    }

    let placeholder = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_get_placeholder(&conn, &relative)?
    };
    let Some((remote_id, item_type, _)) = placeholder else {
        return Ok(());
    };

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(if item_type == "folder" {
            "folder"
        } else {
            "file"
        })
        .to_string();

    if item_type == "folder" {
        let Some(_claim) = try_claim_folder_delete(&remote_id) else {
            return Ok(());
        };
        return soft_trash_my_drive_folder(api, db, &relative, &remote_id, &name).await;
    }

    if item_type == "file" {
        if !remote_id.is_empty() {
            api.delete_file(&remote_id).await?;
        }
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_delete_placeholder(&conn, &relative)?;
        sync_log(format!("My Drive deleted — {name}"));
    }
    Ok(())
}

async fn soft_trash_my_drive_folder(
    api: &ApiClient,
    db: &DbHandle,
    relative: &str,
    remote_id: &str,
    name: &str,
) -> AppResult<()> {
    if !remote_id.is_empty() {
        api.delete_folder_with_mutation(remote_id, None).await?;
    }
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    my_drive_delete_placeholders_under_prefix(&conn, relative)?;
    sync_log(format!("My Drive deleted folder — {name}"));
    Ok(())
}

/// Highest tracked folder ancestor of `relative` whose local path no longer exists.
fn highest_missing_folder_ancestor(
    db: &DbHandle,
    sync_root: &Path,
    relative: &str,
) -> AppResult<Option<(PathBuf, String, String, String)>> {
    let mut best: Option<(PathBuf, String, String, String)> = None;
    let mut cursor = Path::new(relative)
        .parent()
        .map(|p| p.to_string_lossy().replace('/', "\\"));
    while let Some(parent_rel) = cursor {
        if parent_rel.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) || parent_rel.is_empty() {
            break;
        }
        let local = local_dir_for_relative(sync_root, &parent_rel);
        let row = {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            my_drive_get_placeholder(&conn, &parent_rel)?
        };
        if let Some((remote_id, item_type, _)) = row {
            if item_type == "folder" && !remote_id.is_empty() && !local.is_dir() {
                let name = Path::new(&parent_rel)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("folder")
                    .to_string();
                best = Some((local, parent_rel.clone(), remote_id, name));
            }
        }
        cursor = Path::new(&parent_rel)
            .parent()
            .map(|p| p.to_string_lossy().replace('/', "\\"));
    }
    Ok(best)
}

/// Drop a local placeholder row without calling the server (child under folder delete).
pub fn forget_my_drive_placeholder(db: &DbHandle, path: &Path) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let Some(relative) = relative_path_from_sync_root(&sync_root, path) else {
        return Ok(());
    };
    if !is_under_my_drive(&relative) || relative.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) {
        return Ok(());
    }
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    my_drive_delete_placeholder(&conn, &relative)?;
    Ok(())
}

async fn ensure_my_drive_parent_folder(
    api: &ApiClient,
    db: &DbHandle,
    file_relative: &str,
) -> AppResult<String> {
    let parent_relative = Path::new(file_relative)
        .parent()
        .map(|p| p.to_string_lossy().replace('/', "\\"))
        .unwrap_or_else(|| MY_DRIVE_FOLDER_NAME.to_string());
    ensure_my_drive_folder_relative(api, db, &parent_relative).await
}

/// Ensure a My Drive folder (and its parents) exist on the server; return remote id.
pub async fn ensure_my_drive_folder_relative(
    api: &ApiClient,
    db: &DbHandle,
    folder_relative: &str,
) -> AppResult<String> {
    let folder_relative = folder_relative.replace('/', "\\");
    let lock = folder_ensure_lock(&folder_relative);
    let _guard = lock.lock().await;
    ensure_my_drive_folder_relative_locked(api, db, &folder_relative).await
}

async fn ensure_my_drive_folder_relative_locked(
    api: &ApiClient,
    db: &DbHandle,
    folder_relative: &str,
) -> AppResult<String> {
    if folder_relative.eq_ignore_ascii_case(MY_DRIVE_FOLDER_NAME) {
        return resolve_my_drive_root_id(db);
    }

    if let Ok(conn) = db.lock() {
        if let Some((remote_id, item_type, _)) = my_drive_get_placeholder(&conn, folder_relative)? {
            if item_type == "folder" {
                let n = my_drive_reparent_direct_children(&conn, folder_relative, &remote_id)?;
                if n > 0 {
                    sync_log(format!(
                        "My Drive reparented {} child mapping(s) under {}",
                        n, folder_relative
                    ));
                }
                return Ok(remote_id);
            }
        }
    }

    let root_id = resolve_my_drive_root_id(db)?;
    let suffix = folder_relative
        .strip_prefix("My Drive\\")
        .or_else(|| folder_relative.strip_prefix("My Drive/"))
        .unwrap_or("");
    let mut current_parent = root_id;
    let mut built_relative = MY_DRIVE_FOLDER_NAME.to_string();

    for component in Path::new(suffix).components() {
        let std::path::Component::Normal(name) = component else {
            continue;
        };
        let part = name.to_string_lossy();
        built_relative = format!("{}\\{}", built_relative, part);
        if let Ok(conn) = db.lock() {
            if let Some((remote_id, item_type, _)) = my_drive_get_placeholder(&conn, &built_relative)? {
                if item_type == "folder" {
                    current_parent = remote_id;
                    continue;
                }
            }
        }
        let folder = api
            .create_or_resolve_folder(&part, api_folder_parent_id(&current_parent))
            .await?;
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_upsert_placeholder(
            &conn,
            &built_relative,
            &folder.id,
            "folder",
            api_folder_parent_id(&current_parent),
            None,
        )?;
        let n = my_drive_reparent_direct_children(&conn, &built_relative, &folder.id)?;
        if n > 0 {
            sync_log(format!(
                "My Drive reparented {} child mapping(s) under {}",
                n, built_relative
            ));
        }
        current_parent = folder.id;
    }

    Ok(current_parent)
}

/// Create/resolve a local My Drive folder path on the server and mark it as a cloud folder.
pub async fn ensure_my_drive_folder_path(
    api: &ApiClient,
    db: &DbHandle,
    path: &Path,
) -> AppResult<String> {
    let sync_root = sync_root_dir(false)?;
    let relative = relative_path_from_sync_root(&sync_root, path)
        .ok_or_else(|| AppError::msg("path outside sync root"))?;
    if !is_under_my_drive(&relative) {
        return Err(AppError::msg("path not under My Drive"));
    }
    let remote_id = ensure_my_drive_folder_relative(api, db, &relative).await?;
    if let Err(e) = ensure_cloud_placeholder(path, "folder", &remote_id) {
        sync_log(format!(
            "My Drive ensure folder cloud placeholder {}: {}",
            path.display(),
            e
        ));
    }
    sync_log(format!("My Drive folder ensured — {}", relative));
    Ok(remote_id)
}

/// Download (hydrate) a My Drive file or folder for offline/local use (Stream “Available offline”).
pub async fn hydrate_my_drive_path(api: &ApiClient, db: &DbHandle, path: &Path) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let relative = relative_path_from_sync_root(&sync_root, path)
        .ok_or_else(|| AppError::msg("path outside sync root"))?;
    if !is_under_my_drive(&relative) {
        return Err(AppError::msg("path not under My Drive"));
    }

    if path.is_dir() {
        hydrate_my_drive_folder(api, db, path).await?;
        sync_log(format!("My Drive hydrated folder — {}", relative));
        return Ok(());
    }
    if path.is_file() {
        if hydrate_my_drive_file(api, db, path, &relative).await? {
            sync_log(format!("My Drive hydrated file — {}", relative));
        }
        return Ok(());
    }
    Err(AppError::msg("path is not a file or folder"))
}

/// Returns `true` when content was fetched/copied onto `path`.
async fn hydrate_my_drive_file(
    api: &ApiClient,
    db: &DbHandle,
    path: &Path,
    relative: &str,
) -> AppResult<bool> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    if crate::sync::should_skip_file(file_name) {
        return Ok(false);
    }
    // Already on disk — do not re-copy from hydrate_cache (watcher feedback loop).
    // Demoted plain files (after prior fs::copy) still need reconvert + In-Sync.
    if !is_dehydrated_placeholder(path) {
        let remote_id = {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            my_drive_get_placeholder(&conn, relative)?
                .filter(|(_, ty, _)| ty == "file")
                .map(|(id, _, _)| id)
        };
        if let Some(remote_id) = remote_id {
            if !is_cloud_placeholder(path) {
                finalize_hydrated_file(path, &remote_id);
            } else {
                mark_hydrated_available(path);
            }
        } else {
            mark_hydrated_available(path);
        }
        sync_log(format!(
            "My Drive hydrate skipped (already local) — {}",
            relative
        ));
        return Ok(false);
    }
    let remote_id = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_get_placeholder(&conn, relative)?
            .filter(|(_, ty, _)| ty == "file")
            .map(|(id, _, _)| id)
    };
    let Some(remote_id) = remote_id else {
        // Local-only file — already on disk.
        sync_log(format!(
            "My Drive hydrate skipped (no remote id) — {}",
            relative
        ));
        return Ok(false);
    };
    let cached = ensure_hydrated_plaintext(api, db, &remote_id).await?;
    crate::my_drive::pin_hydrated_cache_to_path(&cached, path)?;
    crate::my_drive::mark_recent_hydrate(&remote_id);
    finalize_hydrated_file(path, &remote_id);
    Ok(true)
}

async fn hydrate_my_drive_folder(api: &ApiClient, db: &DbHandle, dir: &Path) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let child = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if child.is_dir() {
            Box::pin(hydrate_my_drive_folder(api, db, &child)).await?;
            continue;
        }
        if !child.is_file() {
            continue;
        }
        if crate::sync::should_skip_file(&name) {
            continue;
        }
        let Some(relative) = relative_path_from_sync_root(&sync_root, &child) else {
            continue;
        };
        if let Err(e) = hydrate_my_drive_file(api, db, &child, &relative).await {
            sync_log(format!(
                "My Drive hydrate file failed {}: {}",
                child.display(),
                e
            ));
        }
    }
    Ok(())
}

/// Upload pending changes then dehydrate to cloud placeholder (Stream “Free up space”).
pub async fn free_up_my_drive_path(
    api: &ApiClient,
    db: &DbHandle,
    path: &Path,
    on_progress: Option<MyDriveBusyCb>,
) -> AppResult<()> {
    let _permit = free_up_semaphore()
        .acquire()
        .await
        .map_err(|e| AppError::msg(e.to_string()))?;
    let _active = begin_free_up_active(path);

    let result = free_up_my_drive_path_inner(api, db, path, on_progress).await;
    // Reached only if not cancelled/aborted — Drop must not treat this as incomplete.
    mark_free_up_completed();
    result
}

async fn free_up_my_drive_path_inner(
    api: &ApiClient,
    db: &DbHandle,
    path: &Path,
    on_progress: Option<MyDriveBusyCb>,
) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let relative = relative_path_from_sync_root(&sync_root, path)
        .ok_or_else(|| AppError::msg("path outside sync root"))?;
    if !is_under_my_drive(&relative) {
        return Err(AppError::msg("path not under My Drive"));
    }

    if path.is_dir() {
        free_up_my_drive_folder(api, db, path, on_progress.as_ref()).await?;
        // One probe-aware tree pass at the top — never dehydrate without a readable cloud blob.
        if let Some(cb) = on_progress.as_ref() {
            cb(&format!("Freeing up space — finishing {}…", relative));
        }
        let tree_freed = free_up_tree_pass_with_probe(api, db, path).await?;
        sync_log(format!(
            "My Drive free-up final tree_pass={} — {}",
            tree_freed, relative
        ));
        if let Some(cb) = on_progress.as_ref() {
            cb(&format!("Freeing up space — sweeping {}…", relative));
        }
        let swept = free_up_sweep_stuck_unpinned(api, db, path).await?;
        sync_log(format!(
            "My Drive free-up sweep hydrated_left={} — {}",
            swept, relative
        ));
        sync_log(format!("My Drive freed folder — {}", relative));
        // Drop AppData plaintext copies — Stream re-downloads on open.
        clear_all_hydrate_cache();
        sync_log("My Drive free-up cleared hydrate_cache");
        // Explorer leaves UNPINNED on the folder — clear so Status is not stuck on arrows.
        refresh_placeholder_status(path);
        if is_unpinned(path) {
            sync_log(format!(
                "My Drive free-up pin clear retry — {}",
                path.display()
            ));
            if let Err(e) = clear_explicit_pin_state(path) {
                sync_log(format!(
                    "My Drive free-up pin clear retry failed {}: {}",
                    path.display(),
                    e
                ));
            } else {
                refresh_placeholder_status(path);
            }
        }
        // One shell refresh for the folder — not per-file (avoids FETCH_DATA thrash).
        notify_directory_updated(path);
        return Ok(());
    }
    if path.is_file() {
        if let Some(cb) = on_progress.as_ref() {
            cb(&format!("Freeing up space — {}…", relative));
        }
        let before = on_disk_allocated_bytes(path).unwrap_or(0);
        match tokio::time::timeout(
            FREE_UP_FILE_TIMEOUT,
            free_up_my_drive_file(api, db, path, &relative),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(AppError::msg(format!(
                    "free-up timed out after {}s: {}",
                    FREE_UP_FILE_TIMEOUT.as_secs(),
                    path.display()
                )));
            }
        }
        let after = on_disk_allocated_bytes(path).unwrap_or(0);
        sync_log(format!(
            "My Drive freed file — {} (on-disk {before} → {after})",
            relative
        ));
        return Ok(());
    }
    Err(AppError::msg("path is not a file or folder"))
}

async fn free_up_my_drive_file(
    api: &ApiClient,
    db: &DbHandle,
    path: &Path,
    relative: &str,
) -> AppResult<()> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    if crate::sync::should_skip_file(file_name) {
        return Ok(());
    }

    // Google Drive–like: never free local bytes until cloud has the current content.
    // upload_my_drive_path no-ops on unchanged hash; Err aborts Free up (keep local).
    if !is_dehydrated_placeholder(path) {
        let file_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if file_len >= FREE_UP_MAX_UPLOAD_BYTES {
            // Huge files: full hash/upload would stall the whole My Drive free-up for hours.
            // Only dehydrate when we already have a known synced content hash (no re-upload).
            let remote_id = {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                my_drive_get_placeholder(&conn, relative)?
                    .filter(|(_, ty, _)| ty == "file")
                    .map(|(id, _, _)| id)
            };
            let known_hash = if let Some(ref id) = remote_id {
                let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                crate::db::my_drive_known_content_hash(&conn, id).unwrap_or_default()
            } else {
                String::new()
            };
            if remote_id.is_none() || known_hash.is_empty() {
                sync_log(format!(
                    "My Drive free-up skip upload (too large {} MiB, keep local) — {}",
                    file_len / (1024 * 1024),
                    path.display()
                ));
                refresh_placeholder_status(path);
                return Ok(());
            }
            // Known sync — skip upload/hash; probe + dehydrate below.
            sync_log(format!(
                "My Drive free-up skip upload (too large {} MiB, known hash) — {}",
                file_len / (1024 * 1024),
                path.display()
            ));
        } else if let Err(e) = upload_my_drive_path(api, db, path).await {
            sync_log(format!(
                "My Drive free-up upload failed {}: {}",
                path.display(),
                e
            ));
            return Err(e);
        }
    }

    let remote_id = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        my_drive_get_placeholder(&conn, relative)?
            .filter(|(_, ty, _)| ty == "file")
            .map(|(id, _, _)| id)
    }
    .ok_or_else(|| AppError::msg(format!("no remote file id for {}", relative)))?;

    // If local content exists, verify the cloud blob before dehydrating — otherwise
    // Free up would destroy the only readable copy when the server blob is missing.
    if !is_dehydrated_placeholder(path) {
        match api.probe_file_download(&remote_id).await {
            Ok(true) => {}
            Ok(false) => {
                sync_log(format!(
                    "My Drive free-up skip dehydrate — cloud blob missing, keeping local {}",
                    path.display()
                ));
                refresh_placeholder_status(path);
                return Ok(());
            }
            Err(e) => {
                let msg = e.to_string();
                if is_blob_missing_error(&msg) {
                    sync_log(format!(
                        "My Drive free-up skip dehydrate — cloud blob missing, keeping local {}: {}",
                        path.display(),
                        e
                    ));
                    refresh_placeholder_status(path);
                    return Ok(());
                }
                sync_log(format!(
                    "My Drive free-up blob probe failed {}, dehydrating anyway: {}",
                    path.display(),
                    e
                ));
            }
        }
    }

    clear_hydrate_cache_for_file(&remote_id);

    // Mark before dehydrate — Explorer FETCH_DATA can fire during CfDehydratePlaceholder.
    crate::my_drive::mark_recent_dehydrate(&remote_id);

    match dehydrate_placeholder_file_async(path).await {
        Ok(()) => {}
        Err(e) if is_not_cloud_file_error(&e) => {
            sync_log(format!(
                "My Drive free-up converting plain file {}",
                path.display()
            ));
            let file_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if file_len >= FREE_UP_MAX_UPLOAD_BYTES {
                sync_log(format!(
                    "My Drive free-up skip convert/upload (too large {} MiB, keep local) — {}",
                    file_len / (1024 * 1024),
                    path.display()
                ));
                refresh_placeholder_status(path);
                return Ok(());
            }
            match convert_file_to_placeholder_async(path, &remote_id).await {
                Ok(()) => dehydrate_placeholder_file_async(path).await?,
                Err(conv_err) => {
                    // Local content may be ahead of cloud — push then convert.
                    sync_log(format!(
                        "My Drive free-up convert failed {}, uploading first: {}",
                        path.display(),
                        conv_err
                    ));
                    upload_my_drive_path(api, db, path).await?;
                    convert_file_to_placeholder_async(path, &remote_id).await?;
                    crate::my_drive::mark_recent_dehydrate(&remote_id);
                    dehydrate_placeholder_file_async(path).await?;
                }
            }
        }
        Err(e) => return Err(e),
    }

    Ok(())
}

async fn free_up_my_drive_folder(
    api: &ApiClient,
    db: &DbHandle,
    dir: &Path,
    on_progress: Option<&MyDriveBusyCb>,
) -> AppResult<()> {
    let sync_root = sync_root_dir(false)?;
    let files = collect_free_up_files_under(dir);
    let total = files.len() as u64;
    set_free_up_total(total);
    sync_log(format!(
        "My Drive free-up folder walk — {} file(s) under {}",
        total,
        dir.display()
    ));
    if let Some(cb) = on_progress {
        cb(&format!(
            "Freeing up space — 0/{} under {}…",
            total,
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("folder")
        ));
    }

    let mut freed = 0u32;
    let mut failed = 0u32;
    let mut already_cloud = 0u32;
    let mut processed = 0u64;

    for child in &files {
        processed += 1;
        update_free_up_progress(child, processed, total, freed, failed);
        let name = child
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        if crate::sync::should_skip_file(&name) {
            continue;
        }
        if is_dehydrated_placeholder(child) {
            already_cloud += 1;
            if is_unpinned(child) {
                refresh_placeholder_status(child);
            }
            continue;
        }
        let Some(relative) = relative_path_from_sync_root(&sync_root, child) else {
            continue;
        };
        match tokio::time::timeout(
            FREE_UP_FILE_TIMEOUT,
            free_up_my_drive_file(api, db, child, &relative),
        )
        .await
        {
            Ok(Ok(())) => {
                freed += 1;
                update_free_up_progress(child, processed, total, freed, failed);
                if freed % 10 == 0 || processed == total {
                    sync_log(format!(
                        "My Drive free-up progress — {}/{} freed={} failed={} cloud={} under {}",
                        processed,
                        total,
                        freed,
                        failed,
                        already_cloud,
                        dir.display()
                    ));
                    if let Some(cb) = on_progress {
                        let short = if relative.len() > 72 {
                            format!("…{}", &relative[relative.len() - 69..])
                        } else {
                            relative.clone()
                        };
                        cb(&format!(
                            "Freeing up space — {}/{} — {}…",
                            processed, total, short
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
            Ok(Err(e)) => {
                failed += 1;
                update_free_up_progress(child, processed, total, freed, failed);
                sync_log(format!(
                    "My Drive free-up file failed {}: {}",
                    child.display(),
                    e
                ));
                // Honest Status: local check, not stuck sync arrows.
                refresh_placeholder_status(child);
                sync_log(format!(
                    "My Drive free-up status kept local — {}",
                    child.display()
                ));
            }
            Err(_) => {
                failed += 1;
                update_free_up_progress(child, processed, total, freed, failed);
                sync_log(format!(
                    "My Drive free-up file timed out ({}s) {}",
                    FREE_UP_FILE_TIMEOUT.as_secs(),
                    child.display()
                ));
                refresh_placeholder_status(child);
                sync_log(format!(
                    "My Drive free-up status kept local — {}",
                    child.display()
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Clear leftover UNPINNED on nested folders themselves.
    clear_unpinned_dirs_under(dir);

    sync_log(format!(
        "My Drive free-up folder done — path={} total={} freed={} failed={} already_cloud={}",
        dir.display(),
        total,
        freed,
        failed,
        already_cloud
    ));
    Ok(())
}

/// Depth-first list of regular files under `dir` (skips desktop.ini / dot names at each level).
fn collect_free_up_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_free_up_files_recursive(dir, &mut out);
    out
}

fn collect_free_up_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            sync_log(format!(
                "My Drive free-up collect walk failed {}: {}",
                dir.display(),
                e
            ));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_free_up_files_recursive(&path, out);
            continue;
        }
        if path.is_file() {
            out.push(path);
        }
    }
}

fn clear_unpinned_dirs_under(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            clear_unpinned_dirs_under(&path);
            if is_unpinned(&path) {
                refresh_placeholder_status(&path);
            }
        }
    }
}

/// Second pass: any still-hydrated file under the tree (not only UNPINNED).
async fn free_up_sweep_stuck_unpinned(
    api: &ApiClient,
    db: &DbHandle,
    root: &Path,
) -> AppResult<u32> {
    let sync_root = sync_root_dir(false)?;
    let files = collect_free_up_files_under(root);
    let mut retried = 0u32;
    for path in files {
        if is_dehydrated_placeholder(&path) {
            if is_unpinned(&path) {
                refresh_placeholder_status(&path);
            }
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        if crate::sync::should_skip_file(name) {
            continue;
        }
        let Some(relative) = relative_path_from_sync_root(&sync_root, &path) else {
            continue;
        };
        retried += 1;
        match tokio::time::timeout(
            FREE_UP_FILE_TIMEOUT,
            free_up_my_drive_file(api, db, &path, &relative),
        )
        .await
        {
            Ok(Ok(())) => {
                sync_log(format!("My Drive free-up sweep ok — {}", path.display()));
            }
            Ok(Err(e)) => {
                sync_log(format!(
                    "My Drive free-up sweep keep local {}: {}",
                    path.display(),
                    e
                ));
                refresh_placeholder_status(&path);
            }
            Err(_) => {
                sync_log(format!(
                    "My Drive free-up sweep timed out ({}s) — status kept local {}",
                    FREE_UP_FILE_TIMEOUT.as_secs(),
                    path.display()
                ));
                refresh_placeholder_status(&path);
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    clear_unpinned_dirs_under(root);
    Ok(retried)
}

/// Final free-up walk: dehydrate leftovers only when the cloud blob is readable.
async fn free_up_tree_pass_with_probe(
    api: &ApiClient,
    db: &DbHandle,
    root: &Path,
) -> AppResult<u32> {
    let sync_root = sync_root_dir(false)?;
    let mut freed = 0u32;
    free_up_tree_pass_recursive(api, db, &sync_root, root, &mut freed).await?;
    Ok(freed)
}

async fn free_up_tree_pass_recursive(
    api: &ApiClient,
    db: &DbHandle,
    sync_root: &Path,
    dir: &Path,
    freed: &mut u32,
) -> AppResult<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            sync_log(format!(
                "My Drive free-up tree_pass walk failed {}: {}",
                dir.display(),
                e
            ));
            return Ok(());
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            Box::pin(free_up_tree_pass_recursive(
                api, db, sync_root, &path, freed,
            ))
            .await?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if crate::sync::should_skip_file(&name) {
            continue;
        }
        if is_dehydrated_placeholder(&path) {
            continue;
        }
        let Some(relative) = relative_path_from_sync_root(sync_root, &path) else {
            continue;
        };
        let remote_id = {
            let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            my_drive_get_placeholder(&conn, &relative)?
                .filter(|(_, ty, _)| ty == "file")
                .map(|(id, _, _)| id)
        };
        let Some(remote_id) = remote_id else {
            // No remote mapping — leave for earlier free_up_my_drive_file pass / next sync.
            continue;
        };

        match api.probe_file_download(&remote_id).await {
            Ok(true) => {}
            Ok(false) => {
                sync_log(format!(
                    "My Drive free-up skip dehydrate — cloud blob missing, keeping local {}",
                    path.display()
                ));
                refresh_placeholder_status(&path);
                continue;
            }
            Err(e) => {
                let msg = e.to_string();
                if is_blob_missing_error(&msg) {
                    sync_log(format!(
                        "My Drive free-up skip dehydrate — cloud blob missing, keeping local {}: {}",
                        path.display(),
                        e
                    ));
                    refresh_placeholder_status(&path);
                    continue;
                }
                sync_log(format!(
                    "My Drive free-up tree_pass probe failed {}, skipping dehydrate: {}",
                    path.display(),
                    e
                ));
                continue;
            }
        }

        clear_hydrate_cache_for_file(&remote_id);
        // Mark before dehydrate — Explorer FETCH_DATA can fire during CfDehydratePlaceholder.
        crate::my_drive::mark_recent_dehydrate(&remote_id);
        match dehydrate_placeholder_file_async(&path).await {
            Ok(()) => {
                *freed += 1;
                sync_log(format!("cfapi: dehydrated {}", path.display()));
            }
            Err(e) => {
                sync_log(format!(
                    "cfapi: dehydrate skipped {}: {}",
                    path.display(),
                    e
                ));
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Ok(())
}

fn local_dir_for_relative(sync_root: &Path, parent_relative: &str) -> PathBuf {
    let mut path = sync_root.to_path_buf();
    for part in parent_relative.split(['\\', '/']).filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path
}

fn join_my_drive_relative(parent_relative: &str, name: &str) -> String {
    format!(
        "{}\\{}",
        parent_relative.trim_end_matches(['\\', '/']),
        name
    )
}

fn sanitize_name(name: &str) -> String {
    name.replace(['/', '\\'], "_")
}

fn set_path_mtime_from_remote(path: &Path, updated_at: &str) {
    let when = chrono::DateTime::parse_from_rfc3339(updated_at)
        .ok()
        .map(|dt| std::time::SystemTime::from(dt.with_timezone(&chrono::Utc)))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%d %H:%M:%S%.f")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%d %H:%M:%S"))
                .ok()
                .map(|ndt| std::time::SystemTime::from(ndt.and_utc()))
        })
        .unwrap_or_else(std::time::SystemTime::now);
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_modified(when);
    }
}

/// When server content is newer (restore / web edit), replace stale local bytes instead of keeping them.
async fn refresh_files_when_remote_newer(
    api: &ApiClient,
    db: &DbHandle,
    parent_relative: &str,
    local_dir: &Path,
    files: &[crate::api::types::FileRecord],
    mirror: bool,
    suppress: Option<&WatcherSuppress>,
    stats: &mut MyDrivePollStats,
) {
    for file in files {
        let known = {
            let Ok(conn) = db.lock() else {
                continue;
            };
            crate::db::my_drive_known_remote_version(&conn, &file.id).unwrap_or(0)
        };
        let local_path = local_dir.join(sanitize_name(&file.name));
        let expected = file.size.max(0) as u64;
        let size_mismatch = match std::fs::metadata(&local_path) {
            Ok(meta) if !is_dehydrated_placeholder(&local_path) => meta.len() != expected,
            Ok(_) => false, // dehydrated placeholder — size on disk is not content
            Err(_) => false,
        };
        if file.version <= known && !size_mismatch {
            continue;
        }
        // Nothing local yet — update known version; open will hydrate via FETCH_DATA.
        if !local_path.exists() && !mirror {
            if let Ok(conn) = db.lock() {
                let child_rel = join_my_drive_relative(parent_relative, &file.name);
                let parent_id = crate::db::my_drive_get_placeholder_by_remote_id(&conn, &file.id)
                    .ok()
                    .flatten()
                    .and_then(|(_, _, p)| p);
                let _ = my_drive_upsert_placeholder(
                    &conn,
                    &child_rel,
                    &file.id,
                    "file",
                    parent_id.as_deref(),
                    Some(file.version),
                );
            }
            continue;
        }
        let relative = join_my_drive_relative(parent_relative, &file.name);
        match pull_remote_file_over_local(api, db, &local_path, &relative, file, suppress).await {
            Ok(()) => {
                stats.files_mirrored += 1;
                sync_log(format!(
                    "My Drive refreshed remote-newer v{} — {}",
                    file.version, relative
                ));
            }
            Err(e) => {
                stats.errors += 1;
                sync_log(format!(
                    "My Drive refresh remote-newer failed {}: {}",
                    relative, e
                ));
            }
        }
    }
}

async fn pull_remote_file_over_local(
    api: &ApiClient,
    db: &DbHandle,
    local_path: &Path,
    relative: &str,
    file: &crate::api::types::FileRecord,
    suppress: Option<&WatcherSuppress>,
) -> AppResult<()> {
    clear_hydrate_cache_for_file(&file.id);
    let parent_remote = {
        let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        crate::db::my_drive_get_placeholder_by_remote_id(&conn, &file.id)?
            .and_then(|(_, _, p)| p)
    };

    let cached = ensure_hydrated_plaintext(api, db, &file.id).await?;
    if let Some(parent) = local_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let copy = || -> AppResult<()> {
        std::fs::copy(&cached, local_path)?;
        set_path_mtime_from_remote(local_path, &file.updated_at);
        Ok(())
    };
    if let Some(suppress) = suppress {
        suppress.run_suppressed(local_path, copy)?;
    } else {
        copy()?;
    }

    // Drive-like Stream: keep pulled content local until Free up (no auto-dehydrate).
    if crate::sync::engine::sync_mode_is_stream(db)
        && local_path.exists()
        && !crate::my_drive::is_fetch_data_inflight(&file.id)
    {
        let mark = || -> AppResult<()> {
            match finalize_stream_placeholder(local_path, &file.id) {
                Ok(()) => Ok(()),
                Err(e) if is_not_cloud_file_error(&e) => {
                    convert_file_to_placeholder(local_path, &file.id)?;
                    finalize_stream_placeholder(local_path, &file.id).or(Ok(()))
                }
                Err(_) => Ok(()),
            }
        };
        if let Some(suppress) = suppress {
            let _ = suppress.run_suppressed(local_path, mark);
        } else {
            let _ = mark();
        }
    }

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    my_drive_upsert_placeholder(
        &conn,
        relative,
        &file.id,
        "file",
        parent_remote.as_deref(),
        Some(file.version),
    )?;
    if let Ok(hash) = crate::my_drive::hash_local_file(&cached) {
        let _ = crate::db::my_drive_set_content_hash(&conn, &file.id, &hash);
    }
    Ok(())
}
