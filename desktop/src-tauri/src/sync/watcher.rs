use crate::error::AppResult;
use crate::sync::engine::SyncEngine;
use notify::EventKind;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct WatcherHandle {
    _debouncer: notify_debouncer_full::Debouncer<
        notify::RecommendedWatcher,
        notify_debouncer_full::FileIdMap,
    >,
}

impl WatcherHandle {
    pub fn start(paths: Vec<PathBuf>, engine: Arc<SyncEngine>) -> AppResult<Self> {
        let engine_clone = engine.clone();
        let mut debouncer = new_debouncer(
            Duration::from_secs(1),
            None,
            move |result: DebounceEventResult| {
                if let Ok(events) = result {
                    for debounced in events {
                        let kind = debounced.event.kind;
                        for path in &debounced.event.paths {
                            if matches!(kind, EventKind::Remove(_)) {
                                engine_clone.enqueue_file_removed(path.clone());
                                continue;
                            }
                            if path.is_file() {
                                engine_clone.enqueue_file_path(path.clone());
                            }
                        }
                    }
                }
            },
        )
        .map_err(|e| crate::error::AppError::msg(e.to_string()))?;

        for path in paths {
            if path.exists() {
                debouncer
                    .watch(&path, notify::RecursiveMode::Recursive)
                    .map_err(|e| crate::error::AppError::msg(e.to_string()))?;
            }
        }

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
