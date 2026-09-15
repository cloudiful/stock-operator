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

/// Account funds parsed from the terminal's `Static` labels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FundsView {
    pub available: Option<f64>,
    pub market_value: Option<f64>,
    pub total_assets: Option<f64>,
    pub frozen: Option<f64>,
    pub withdrawable: Option<f64>,
    /// True when a required amount is missing or still shows a placeholder.
    pub stale: bool,
}

/// One five-level quote row (price column then volume column).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteLevel {
    pub label: String,
    pub price: Option<f64>,
    pub volume: Option<u64>,
}

/// Five-level quotes plus the latest/limit scalars for the selected security.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteView {
    pub levels: Vec<QuoteLevel>,
    pub latest: Option<f64>,
    pub change_pct: Option<f64>,
    pub limit_up: Option<f64>,
    pub limit_down: Option<f64>,
    /// True when any price still shows the `888.888`/`88888888` placeholder.
    pub stale: bool,
}

pub trait Backend: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn status(&self) -> BackendStatus;
    fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<serde_json::Value>;
    fn read_texts(&self, class: &str) -> Result<Vec<WinText>>;
}
