use anyhow::{Result, bail};

use crate::backend::{Backend, BackendStatus, WinText};

pub struct WinStub;

impl WinStub {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for WinStub {
    fn name(&self) -> &'static str {
        "win32-hs"
    }

    fn status(&self) -> BackendStatus {
        BackendStatus {
            backend: self.name(),
            target_process_name: crate::config::DEFAULT_TARGET_PROCESS_NAME.to_string(),
            target_pid: None,
            target_found: false,
            main_window_found: false,
            version_low_detected: false,
            blocking_popup: false,
            mutations_allowed: false,
            notes: vec!["win backend not implemented (task 1 stub)".to_string()],
        }
    }

    fn snapshot(&self, _max_depth: usize, _max_nodes: usize) -> Result<serde_json::Value> {
        bail!("win backend not implemented")
    }

    fn read_texts(&self, _class: &str) -> Result<Vec<WinText>> {
        bail!("win backend not implemented")
    }
}
