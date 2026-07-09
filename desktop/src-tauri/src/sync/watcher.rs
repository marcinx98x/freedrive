use crate::error::AppResult;

use crate::sync::engine::SyncEngine;

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

    pub fn start(

        paths: Vec<PathBuf>,

        engine: Arc<SyncEngine>,

    ) -> AppResult<Self> {

        let engine_clone = engine.clone();

        let mut debouncer = new_debouncer(

            Duration::from_secs(2),

            None,

            move |result: DebounceEventResult| {

                if let Ok(events) = result {

                    for event in events {

                        for path in &event.paths {

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



        Ok(Self { _debouncer: debouncer })

    }

}


