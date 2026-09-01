use serde::{Deserialize, Serialize};

use chrono::Utc;
use rusqlite::params;

use crate::{
    desktop::validation::{
        validate_bind_addr_for_mode, validate_bundle_id, validate_max_depth, validate_max_nodes,
        validate_mcp_path, validate_network_mode, validate_process_name,
        validate_stock_service_url,
    },
    storage::Storage,
    storage::settings as storage_keys,
};

fn default_network_mode() -> String {
    "loopback".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicSettings {
    pub stock_service_url: Option<String>,
    pub bind_addr: String,
    pub mcp_path: String,
    pub target_bundle_id: String,
    pub target_process_name: String,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub instance_id: String,
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveSettingsRequest {
    pub stock_service_url: Option<String>,
    pub bind_addr: String,
    pub mcp_path: String,
    pub target_bundle_id: String,
    pub target_process_name: String,
    pub max_depth: usize,
    pub max_nodes: usize,
    #[serde(default = "default_network_mode")]
    pub network_mode: String,
    #[serde(default)]
    pub private_overlay_ack: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveSettingsResponse {
    pub settings: PublicSettings,
    pub restart_required: bool,
    pub restart_reasons: Vec<String>,
    pub message: String,
}

pub fn load_public_settings(storage: &Storage) -> Result<PublicSettings, String> {
    let stock_service_url = storage
        .get_setting(storage_keys::SETTING_STOCK_SERVICE_URL)
        .map_err(|e| format!("failed to load stock service url: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let bind_addr = storage
        .get_setting(storage_keys::SETTING_BIND_ADDR)
        .map_err(|e| format!("failed to load bind addr: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "127.0.0.1:5190".to_string());

    let mcp_path = storage
        .get_setting(storage_keys::SETTING_MCP_PATH)
        .map_err(|e| format!("failed to load mcp path: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/mcp".to_string());

    let target_bundle_id = storage
        .get_setting(storage_keys::SETTING_TARGET_BUNDLE_ID)
        .map_err(|e| format!("failed to load bundle id: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "com.citics.mac.tdx".to_string());

    let target_process_name = storage
        .get_setting(storage_keys::SETTING_TARGET_PROCESS_NAME)
        .map_err(|e| format!("failed to load process name: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "中信证券网上交易".to_string());

    let max_depth = storage
        .get_setting(storage_keys::SETTING_MAX_DEPTH)
        .map_err(|e| format!("failed to load max_depth: {e}"))?
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(6);

    let max_nodes = storage
        .get_setting(storage_keys::SETTING_MAX_NODES)
        .map_err(|e| format!("failed to load max_nodes: {e}"))?
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(300);

    let instance_id = storage
        .get_setting(storage_keys::SETTING_INSTANCE_ID)
        .map_err(|e| format!("failed to load instance id: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let network_mode = storage
        .get_setting(storage_keys::SETTING_NETWORK_MODE)
        .map_err(|e| format!("failed to load network_mode: {e}"))?
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(default_network_mode);
    let network_mode =
        validate_network_mode(&network_mode).unwrap_or_else(|_| default_network_mode());

    Ok(PublicSettings {
        stock_service_url,
        bind_addr,
        mcp_path,
        target_bundle_id,
        target_process_name,
        max_depth,
        max_nodes,
        instance_id,
        network_mode,
    })
}

pub fn save_public_settings(
    storage: &Storage,
    request: SaveSettingsRequest,
    running_bind: &str,
    running_mcp: &str,
    running_bundle: &str,
    running_process: &str,
    running_depth: usize,
    running_nodes: usize,
    running_network_mode: &str,
) -> Result<SaveSettingsResponse, String> {
    let network_mode =
        validate_network_mode(&request.network_mode).map_err(|e| format!("network_mode: {e}"))?;
    // Private overlay requires explicit acknowledgement
    if network_mode == "private-overlay" && !request.private_overlay_ack {
        return Err(
            "private-overlay mode requires explicit acknowledgement that bearer auth and private/encrypted transport are required; check the safety box".to_string(),
        );
    }
    let bind_addr = validate_bind_addr_for_mode(&request.bind_addr, &network_mode)
        .map_err(|e| format!("bind_addr: {e}"))?;
    // Also guard non-loopback with loopback mode already handled by validate, but ensure ack for any non-loopback attempt
    if !bind_addr.ip().is_loopback() && network_mode != "private-overlay" {
        return Err(
            "non-loopback bind requires network_mode 'private-overlay' with acknowledgement"
                .to_string(),
        );
    }
    if !bind_addr.ip().is_loopback() && !request.private_overlay_ack {
        return Err("non-loopback bind requires private_overlay_ack=true".to_string());
    }
    let mcp_path = validate_mcp_path(&request.mcp_path).map_err(|e| format!("mcp_path: {e}"))?;
    let bundle_id = validate_bundle_id(&request.target_bundle_id)
        .map_err(|e| format!("target_bundle_id: {e}"))?;
    let process_name = validate_process_name(&request.target_process_name)
        .map_err(|e| format!("target_process_name: {e}"))?;
    let max_depth = validate_max_depth(request.max_depth).map_err(|e| format!("max_depth: {e}"))?;
    let max_nodes = validate_max_nodes(request.max_nodes).map_err(|e| format!("max_nodes: {e}"))?;
    let stock_url = validate_stock_service_url(&request.stock_service_url)
        .map_err(|e| format!("stock_service_url: {e}"))?;

    {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let conn = storage.conn.lock().expect("storage mutex poisoned");
        conn.execute("BEGIN IMMEDIATE", [])
            .map_err(|e| format!("failed to begin transaction: {e}"))?;
        let tx_result: Result<(), String> = (|| {
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_BIND_ADDR, bind_addr.to_string(), now],
            )
            .map_err(|e| format!("failed to save bind_addr: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_MCP_PATH, mcp_path, now],
            )
            .map_err(|e| format!("failed to save mcp_path: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_TARGET_BUNDLE_ID, bundle_id, now],
            )
            .map_err(|e| format!("failed to save bundle id: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_TARGET_PROCESS_NAME, process_name, now],
            )
            .map_err(|e| format!("failed to save process name: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_MAX_DEPTH, max_depth.to_string(), now],
            )
            .map_err(|e| format!("failed to save max_depth: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_MAX_NODES, max_nodes.to_string(), now],
            )
            .map_err(|e| format!("failed to save max_nodes: {e}"))?;
            conn.execute(
                "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![storage_keys::SETTING_NETWORK_MODE, network_mode, now],
            )
            .map_err(|e| format!("failed to save network_mode: {e}"))?;
            if let Some(url) = stock_url.clone() {
                conn.execute(
                    "INSERT INTO operator_settings (key, value, updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                    params![storage_keys::SETTING_STOCK_SERVICE_URL, url, now],
                )
                .map_err(|e| format!("failed to save stock url: {e}"))?;
            } else {
                conn.execute(
                    "DELETE FROM operator_settings WHERE key = ?1",
                    params![storage_keys::SETTING_STOCK_SERVICE_URL],
                )
                .map_err(|e| format!("failed to clear stock url: {e}"))?;
            }
            Ok(())
        })();
        if let Err(e) = tx_result {
            let _ = conn.execute("ROLLBACK", []);
            return Err(e);
        }
        conn.execute("COMMIT", [])
            .map_err(|e| format!("failed to commit transaction: {e}"))?;
    }

    let mut reasons = Vec::new();
    if bind_addr.to_string() != running_bind {
        reasons.push("bind address requires restart".to_string());
    }
    if mcp_path != running_mcp {
        reasons.push("MCP path requires restart".to_string());
    }
    if bundle_id != running_bundle {
        reasons.push("broker bundle identifier requires restart".to_string());
    }
    if process_name != running_process {
        reasons.push("broker process name requires restart".to_string());
    }
    if max_depth != running_depth {
        reasons.push("traversal depth requires restart".to_string());
    }
    if max_nodes != running_nodes {
        reasons.push("traversal limit requires restart".to_string());
    }
    if network_mode != running_network_mode {
        reasons.push("network mode requires restart".to_string());
    }
    let restart_required = !reasons.is_empty();
    let settings = load_public_settings(storage)?;
    let message = if restart_required {
        "settings saved; restart required for some changes".to_string()
    } else {
        "settings saved".to_string()
    };
    Ok(SaveSettingsResponse {
        settings,
        restart_required,
        restart_reasons: reasons,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::{SaveSettingsRequest, load_public_settings, save_public_settings};
    use crate::storage::Storage;

    fn sample_request() -> SaveSettingsRequest {
        SaveSettingsRequest {
            stock_service_url: Some("http://localhost:3000".to_string()),
            bind_addr: "127.0.0.1:5190".to_string(),
            mcp_path: "/mcp".to_string(),
            target_bundle_id: "com.citics.mac.tdx".to_string(),
            target_process_name: "中信证券网上交易".to_string(),
            max_depth: 6,
            max_nodes: 300,
            network_mode: "loopback".to_string(),
            private_overlay_ack: false,
        }
    }
    fn running() -> (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        usize,
        usize,
        &'static str,
    ) {
        (
            "127.0.0.1:5190",
            "/mcp",
            "com.citics.mac.tdx",
            "中信证券网上交易",
            6,
            300,
            "loopback",
        )
    }

    #[test]
    fn round_trips_settings() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let req = sample_request();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let resp = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        assert!(!resp.restart_required);
        let loaded = load_public_settings(&storage).unwrap();
        assert_eq!(loaded.bind_addr, "127.0.0.1:5190");
        assert_eq!(loaded.mcp_path, "/mcp");
        assert_eq!(loaded.stock_service_url.unwrap(), "http://localhost:3000");
        assert_eq!(loaded.network_mode, "loopback");
    }

    #[test]
    fn rejects_non_loopback_without_private_mode() {
        let storage = Storage::open_in_memory().unwrap();
        let mut req = sample_request();
        req.bind_addr = "0.0.0.0:5190".to_string();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let err = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap_err();
        assert!(err.contains("bind_addr"));
    }

    #[test]
    fn private_overlay_requires_ack() {
        let storage = Storage::open_in_memory().unwrap();
        let mut req = sample_request();
        req.network_mode = "private-overlay".to_string();
        req.private_overlay_ack = false;
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let err = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap_err();
        assert!(err.contains("acknowledgement"));
    }

    #[test]
    fn private_overlay_with_ack_allows_private_bind() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let mut req = sample_request();
        req.network_mode = "private-overlay".to_string();
        req.private_overlay_ack = true;
        req.bind_addr = "192.168.1.10:5190".to_string();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let resp = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        assert!(resp.restart_required);
        let loaded = load_public_settings(&storage).unwrap();
        assert_eq!(loaded.network_mode, "private-overlay");
        assert_eq!(loaded.bind_addr, "192.168.1.10:5190");
    }

    #[test]
    fn rejects_public_even_with_private_mode() {
        let storage = Storage::open_in_memory().unwrap();
        let mut req = sample_request();
        req.network_mode = "private-overlay".to_string();
        req.private_overlay_ack = true;
        req.bind_addr = "8.8.8.8:5190".to_string();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let err = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap_err();
        assert!(err.contains("private"));
    }

    #[test]
    fn detects_restart_required() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let mut req = sample_request();
        req.bind_addr = "127.0.0.1:5191".to_string();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let resp = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        assert!(resp.restart_required);
        assert!(
            resp.restart_reasons
                .iter()
                .any(|r| r.contains("bind address"))
        );
    }

    #[test]
    fn detects_network_mode_restart() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let mut req = sample_request();
        req.network_mode = "private-overlay".to_string();
        req.private_overlay_ack = true;
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let resp = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        assert!(resp.restart_required);
        assert!(
            resp.restart_reasons
                .iter()
                .any(|r| r.contains("network mode"))
        );
    }

    #[test]
    fn rejects_invalid_url() {
        let storage = Storage::open_in_memory().unwrap();
        let mut req = sample_request();
        req.stock_service_url = Some("ftp://example.com".to_string());
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        let err = save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap_err();
        assert!(err.contains("stock_service_url"));
    }

    #[test]
    fn plain_storage_does_not_contain_token() {
        let storage = Storage::open_in_memory().unwrap();
        let req = sample_request();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        let conn = storage.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM operator_settings WHERE key LIKE '%token%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn clearing_url_removes_setting() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let req = sample_request();
        let (rb, rm, rbu, rp, rd, rn, rnm) = running();
        save_public_settings(&storage, req, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        let mut clear = sample_request();
        clear.stock_service_url = None;
        save_public_settings(&storage, clear, rb, rm, rbu, rp, rd, rn, rnm).unwrap();
        let loaded = load_public_settings(&storage).unwrap();
        assert!(loaded.stock_service_url.is_none());
    }
}
