use crate::auth_store::data_dir;
use crate::error::AppResult;
use std::fs::OpenOptions;
use std::io::Write;

pub fn sync_log(message: impl AsRef<str>) {
    let _ = sync_log_result(message);
}

pub fn sync_log_result(message: impl AsRef<str>) -> AppResult<()> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("sync.log");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let ts = chrono::Utc::now().to_rfc3339();
    writeln!(file, "[{ts}] {}", message.as_ref())?;
    Ok(())
}
