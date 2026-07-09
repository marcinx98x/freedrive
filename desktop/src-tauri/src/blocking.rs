use crate::error::{AppError, AppResult};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Runs `f` on a detached OS thread. Returns after `timeout` even if `f` is still running
/// (the thread is orphaned). Does not use tokio's blocking pool for `f`.
pub async fn run_blocking_with_timeout_async<T: Send + 'static>(
    timeout: Duration,
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    let (tx, rx) = mpsc::sync_channel::<AppResult<T>>(1);
    thread::spawn(move || {
        let _ = tx.send(f());
    });

    tokio::task::spawn_blocking(move || match rx.recv_timeout(timeout) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(AppError::msg("operation timed out")),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(AppError::msg("blocking task failed")),
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

pub fn hash_timeout() -> Duration {
    Duration::from_secs(60)
}

pub fn upload_prep_timeout(file_size: u64) -> Duration {
    let size_mb = file_size / (1024 * 1024);
    Duration::from_secs(120).max(Duration::from_secs(30 + size_mb.saturating_mul(10)))
}

pub fn upload_http_timeout(file_size: u64) -> Duration {
    let size_mb = file_size / (1024 * 1024);
    Duration::from_secs(120).max(Duration::from_secs(60 + size_mb.saturating_mul(15)))
}

/// Total per-file budget: encrypt prep + HTTP upload + margin.
pub fn file_sync_timeout(file_size: u64) -> Duration {
    upload_prep_timeout(file_size) + upload_http_timeout(file_size) + Duration::from_secs(30)
}
