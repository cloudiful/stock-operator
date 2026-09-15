use anyhow::Result;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BackendStatus {
    pub backend: &'static str,
    pub target_process_name: String,
    pub target_pid: Option<u32>,
    pub target_found: bool,
    pub main_window_found: bool,
    pub version_low_detected: bool,
    pub blocking_popup: bool,
    pub mutations_allowed: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WinText {
    pub class: String,
    pub label: String,
    pub value: String,
    pub handle: isize,
}

pub trait Backend: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn status(&self) -> BackendStatus;
    fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<serde_json::Value>;
    fn read_texts(&self, class: &str) -> Result<Vec<WinText>>;
}
