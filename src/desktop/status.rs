use serde::Serialize;

use crate::ax::AccessibilityStatus;

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub server_running: bool,
    pub server_bind_addr: Option<String>,
    pub server_error: Option<String>,
    pub token_configured: bool,
    pub token_source: String,
    pub accessibility: AccessibilityStatusDto,
    pub restart_required: bool,
    pub restart_reasons: Vec<String>,
    pub instance_id: String,
    pub db_path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccessibilityStatusDto {
    pub api_enabled: bool,
    pub process_trusted: bool,
    pub target_process_name: String,
    pub target_bundle_id: String,
    pub target_pid: Option<i32>,
    pub target_found: bool,
    pub notes: Vec<String>,
}

impl From<AccessibilityStatus> for AccessibilityStatusDto {
    fn from(s: AccessibilityStatus) -> Self {
        Self {
            api_enabled: s.api_enabled,
            process_trusted: s.process_trusted,
            target_process_name: s.target_process_name,
            target_bundle_id: s.target_bundle_id,
            target_pid: s.target_pid,
            target_found: s.target_found,
            notes: s.notes,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TestConnectionResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub message: String,
    pub latency_ms: Option<u64>,
}

pub fn check_restart_required(
    storage: &crate::storage::Storage,
    running: &crate::config::OperatorConfig,
) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();

    let current_bind = storage
        .get_setting(crate::storage::settings::SETTING_BIND_ADDR)
        .ok()
        .flatten()
        .unwrap_or_else(|| running.bind_addr.to_string());
    if current_bind != running.bind_addr.to_string() {
        reasons.push("bind address requires restart".to_string());
    }

    let current_mcp = storage
        .get_setting(crate::storage::settings::SETTING_MCP_PATH)
        .ok()
        .flatten()
        .unwrap_or_else(|| running.mcp_path.clone());
    if current_mcp != running.mcp_path {
        reasons.push("MCP path requires restart".to_string());
    }

    let current_bundle = storage
        .get_setting(crate::storage::settings::SETTING_TARGET_BUNDLE_ID)
        .ok()
        .flatten()
        .unwrap_or_else(|| running.target_bundle_id.clone());
    if current_bundle != running.target_bundle_id {
        reasons.push("broker bundle identifier requires restart".to_string());
    }

    let current_process = storage
        .get_setting(crate::storage::settings::SETTING_TARGET_PROCESS_NAME)
        .ok()
        .flatten()
        .unwrap_or_else(|| running.target_process_name.clone());
    if current_process != running.target_process_name {
        reasons.push("broker process name requires restart".to_string());
    }

    let current_depth = storage
        .get_setting(crate::storage::settings::SETTING_MAX_DEPTH)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(running.max_depth);
    if current_depth != running.max_depth {
        reasons.push("traversal depth requires restart".to_string());
    }

    let current_nodes = storage
        .get_setting(crate::storage::settings::SETTING_MAX_NODES)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(running.max_nodes);
    if current_nodes != running.max_nodes {
        reasons.push("traversal limit requires restart".to_string());
    }

    let current_network_mode = storage
        .get_setting(crate::storage::settings::SETTING_NETWORK_MODE)
        .ok()
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| running.network_mode.to_string());
    if current_network_mode != running.network_mode.to_string() {
        reasons.push("network mode requires restart".to_string());
    }

    let required = !reasons.is_empty();
    (required, reasons)
}

#[cfg(test)]
mod tests {
    use super::check_restart_required;
    use crate::{config::OperatorConfig, storage::Storage};

    #[test]
    fn detects_no_restart_when_unchanged() {
        let storage = Storage::open_in_memory().unwrap();
        let config = OperatorConfig::from_env().unwrap();
        storage
            .seed_from_config(
                config.stock_service_url.as_deref(),
                &config.bind_addr.to_string(),
                &config.mcp_path,
                &config.target_bundle_id,
                &config.target_process_name,
                config.max_depth,
                config.max_nodes,
                &config.network_mode.to_string(),
            )
            .unwrap();
        let (required, reasons) = check_restart_required(&storage, &config);
        assert!(!required);
        assert!(reasons.is_empty());
    }

    #[test]
    fn detects_restart_on_bind_change() {
        let storage = Storage::open_in_memory().unwrap();
        let config = OperatorConfig::from_env().unwrap();
        storage
            .seed_from_config(
                None,
                "127.0.0.1:5190",
                "/mcp",
                &config.target_bundle_id,
                &config.target_process_name,
                6,
                300,
                "loopback",
            )
            .unwrap();
        storage.set_setting("bind_addr", "127.0.0.1:5191").unwrap();
        let (required, reasons) = check_restart_required(&storage, &config);
        assert!(required);
        assert!(reasons.iter().any(|r| r.contains("bind address")));
    }

    #[test]
    fn detects_restart_on_network_mode_change() {
        let storage = Storage::open_in_memory().unwrap();
        let config = OperatorConfig::from_env().unwrap();
        storage
            .seed_from_config(
                None,
                "127.0.0.1:5190",
                "/mcp",
                &config.target_bundle_id,
                &config.target_process_name,
                6,
                300,
                "loopback",
            )
            .unwrap();
        storage
            .set_setting("network_mode", "private-overlay")
            .unwrap();
        let (required, reasons) = check_restart_required(&storage, &config);
        assert!(required);
        assert!(reasons.iter().any(|r| r.contains("network mode")));
    }
}
