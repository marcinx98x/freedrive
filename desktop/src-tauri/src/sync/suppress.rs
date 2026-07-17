use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct WatcherSuppress {
    inner: Arc<RwLock<HashSet<PathBuf>>>,
}

impl WatcherSuppress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn is_suppressed(&self, path: &Path) -> bool {
        self.inner.read().contains(path)
    }

    pub fn run_suppressed<F: FnOnce()>(&self, path: &Path, f: F) {
        self.inner.write().insert(path.to_path_buf());
        f();
        let inner = Arc::clone(&self.inner);
        let p = path.to_path_buf();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            inner.write().remove(&p);
        });
    }
}

impl Default for WatcherSuppress {
    fn default() -> Self {
        Self::new()
    }
}
