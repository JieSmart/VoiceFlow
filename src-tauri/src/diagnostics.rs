use parking_lot::Mutex;
use serde_json::Value;
use std::{
    fs::{create_dir_all, OpenOptions},
    io::Write,
    path::PathBuf,
};

pub struct DiagnosticLog {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl DiagnosticLog {
    pub fn new(data_root: &std::path::Path) -> Self {
        Self {
            path: data_root.join("logs").join("yuliu.log"),
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn write(&self, level: &str, event: &str, session_id: Option<&str>, details: Value) {
        let _guard = self.write_lock.lock();
        let Some(parent) = self.path.parent() else {
            return;
        };
        if create_dir_all(parent).is_err() {
            return;
        }
        let entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "level": level,
            "event": event,
            "sessionId": session_id,
            "details": details,
        });
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}
