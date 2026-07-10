use crate::api::ApiClient;
use crate::auth_store::mirror_dir;
use crate::blocking::{self, file_sync_timeout, hash_timeout};
use crate::crypto::key_to_b64url;
use crate::db::{
    self, clear_folder_mapping_prefix, clear_folder_mappings, clear_stale_activity,
    clear_sync_state_for_folder, clear_sync_state_remote_file, config_get, config_set,
    delete_folder_mapping, get_file_key, get_folder_mapping, get_sync_folder_by_path,
    get_sync_state, insert_sync_folder, is_pending_remote_folder, list_sync_folders,
    set_folder_mapping, store_file_key, update_sync_folder_remote_id, upsert_activity,
    upsert_sync_state, DbHandle, SyncFolderRow,
};
use crate::error::{AppError, AppResult};
use crate::sync::log::sync_log;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatusKind {
    UpToDate,
    Syncing,
    Paused,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    pub status: SyncStatusKind,
    pub message: String,
    pub last_synced_at: Option<String>,
    pub paused: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncProgress {
    pub phase: String,
    pub processed: u64,
    pub total: u64,
    pub uploaded: u64,
    pub skipped: u64,
    pub unchanged: u64,
    pub errors: u64,
    pub current: u64,
    pub current_file: String,
    pub message: String,
    #[serde(default = "default_show_in_ui")]
    pub show_in_ui: bool,
}

fn default_show_in_ui() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    /// First sync or explicit resume — progress only for files that need work.
    Interactive,
    /// Startup verification — no UI for unchanged files.
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSyncOutcome {
    Synced,
    Unchanged,
    Skipped,
    Failed,
}

enum SyncAttemptResult {
    Done(FileSyncOutcome),
    RetryClearFileState,
    RetryClearRoot,
    RetryClearFolderMapping(String),
}

const FOLDER_TIMEOUT: Duration = Duration::from_secs(30);
const LARGE_FILE_WARN_BYTES: i64 = 50 * 1024 * 1024;

struct InitialSyncGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for InitialSyncGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

pub struct SyncEngine {
    api: ApiClient,
    db: DbHandle,
    app: AppHandle,
    paused: AtomicBool,
    initial_sync_running: AtomicBool,
    sync_lock: tokio::sync::Mutex<()>,
    pending_paths: Mutex<VecDeque<PathBuf>>,
    status: RwLock<SyncStatus>,
    computer_id: RwLock<Option<String>>,
    computer_root_id: RwLock<Option<String>>,
    shutdown: AtomicBool,
}

impl SyncEngine {
    pub fn new(api: ApiClient, db: DbHandle, app: AppHandle) -> Self {
        Self {
            api,
            db,
            app,
            paused: AtomicBool::new(false),
            initial_sync_running: AtomicBool::new(false),
            sync_lock: tokio::sync::Mutex::new(()),
            pending_paths: Mutex::new(VecDeque::new()),
            status: RwLock::new(SyncStatus {
                status: SyncStatusKind::UpToDate,
                message: "Ready".into(),
                last_synced_at: None,
                paused: false,
            }),
            computer_id: RwLock::new(None),
            computer_root_id: RwLock::new(None),
            shutdown: AtomicBool::new(false),
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    pub fn load_computer_from_db(&self) {
        if let Ok(conn) = self.db.lock() {
            if let Ok(Some(id)) = config_get(&conn, "computer_id") {
                if !id.is_empty() {
                    *self.computer_id.write() = Some(id);
                }
            }
            if let Ok(Some(root)) = config_get(&conn, "computer_root_id") {
                if !root.is_empty() {
                    *self.computer_root_id.write() = Some(root);
                }
            }
            let _ = clear_stale_activity(&conn);
            if let Ok(Some(ts)) = config_get(&conn, "last_synced_at") {
                if !ts.is_empty() {
                    let mut st = self.status.write();
                    st.last_synced_at = Some(ts);
                    st.status = SyncStatusKind::UpToDate;
                    st.message = "Up to date".into();
                }
            }
        }
    }

    pub fn enqueue_file_path(self: &Arc<Self>, path: PathBuf) {
        if self.is_paused() || !path.is_file() {
            return;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        if should_skip_file(file_name) {
            return;
        }

        if self.is_initial_sync_running() {
            let mut queue = self.pending_paths.lock();
            let canonical = path.canonicalize().unwrap_or(path.clone());
            if !queue
                .iter()
                .any(|p| p.canonicalize().unwrap_or(p.clone()) == canonical)
            {
                queue.push_back(path);
            }
            return;
        }

        let engine = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let _ = engine.sync_file_path(&path).await;
        });
    }

    pub async fn drain_pending_paths(&self) {
        let paths: Vec<PathBuf> = {
            let mut queue = self.pending_paths.lock();
            queue.drain(..).collect()
        };

        let mut seen = HashSet::new();
        for path in paths {
            let key = path.canonicalize().unwrap_or(path.clone());
            if seen.insert(key) {
                let _ = self.sync_file_path(&path).await;
            }
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let mut st = self.status.write();
        st.paused = paused;
        st.status = if paused {
            SyncStatusKind::Paused
        } else {
            SyncStatusKind::UpToDate
        };
        st.message = if paused {
            "Sync paused".into()
        } else {
            "Up to date".into()
        };
        let _ = self.app.emit("sync-status-changed", st.clone());
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_initial_sync_running(&self) -> bool {
        self.initial_sync_running.load(Ordering::SeqCst)
    }

    pub fn get_status(&self) -> SyncStatus {
        self.status.read().clone()
    }

    pub fn report_sync_error(&self, message: impl Into<String>) {
        self.set_status(SyncStatusKind::Error, message);
    }

    fn set_status(&self, kind: SyncStatusKind, message: impl Into<String>) {
        let mut st = self.status.write();
        st.status = kind;
        st.message = message.into();
        st.paused = self.paused.load(Ordering::SeqCst);
        if matches!(st.status, SyncStatusKind::UpToDate) {
            let now = chrono::Utc::now().to_rfc3339();
            st.last_synced_at = Some(now.clone());
            if let Ok(conn) = self.db.lock() {
                let _ = config_set(&conn, "last_synced_at", &now);
            }
        }
        let _ = self.app.emit("sync-status-changed", st.clone());
    }

    fn emit_activity_with_conn(
        &self,
        conn: &rusqlite::Connection,
        name: &str,
        detail: &str,
        size: i64,
        status: &str,
    ) {
        let activity_id = upsert_activity(conn, name, detail, size, status).ok();
        let _ = self.app.emit(
            "sync-activity",
            serde_json::json!({
                "id": activity_id,
                "name": name,
                "detail": detail,
                "file_size": size,
                "status": status,
            }),
        );
    }

    fn emit_activity(&self, name: &str, detail: &str, size: i64, status: &str) {
        if let Ok(conn) = self.db.lock() {
            self.emit_activity_with_conn(&conn, name, detail, size, status);
        } else {
            let _ = self.app.emit(
                "sync-activity",
                serde_json::json!({
                    "id": null,
                    "name": name,
                    "detail": detail,
                    "file_size": size,
                    "status": status,
                }),
            );
        }
    }

    fn emit_progress(&self, progress: &SyncProgress) {
        let _ = self.app.emit("sync-progress", progress.clone());
        if !progress.show_in_ui {
            return;
        }
        let status_msg = match progress.phase.as_str() {
            "scanning" => progress.message.clone(),
            "syncing" if !progress.message.is_empty() => progress.message.clone(),
            "syncing" if progress.total > 0 && !progress.current_file.is_empty() => {
                format_sync_progress(progress.processed, progress.total, &progress.current_file)
            }
            "syncing" if progress.total > 0 => {
                let pct = (progress.processed * 100) / progress.total.max(1);
                format!(
                    "Processing {}/{} ({}%) files…",
                    progress.processed, progress.total, pct
                )
            }
            "syncing" => progress.message.clone(),
            "done" => progress.message.clone(),
            _ => progress.message.clone(),
        };
        match progress.phase.as_str() {
            "done" if progress.errors > 0 => {
                self.set_status(SyncStatusKind::Error, status_msg);
            }
            "done" => {
                self.set_status(SyncStatusKind::UpToDate, status_msg);
            }
            "scanning" | "syncing" => {
                self.set_status(SyncStatusKind::Syncing, status_msg);
            }
            _ => {}
        }
    }

    async fn with_heartbeat<F, Fut, T>(&self, progress: SyncProgress, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_hb = cancel.clone();
        let app = self.app.clone();
        let heartbeat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if cancel_hb.load(Ordering::Relaxed) {
                    break;
                }
                let _ = app.emit("sync-progress", progress.clone());
            }
        });

        let result = f().await;
        cancel.store(true, Ordering::Relaxed);
        let _ = heartbeat.await;
        result
    }

    fn mark_initial_sync_complete(&self) {
        if let Ok(conn) = self.db.lock() {
            let _ = config_set(&conn, "initial_sync_complete", "true");
        }
    }

    fn record_sync_failure(
        &self,
        sync_folder_id: i64,
        relative_path: &str,
        local_path: &str,
        mtime: i64,
        _detail: &str,
        permanent: bool,
    ) {
        if let Ok(conn) = self.db.lock() {
            let status = if permanent { "rejected" } else { "error" };
            let _ = upsert_sync_state(
                &conn,
                sync_folder_id,
                relative_path,
                local_path,
                None,
                None,
                Some(mtime),
                None,
                status,
            );
        }
    }

    fn is_computer_not_found_error(err: &AppError) -> bool {
        let msg = err.to_string().to_lowercase();
        msg.contains("not found") || msg.contains("(404)")
    }

    async fn heartbeat_existing_computer(&self, id: &str) -> AppResult<bool> {
        for attempt in 0..3u32 {
            match self.api.heartbeat(id).await {
                Ok(_) => return Ok(true),
                Err(e) if Self::is_computer_not_found_error(&e) => return Ok(false),
                Err(e) => {
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(500 * (attempt + 1) as u64)).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        Err(AppError::msg("heartbeat failed"))
    }

    fn store_computer_ids(&self, computer_id: &str, root_folder_id: &str) -> AppResult<()> {
        let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        config_set(&conn, "computer_id", computer_id)?;
        config_set(&conn, "computer_root_id", root_folder_id)?;
        *self.computer_id.write() = Some(computer_id.to_string());
        *self.computer_root_id.write() = Some(root_folder_id.to_string());
        Ok(())
    }

    pub async fn setup_computer(&self) -> AppResult<()> {
        let existing_id = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            config_get(&conn, "computer_id")?
        };

        if let Some(id) = existing_id.filter(|id| !id.is_empty()) {
            if self.heartbeat_existing_computer(&id).await? {
                let root = {
                    let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                    config_get(&conn, "computer_root_id")?
                };
                if let Some(r) = root.filter(|r| !r.is_empty()) {
                    *self.computer_root_id.write() = Some(r);
                }
                *self.computer_id.write() = Some(id);
                return Ok(());
            }
            {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                config_set(&conn, "computer_id", "")?;
                config_set(&conn, "computer_root_id", "")?;
            }
            *self.computer_id.write() = None;
            *self.computer_root_id.write() = None;
        }

        let host = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "desktop".into());
        let name = host.clone();

        let computer = self.api.register_computer(&name, &host).await?;
        self.store_computer_ids(&computer.id, &computer.root_folder_id)
    }

    async fn ensure_sync_folder_remote(&self, sf: &SyncFolderRow) -> AppResult<String> {
        if self
            .api
            .get_folder_contents(&sf.remote_folder_id)
            .await
            .is_ok()
        {
            return Ok(sf.remote_folder_id.clone());
        }

        self.setup_computer().await?;
        let root = self
            .computer_root_id
            .read()
            .clone()
            .ok_or_else(|| AppError::msg("computer not registered"))?;

        let folder_name = if sf.label.is_empty() {
            Path::new(&sf.local_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Folder")
                .to_string()
        } else {
            sf.label.clone()
        };

        let remote = self
            .api
            .create_or_resolve_folder(&folder_name, Some(&root))
            .await?;
        {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            update_sync_folder_remote_id(&conn, sf.id, &remote.id)?;
            clear_folder_mappings(&conn, sf.id)?;
        }
        Ok(remote.id)
    }

    pub async fn configure_folders(&self, folders: Vec<(String, String)>) -> AppResult<()> {
        self.setup_computer().await?;
        let root_id = self
            .computer_root_id
            .read()
            .clone()
            .ok_or_else(|| AppError::msg("computer not registered"))?;

        for (local_path, label) in folders {
            let existing = {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                get_sync_folder_by_path(&conn, &local_path)?
            };

            if let Some(ref row) = existing {
                if !is_pending_remote_folder(&row.remote_folder_id) {
                    continue;
                }
            }

            let folder_name = if label.is_empty() {
                Path::new(&local_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Folder")
                    .to_string()
            } else {
                label.clone()
            };

            let remote = self
                .api
                .create_or_resolve_folder(&folder_name, Some(&root_id))
                .await?;

            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            if let Some(row) = existing {
                update_sync_folder_remote_id(&conn, row.id, &remote.id)?;
            } else {
                insert_sync_folder(&conn, &local_path, &remote.id, &folder_name)?;
            }
        }

        Ok(())
    }

    pub async fn add_sync_folder(&self, local_path: &str, label: &str) -> AppResult<()> {
        {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            if get_sync_folder_by_path(&conn, local_path)?.is_some() {
                return Err(AppError::msg("folder is already syncing"));
            }
        }

        self.configure_folders(vec![(local_path.to_string(), label.to_string())])
            .await?;

        Ok(())
    }

    pub async fn sync_single_folder(self: Arc<Self>, sync_folder_id: i64) -> AppResult<()> {
        if self.is_paused() {
            return Ok(());
        }

        if self
            .initial_sync_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        let _guard = InitialSyncGuard {
            flag: &self.initial_sync_running,
        };
        let result = Self::sync_single_folder_inner(&self, sync_folder_id).await;
        self.drain_pending_paths().await;
        result
    }

    async fn sync_single_folder_inner(self: &Arc<Self>, sync_folder_id: i64) -> AppResult<()> {
        let sf = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            list_sync_folders(&conn)?
                .into_iter()
                .find(|f| f.id == sync_folder_id)
                .ok_or_else(|| AppError::msg("sync folder not found"))?
        };

        let remote_root_id = self.prepare_sync_folder_for_scan(&sf).await?;

        let stats = self
            .scan_folder(
                sf.id,
                &sf.local_path,
                &remote_root_id,
                ScanMode::Interactive,
            )
            .await?;

        let summary = format!(
            "Up to date — {} uploaded, {} unchanged, {} skipped",
            stats.0, stats.2, stats.1
        );
        self.set_status(SyncStatusKind::UpToDate, &summary);
        Ok(())
    }

    pub async fn run_background_verify(self: Arc<Self>) -> AppResult<()> {
        if self.is_paused() {
            return Ok(());
        }

        if self
            .initial_sync_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        let _guard = InitialSyncGuard {
            flag: &self.initial_sync_running,
        };

        sync_log("background verify started");
        let result = Self::run_background_verify_inner(&self).await;
        self.drain_pending_paths().await;
        sync_log("background verify finished");
        result
    }

    async fn run_background_verify_inner(self: &Arc<Self>) -> AppResult<()> {
        let sync_folders = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            list_sync_folders(&conn)?
        };

        let mut total_uploaded = 0u64;
        let mut total_skipped = 0u64;
        let mut total_unchanged = 0u64;
        let mut total_errors = 0u64;

        for sf in sync_folders {
            let remote_root_id = self.prepare_sync_folder_for_scan(&sf).await?;
            let stats = self
                .scan_folder(
                    sf.id,
                    &sf.local_path,
                    &remote_root_id,
                    ScanMode::Background,
                )
                .await?;
            total_uploaded += stats.0;
            total_skipped += stats.1;
            total_unchanged += stats.2;
            total_errors += stats.3;
        }

        let total_processed = total_uploaded + total_skipped + total_unchanged + total_errors;
        if total_uploaded > 0 || total_errors > 0 {
            let summary = if total_errors > 0 {
                format!(
                    "Sync finished: {} uploaded, {} unchanged, {} skipped, {} errors",
                    total_uploaded, total_unchanged, total_skipped, total_errors
                )
            } else {
                format!(
                    "Up to date — {} uploaded, {} unchanged, {} skipped",
                    total_uploaded, total_unchanged, total_skipped
                )
            };

            if total_errors > 0 {
                self.set_status(SyncStatusKind::Error, &summary);
            } else {
                self.set_status(SyncStatusKind::UpToDate, &summary);
            }

            self.emit_progress(&SyncProgress {
                phase: "done".into(),
                processed: total_processed,
                total: total_processed,
                uploaded: total_uploaded,
                skipped: total_skipped,
                unchanged: total_unchanged,
                errors: total_errors,
                current: total_processed,
                current_file: String::new(),
                message: summary,
                show_in_ui: true,
            });
        } else if matches!(self.get_status().status, SyncStatusKind::Syncing) {
            self.set_status(SyncStatusKind::UpToDate, "Up to date");
        }

        Ok(())
    }

    pub async fn run_initial_sync(self: Arc<Self>) -> AppResult<()> {
        if self.is_paused() {
            return Ok(());
        }

        if self
            .initial_sync_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        let _guard = InitialSyncGuard {
            flag: &self.initial_sync_running,
        };
        sync_log("initial sync started");
        let result = Self::run_initial_sync_inner(&self).await;
        self.drain_pending_paths().await;
        sync_log("initial sync finished");
        result
    }

    async fn prepare_sync_folder_for_scan(&self, sf: &SyncFolderRow) -> AppResult<String> {
        if self.remote_folder_exists(&sf.remote_folder_id).await {
            return Ok(sf.remote_folder_id.clone());
        }

        let new_root = self.ensure_sync_folder_remote(sf).await?;
        let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        clear_folder_mappings(&conn, sf.id)?;
        clear_sync_state_for_folder(&conn, sf.id)?;
        Ok(new_root)
    }

    async fn remote_folder_exists(&self, folder_id: &str) -> bool {
        self.api.get_folder_contents(folder_id).await.is_ok()
    }

    async fn run_initial_sync_inner(self: &Arc<Self>) -> AppResult<()> {
        let sync_folders = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            list_sync_folders(&conn)?
        };

        let mut total_uploaded = 0u64;
        let mut total_skipped = 0u64;
        let mut total_unchanged = 0u64;
        let mut total_errors = 0u64;

        for sf in sync_folders {
            let remote_root_id = self.prepare_sync_folder_for_scan(&sf).await?;
            let stats = self
                .scan_folder(
                    sf.id,
                    &sf.local_path,
                    &remote_root_id,
                    ScanMode::Interactive,
                )
                .await?;
            total_uploaded += stats.0;
            total_skipped += stats.1;
            total_unchanged += stats.2;
            total_errors += stats.3;
        }

        let total_processed = total_uploaded + total_skipped + total_unchanged + total_errors;
        let summary = if total_errors > 0 {
            format!(
                "Sync finished: {} uploaded, {} unchanged, {} skipped, {} errors",
                total_uploaded, total_unchanged, total_skipped, total_errors
            )
        } else {
            format!(
                "Up to date — {} uploaded, {} unchanged, {} skipped",
                total_uploaded, total_unchanged, total_skipped
            )
        };

        if total_errors > 0 {
            self.set_status(SyncStatusKind::Error, &summary);
        } else {
            self.set_status(SyncStatusKind::UpToDate, &summary);
        }
        self.mark_initial_sync_complete();

        let _ = self.app.emit(
            "sync-progress",
            SyncProgress {
                phase: "done".into(),
                processed: total_processed,
                total: total_processed,
                uploaded: total_uploaded,
                skipped: total_skipped,
                unchanged: total_unchanged,
                errors: total_errors,
                current: total_processed,
                current_file: String::new(),
                message: summary.clone(),
                show_in_ui: true,
            },
        );

        Ok(())
    }

    /// Returns (uploaded, skipped, unchanged, errors)
    async fn scan_folder(
        self: &Arc<Self>,
        sync_folder_id: i64,
        local_root: &str,
        remote_root_id: &str,
        mode: ScanMode,
    ) -> AppResult<(u64, u64, u64, u64)> {
        let root = PathBuf::from(local_root);
        if !root.exists() {
            return Ok((0, 0, 0, 0));
        }

        let show_ui = mode == ScanMode::Interactive;
        sync_log(format!("scan start {} ({:?})", local_root, mode));

        let walk_root = root.clone();
        let files = blocking::run_blocking_with_timeout_async(
            Duration::from_secs(120),
            move || Ok(collect_files(&walk_root)),
        )
        .await
        .unwrap_or_default();

        let total = files.len() as u64;
        sync_log(format!("scan complete — {} files", total));

        if show_ui {
            self.emit_progress(&SyncProgress {
                phase: "scanning".into(),
                processed: 0,
                total,
                uploaded: 0,
                skipped: 0,
                unchanged: 0,
                errors: 0,
                current: 0,
                current_file: String::new(),
                message: format!("Scanning complete — {} files", total),
                show_in_ui: true,
            });
        }

        let mut uploaded = 0u64;
        let mut skipped = 0u64;
        let mut unchanged = 0u64;
        let mut errors = 0u64;
        let mut processed = 0u64;
        let remote_root = remote_root_id.to_string();

        for path in files {
            if self.is_paused() {
                sync_log("scan paused by user");
                break;
            }

            processed += 1;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            let size = file_size(&path);

            if should_skip_file(&name) {
                skipped += 1;
                continue;
            }

            let relative_str = path
                .strip_prefix(&root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let mtime = file_mtime(&path).unwrap_or(0);

            let looks_unchanged = {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                file_looks_unchanged(&conn, sync_folder_id, &relative_str, mtime)
            };

            if looks_unchanged {
                unchanged += 1;
                sync_log(format!(
                    "file unchanged (quick) {}/{} — {}",
                    processed,
                    total.max(1),
                    name
                ));
                continue;
            }

            if show_ui {
                self.emit_progress(&SyncProgress {
                    phase: "syncing".into(),
                    processed,
                    total,
                    uploaded,
                    skipped,
                    unchanged,
                    errors,
                    current: processed,
                    current_file: name.clone(),
                    message: format_file_sync_message(&name),
                    show_in_ui: true,
                });
            }

            sync_log(format!(
                "file start {}/{} — {} ({} bytes)",
                processed,
                total.max(1),
                name,
                size
            ));

            let engine = Arc::clone(self);
            let root_clone = root.clone();
            let remote_root_clone = remote_root.clone();
            let path_clone = path.clone();
            let file_timeout = file_sync_timeout(size.max(0) as u64);

            let mut handle = tokio::spawn(async move {
                engine
                    .sync_one_file(
                        sync_folder_id,
                        &root_clone,
                        &path_clone,
                        &remote_root_clone,
                        processed,
                        total,
                        false,
                        show_ui,
                    )
                    .await
            });

            let sync_result = tokio::select! {
                res = &mut handle => {
                    match res {
                        Ok(Ok(outcome)) => Ok(outcome),
                        Ok(Err(e)) => Err(e),
                        Err(e) => Err(AppError::msg(e.to_string())),
                    }
                }
                _ = tokio::time::sleep(file_timeout) => {
                    handle.abort();
                    let _ = handle.await;
                    sync_log(format!("file timeout abort — {}", name));
                    Err(AppError::msg(format!("Timed out syncing {}", name)))
                }
            };

            match sync_result {
                Ok(FileSyncOutcome::Synced) => {
                    uploaded += 1;
                    sync_log(format!("file synced — {}", name));
                }
                Ok(FileSyncOutcome::Unchanged) => {
                    unchanged += 1;
                    sync_log(format!("file unchanged — {}", name));
                }
                Ok(FileSyncOutcome::Skipped) => {
                    skipped += 1;
                    sync_log(format!("file skipped — {}", name));
                }
                Ok(FileSyncOutcome::Failed) => {
                    errors += 1;
                    self.record_sync_failure(
                        sync_folder_id,
                        &relative_str,
                        &path.to_string_lossy(),
                        mtime,
                        "Sync failed",
                        false,
                    );
                    sync_log(format!("file failed — {}", name));
                }
                Err(e) => {
                    errors += 1;
                    let msg = e.to_string();
                    if show_ui {
                        self.emit_activity(&name, &msg, size, "error");
                    }
                    self.record_sync_failure(
                        sync_folder_id,
                        &relative_str,
                        &path.to_string_lossy(),
                        mtime,
                        &msg,
                        is_permanent_sync_error(&msg),
                    );
                    sync_log(format!("file error — {}: {}", name, e));
                }
            }

            sync_log(format!("file done {}/{} — next", processed, total.max(1)));
        }

        sync_log(format!(
            "scan folder done — uploaded={} skipped={} unchanged={} errors={}",
            uploaded, skipped, unchanged, errors
        ));

        Ok((uploaded, skipped, unchanged, errors))
    }

    pub async fn sync_file_path(&self, path: &Path) -> AppResult<()> {
        if self.is_paused() || self.is_initial_sync_running() || !path.is_file() {
            return Ok(());
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        if should_skip_file(file_name) {
            return Ok(());
        }

        let sync_folders = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            list_sync_folders(&conn)?
        };

        let _guard = self.sync_lock.lock().await;
        for sf in sync_folders {
            let root = PathBuf::from(&sf.local_path);
            if path.starts_with(&root) {
                let remote_root_id = self.ensure_sync_folder_remote(&sf).await?;
                let _ = self
                    .sync_one_file(
                        sf.id,
                        &root,
                        path,
                        &remote_root_id,
                        0,
                        0,
                        false,
                        true,
                    )
                    .await;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn sync_one_file(
        &self,
        sync_folder_id: i64,
        local_root: &Path,
        file_path: &Path,
        remote_root_id: &str,
        processed: u64,
        total: u64,
        _retried: bool,
        show_ui: bool,
    ) -> AppResult<FileSyncOutcome> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let relative_str = file_path
            .strip_prefix(local_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mtime = file_mtime(file_path).unwrap_or(0);

        match self
            .sync_one_file_with_retries(
                sync_folder_id,
                local_root,
                file_path,
                remote_root_id,
                processed,
                total,
                &file_name,
                show_ui,
            )
            .await
        {
            Ok(outcome) => Ok(outcome),
            Err(e) => {
                let msg = e.to_string();
                if show_ui {
                    self.emit_activity(&file_name, &msg, 0, "error");
                }
                self.record_sync_failure(
                    sync_folder_id,
                    &relative_str,
                    &file_path.to_string_lossy(),
                    mtime,
                    &msg,
                    is_permanent_sync_error(&msg),
                );
                Ok(FileSyncOutcome::Failed)
            }
        }
    }

    async fn sync_one_file_with_retries(
        &self,
        sync_folder_id: i64,
        local_root: &Path,
        file_path: &Path,
        remote_root_id: &str,
        processed: u64,
        total: u64,
        file_name: &str,
        show_ui: bool,
    ) -> AppResult<FileSyncOutcome> {
        let mut remote_root = remote_root_id.to_string();
        for attempt in 0..2u8 {
            match self
                .sync_one_file_attempt(
                    sync_folder_id,
                    local_root,
                    file_path,
                    &remote_root,
                    processed,
                    total,
                    file_name,
                    attempt > 0,
                    show_ui,
                )
                .await?
            {
                SyncAttemptResult::Done(outcome) => return Ok(outcome),
                SyncAttemptResult::RetryClearFileState => {
                    let relative_str = file_path
                        .strip_prefix(local_root)
                        .map_err(|e| AppError::msg(e.to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                    clear_sync_state_remote_file(&conn, sync_folder_id, &relative_str)?;
                }
                SyncAttemptResult::RetryClearRoot => {
                    let sf = {
                        let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                        list_sync_folders(&conn)?
                            .into_iter()
                            .find(|f| f.id == sync_folder_id)
                            .ok_or_else(|| AppError::msg("sync folder not found"))?
                    };
                    remote_root = self.ensure_sync_folder_remote(&sf).await?;
                }
                SyncAttemptResult::RetryClearFolderMapping(parent_relative) => {
                    let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                    clear_folder_mapping_prefix(&conn, sync_folder_id, &parent_relative)?;
                }
            }
        }
        Ok(FileSyncOutcome::Failed)
    }

    async fn sync_one_file_attempt(
        &self,
        sync_folder_id: i64,
        local_root: &Path,
        file_path: &Path,
        remote_root_id: &str,
        processed: u64,
        total: u64,
        file_name: &str,
        is_retry: bool,
        show_ui: bool,
    ) -> AppResult<SyncAttemptResult> {
        let relative = file_path
            .strip_prefix(local_root)
            .map_err(|e| AppError::msg(e.to_string()))?;
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        let size = file_size(file_path);
        let mtime = file_mtime(file_path).unwrap_or(0);

        let existing = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            get_sync_state(&conn, sync_folder_id, &relative_str)?
        };

        if file_looks_unchanged_from_state(existing.as_ref(), mtime) {
            return Ok(SyncAttemptResult::Done(FileSyncOutcome::Unchanged));
        }

        let base_progress = SyncProgress {
            phase: "syncing".into(),
            processed,
            total,
            uploaded: 0,
            skipped: 0,
            unchanged: 0,
            errors: 0,
            current: processed,
            current_file: file_name.to_string(),
            message: format_file_sync_message(file_name),
            show_in_ui: show_ui,
        };

        if show_ui {
            self.emit_activity(file_name, "Hashing…", size, "uploading");
        }
        sync_log(format!("hash start — {}", file_name));
        let hash = match hash_file_async(file_path, hash_timeout()).await {
            Ok(h) => {
                sync_log(format!("hash ok — {}", file_name));
                h
            }
            Err(e) => {
                let msg = e.to_string();
                let detail = if msg.contains("timed out") {
                    "Cannot read file (timeout)".to_string()
                } else {
                    format!("Cannot read file: {}", msg)
                };
                if show_ui {
                    self.emit_activity(file_name, &detail, 0, "skipped");
                }
                return Ok(SyncAttemptResult::Done(FileSyncOutcome::Skipped));
            }
        };

        if let Some((remote_id, old_hash, _, _)) = &existing {
            if old_hash.as_deref() == Some(&hash) {
                if let Ok(conn) = self.db.lock() {
                    let _ = upsert_sync_state(
                        &conn,
                        sync_folder_id,
                        &relative_str,
                        &file_path.to_string_lossy(),
                        remote_id.as_deref(),
                        Some(&hash),
                        Some(mtime),
                        None,
                        "synced",
                    );
                }
                return Ok(SyncAttemptResult::Done(FileSyncOutcome::Unchanged));
            }
            if let Some(rid) = remote_id.clone() {
                if show_ui {
                    self.emit_activity(file_name, "Updating…", size, "uploading");
                }
                let existing_key = {
                    let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                    get_file_key(&conn, &rid)?
                        .and_then(|k| crate::crypto::key_from_b64url(&k).ok())
                };
                let upload_result = self
                    .with_heartbeat(
                        SyncProgress {
                            message: format!("Uploading {} ({})…", file_name, format_size(size)),
                            ..base_progress.clone()
                        },
                        || self.api.update_file_content(&rid, file_path, file_name, existing_key),
                    )
                    .await;

                match upload_result {
                    Ok((rec, key)) => {
                        let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                        store_file_key(&conn, &rec.id, &key_to_b64url(&key))?;
                        upsert_sync_state(
                            &conn,
                            sync_folder_id,
                            &relative_str,
                            &file_path.to_string_lossy(),
                            Some(&rec.id),
                            Some(&hash),
                            Some(mtime),
                            Some(&rec.updated_at),
                            "synced",
                        )?;
                        if show_ui {
                            self.emit_activity_with_conn(
                                &conn,
                                file_name,
                                "Successfully uploaded",
                                rec.size,
                                "synced",
                            );
                        }
                        return Ok(SyncAttemptResult::Done(FileSyncOutcome::Synced));
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if is_stale_remote_ref(&msg) && !is_retry {
                            return Ok(SyncAttemptResult::RetryClearFileState);
                        }
                        if show_ui {
                            self.emit_activity(file_name, &msg, 0, "error");
                        }
                        self.record_sync_failure(
                            sync_folder_id,
                            &relative_str,
                            &file_path.to_string_lossy(),
                            mtime,
                            &msg,
                            is_permanent_sync_error(&msg),
                        );
                        return Ok(SyncAttemptResult::Done(FileSyncOutcome::Failed));
                    }
                }
            }
        }

        let parent_relative = Path::new(&relative_str)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        if show_ui {
            self.emit_activity(file_name, "Resolving folder…", size, "uploading");
        }
        let remote_folder_id = match tokio::time::timeout(
            FOLDER_TIMEOUT,
            self.ensure_remote_folder(sync_folder_id, remote_root_id, &parent_relative),
        )
        .await
        {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                let msg = e.to_string();
                if show_ui {
                    self.emit_activity(file_name, &msg, 0, "error");
                }
                self.record_sync_failure(
                    sync_folder_id,
                    &relative_str,
                    &file_path.to_string_lossy(),
                    mtime,
                    &msg,
                    false,
                );
                return Ok(SyncAttemptResult::Done(FileSyncOutcome::Failed));
            }
            Err(_) => {
                if show_ui {
                    self.emit_activity(file_name, "Folder resolution timed out", 0, "error");
                }
                self.record_sync_failure(
                    sync_folder_id,
                    &relative_str,
                    &file_path.to_string_lossy(),
                    mtime,
                    "Folder resolution timed out",
                    false,
                );
                return Ok(SyncAttemptResult::Done(FileSyncOutcome::Failed));
            }
        };

        if show_ui {
            self.emit_activity(file_name, "Uploading…", size, "uploading");
            if size >= LARGE_FILE_WARN_BYTES {
                self.emit_activity(
                    file_name,
                    &format!(
                        "Large file ({}) — upload may take several minutes",
                        format_size(size)
                    ),
                    size,
                    "uploading",
                );
            }
        }
        let upload_result = self
            .with_heartbeat(
                SyncProgress {
                    message: format!("Uploading {} ({})…", file_name, format_size(size)),
                    ..base_progress
                },
                || {
                    self.api
                        .upload_file(&self.db, file_path, file_name, &remote_folder_id)
                },
            )
            .await;

        match upload_result {
            Ok((rec, key)) => {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                store_file_key(&conn, &rec.id, &key_to_b64url(&key))?;
                upsert_sync_state(
                    &conn,
                    sync_folder_id,
                    &relative_str,
                    &file_path.to_string_lossy(),
                    Some(&rec.id),
                    Some(&hash),
                    Some(mtime),
                    Some(&rec.updated_at),
                    "synced",
                )?;
                if show_ui {
                    self.emit_activity_with_conn(
                        &conn,
                        file_name,
                        "Successfully uploaded",
                        rec.size,
                        "synced",
                    );
                }
                if crate::db::has_pending_key_upload(&conn, &rec.id).unwrap_or(false) {
                    let _ = self.app.emit(
                        "crypto-key-queued",
                        format!(
                            "Encryption not unlocked — {} may be unavailable in the browser until you sign in with your password",
                            file_name
                        ),
                    );
                }
                Ok(SyncAttemptResult::Done(FileSyncOutcome::Synced))
            }
            Err(e) => {
                let msg = e.to_string();
                if is_stale_remote_ref(&msg) && !is_retry {
                    if parent_relative.is_empty() {
                        return Ok(SyncAttemptResult::RetryClearRoot);
                    }
                    return Ok(SyncAttemptResult::RetryClearFolderMapping(
                        parent_relative,
                    ));
                }
                if show_ui {
                    self.emit_activity(file_name, &msg, 0, "error");
                }
                self.record_sync_failure(
                    sync_folder_id,
                    &relative_str,
                    &file_path.to_string_lossy(),
                    mtime,
                    &msg,
                    is_permanent_sync_error(&msg),
                );
                Ok(SyncAttemptResult::Done(FileSyncOutcome::Failed))
            }
        }
    }

    async fn ensure_remote_folder(
        &self,
        sync_folder_id: i64,
        remote_root_id: &str,
        relative_dir: &str,
    ) -> AppResult<String> {
        if relative_dir.is_empty() {
            return Ok(remote_root_id.to_string());
        }

        {
            let cached_id = {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                get_folder_mapping(&conn, sync_folder_id, relative_dir)?
            };
            if let Some(id) = cached_id {
                if self.remote_folder_exists(&id).await {
                    return Ok(id);
                }
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                delete_folder_mapping(&conn, sync_folder_id, relative_dir)?;
            }
        }

        let parts: Vec<&str> = relative_dir.split('/').filter(|p| !p.is_empty()).collect();
        let mut current_parent = remote_root_id.to_string();
        let mut built = String::new();

        for part in parts {
            if !built.is_empty() {
                built.push('/');
            }
            built.push_str(part);

            let cached_id = {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                get_folder_mapping(&conn, sync_folder_id, &built)?
            };

            if let Some(id) = cached_id {
                if self.remote_folder_exists(&id).await {
                    current_parent = id;
                    continue;
                }
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                delete_folder_mapping(&conn, sync_folder_id, &built)?;
            }

            let folder = self
                .api
                .create_or_resolve_folder(part, Some(&current_parent))
                .await?;
            {
                let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                set_folder_mapping(&conn, sync_folder_id, &built, &folder.id)?;
            }
            current_parent = folder.id;
        }

        Ok(current_parent)
    }

    pub async fn poll_remote(&self) -> AppResult<()> {
        if self.is_paused() || self.is_initial_sync_running() {
            return Ok(());
        }

        let sync_folders = {
            let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
            list_sync_folders(&conn)?
        };

        let mirror = mirror_dir()?;

        for sf in sync_folders {
            if let Err(e) = self.poll_folder_tree(&sf, &mirror).await {
                self.emit_activity("poll", &e.to_string(), 0, "error");
            }
        }

        if !self.is_paused() && !self.is_initial_sync_running() {
            self.set_status(SyncStatusKind::UpToDate, "Up to date");
        }
        Ok(())
    }

    async fn poll_folder_tree(
        &self,
        sf: &db::SyncFolderRow,
        mirror: &Path,
    ) -> AppResult<()> {
        let label = if sf.label.is_empty() {
            Path::new(&sf.local_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("folder")
                .to_string()
        } else {
            sf.label.clone()
        };

        let mut queue: Vec<(String, String)> = vec![(sf.remote_folder_id.clone(), String::new())];

        while let Some((folder_id, relative_dir)) = queue.pop() {
            let contents = self.api.get_folder_contents(&folder_id).await?;
            for sub in contents.folders {
                let sub_rel = if relative_dir.is_empty() {
                    sub.name.clone()
                } else {
                    format!("{}/{}", relative_dir, sub.name)
                };
                queue.push((sub.id, sub_rel));
            }

            for file in contents.files {
                let rel = if relative_dir.is_empty() {
                    file.name.clone()
                } else {
                    format!("{}/{}", relative_dir, file.name)
                };
                let rel_path = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
                let local_path = mirror.join(&label).join(&rel_path);

                if local_path.exists() {
                    continue;
                }

                self.emit_activity(&file.name, "Downloading to mirror…", file.size, "uploading");
                if let Some(parent) = local_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let key_b64url = {
                    let conn = self.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
                    get_file_key(&conn, &file.id)?
                };
                match self.api.download_file(&file.id, key_b64url.as_deref()).await {
                    Ok(bytes) => {
                        std::fs::write(&local_path, &bytes)?;
                        self.emit_activity(&file.name, "Downloaded to mirror", file.size, "synced");
                    }
                    Err(e) => {
                        self.emit_activity(
                            &file.name,
                            &e.to_string(),
                            0,
                            "skipped",
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn heartbeat_loop(&self) -> AppResult<()> {
        let id = {
            let guard = self.computer_id.read();
            guard.clone()
        };
        if let Some(id) = id {
            let _ = self.api.heartbeat(&id).await;
        }
        Ok(())
    }
}

fn is_stale_remote_ref(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("foreign key")
        || lower.contains("787")
        || lower.contains("not found")
        || lower.contains("folder not found")
}

fn should_skip_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == "desktop.ini" || lower == "thumbs.db" || lower == "ehthumbs.db" {
        return true;
    }
    if lower.ends_with(".tmp") || lower.ends_with(".temp") || lower.ends_with(".lnk") {
        return true;
    }
    if lower.starts_with('.') || lower.starts_with("~$") {
        return true;
    }
    false
}

fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files_incremental(root, |path| files.push(path));
    files.sort_by_key(|p| file_size(p));
    files
}

fn walk_files_incremental(root: &Path, mut on_file: impl FnMut(PathBuf)) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                on_file(path);
            }
        }
    }
}

fn file_hash(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn hash_file_async(path: &Path, timeout: Duration) -> AppResult<String> {
    let path = path.to_path_buf();
    blocking::run_blocking_with_timeout_async(timeout, move || file_hash(&path)).await
}

fn format_file_sync_message(name: &str) -> String {
    format!("Syncing {}…", name)
}

fn format_sync_progress(processed: u64, total: u64, name: &str) -> String {
    if total > 0 {
        let pct = (processed * 100) / total;
        format!("Processing {}/{} ({}%) — {}", processed, total, pct, name)
    } else {
        format!("Processing {} — {}", processed, name)
    }
}

fn file_looks_unchanged_from_state(
    existing: Option<&(Option<String>, Option<String>, Option<i64>, String)>,
    mtime: i64,
) -> bool {
    match existing {
        Some((remote_id, _, Some(old_mtime), status)) if *old_mtime == mtime => {
            if remote_id.as_ref().is_some_and(|id| !id.is_empty()) {
                return true;
            }
            status == "rejected" || status == "error"
        }
        _ => false,
    }
}

fn file_looks_unchanged(
    conn: &rusqlite::Connection,
    sync_folder_id: i64,
    relative_path: &str,
    mtime: i64,
) -> bool {
    match get_sync_state(conn, sync_folder_id, relative_path) {
        Ok(state) => file_looks_unchanged_from_state(state.as_ref(), mtime),
        Err(_) => false,
    }
}

fn is_permanent_sync_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("file type not allowed")
        || lower.contains("status 400")
        || lower.contains("(400)")
}

fn file_mtime(path: &Path) -> AppResult<i64> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified()?;
    Ok(modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64)
}

fn file_size(path: &Path) -> i64 {
    std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
}

fn format_size(bytes: i64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

pub fn initial_sync_complete(db: &DbHandle) -> bool {
    db.lock()
        .ok()
        .and_then(|c| config_get(&c, "initial_sync_complete").ok().flatten())
        .map(|v| v == "true")
        .unwrap_or(false)
}

pub fn has_pending_sync(db: &DbHandle) -> bool {
    !initial_sync_complete(db)
}
