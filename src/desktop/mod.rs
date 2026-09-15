pub mod keychain;
pub mod server;
pub mod settings;
pub mod status;
pub mod validation;

pub use server::{AppState, spawn_background_server};

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(
    state: tauri::State<'_, AppState>,
) -> Result<settings::PublicSettings, String> {
    settings::load_public_settings(state.storage.as_ref())
}

#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, AppState>,
    request: settings::SaveSettingsRequest,
) -> Result<settings::SaveSettingsResponse, String> {
    let cfg_snapshot = state.config.lock().await.clone();
    let resp = settings::save_public_settings(
        state.storage.as_ref(),
        request,
        &cfg_snapshot.bind_addr.to_string(),
        &cfg_snapshot.mcp_path,
        &cfg_snapshot.target_bundle_id,
        &cfg_snapshot.target_process_name,
        cfg_snapshot.max_depth,
        cfg_snapshot.max_nodes,
        &cfg_snapshot.network_mode.to_string(),
    )?;
    if let Ok(reloaded) =
        crate::config::OperatorConfig::from_env_with_storage(state.storage.as_ref())
    {
        let mut guard = state.config.lock().await;
        *guard = reloaded;
    }
    Ok(resp)
}

#[tauri::command]
pub async fn get_runtime_status(
    state: tauri::State<'_, AppState>,
) -> Result<status::RuntimeStatus, String> {
    let cfg = state.config.lock().await.clone();
    let server = state.server_state.lock().await.clone();
    let token = keychain::token_status();
    let accessibility = state.service.backend.status();
    let (restart_required, restart_reasons) =
        status::check_restart_required(state.storage.as_ref(), &state.initial_config);
    let instance_id = state
        .storage
        .get_setting(crate::storage::settings::SETTING_INSTANCE_ID)
        .ok()
        .flatten()
        .or_else(|| cfg.instance_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(status::RuntimeStatus {
        server_running: server.running,
        server_bind_addr: server.bind_addr,
        server_error: server.error,
        token_configured: token.configured,
        token_source: token.source,
        accessibility: accessibility.into(),
        restart_required,
        restart_reasons,
        instance_id,
        db_path: cfg.db_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn get_token_status(
    _state: tauri::State<'_, AppState>,
) -> Result<keychain::TokenStatus, String> {
    Ok(keychain::token_status())
}

fn env_token_present() -> bool {
    std::env::var("STOCK_OPERATOR_AUTH_TOKEN")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn save_token(
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<keychain::TokenStatus, String> {
    keychain::save_keychain_token(&token)?;
    if env_token_present() {
        return Ok(keychain::token_status());
    }
    server::stop_server_pub(state.inner()).await;
    let status = keychain::token_status();
    if status.configured {
        if let Err(e) = server::start_server_pub(state.inner()).await {
            let mut srv = state.inner().server_state.lock().await;
            srv.running = false;
            srv.error = Some(e);
        }
    }
    Ok(keychain::token_status())
}

#[tauri::command]
pub async fn clear_token(
    state: tauri::State<'_, AppState>,
) -> Result<keychain::TokenStatus, String> {
    keychain::clear_keychain_token()?;
    if env_token_present() {
        return Ok(keychain::token_status());
    }
    server::stop_server_pub(state.inner()).await;
    {
        let mut srv = state.inner().server_state.lock().await;
        srv.running = false;
        srv.bind_addr = None;
        srv.error = Some("token cleared".to_string());
    }
    Ok(keychain::token_status())
}

#[tauri::command]
pub async fn test_stock_service_url(url: String) -> Result<status::TestConnectionResult, String> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return Err("URL is empty".to_string());
    }
    let parsed = url::Url::parse(&trimmed).map_err(|e| format!("invalid URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("URL must be http or https".to_string());
    }
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build client: {e}"))?;
    match client.get(trimmed.clone()).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_millis() as u64;
            let status = resp.status().as_u16();
            let ok = resp.status().is_success() || resp.status().is_redirection();
            Ok(status::TestConnectionResult {
                ok,
                status: Some(status),
                message: if ok {
                    format!("reachable (HTTP {status})")
                } else {
                    format!("returned HTTP {status}")
                },
                latency_ms: Some(latency),
            })
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(status::TestConnectionResult {
                ok: false,
                status: None,
                message: format!("connection failed: {e}"),
                latency_ms: Some(latency),
            })
        }
    }
}

#[tauri::command]
pub async fn list_operations(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
    kind: Option<String>,
    state_filter: Option<String>,
) -> Result<crate::operator_types::OperationHistoryResponse, String> {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let offset = offset.unwrap_or(0);
    let kind = kind.and_then(|k| match k.as_str() {
        "submit_order" => Some(crate::storage::LiveOperationKind::SubmitOrder),
        "cancel_order" => Some(crate::storage::LiveOperationKind::CancelOrder),
        _ => None,
    });
    let state_filter = state_filter.and_then(|s| match s.as_str() {
        "confirmation_opened" => Some(crate::storage::LiveOperationState::ConfirmationOpened),
        "confirming" => Some(crate::storage::LiveOperationState::Confirming),
        "confirmed" => Some(crate::storage::LiveOperationState::Confirmed),
        "unknown" => Some(crate::storage::LiveOperationState::Unknown),
        "expired" => Some(crate::storage::LiveOperationState::Expired),
        "aborted" => Some(crate::storage::LiveOperationState::Aborted),
        _ => None,
    });
    state
        .service
        .list_operations(limit, offset, kind, state_filter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_audit_events(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
    operation_id: Option<String>,
) -> Result<crate::operator_types::AuditHistoryResponse, String> {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let offset = offset.unwrap_or(0);
    let op_id = operation_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    state
        .service
        .list_audit_events(limit, offset, op_id)
        .map_err(|e| e.to_string())
}

/// Supervised desktop-only recovery for stale operations.
/// Only `unknown`/`expired` are accepted; it never submits/confirms the broker dialog.
/// The operator must have verified the broker confirmation dialog is closed, then call this
/// to write a `stale_resolved` audit and move the operation to terminal `aborted` so future
/// prepares are unblocked. Clears any in-memory confirmation token.
#[tauri::command]
pub async fn resolve_stale_operation(
    state: tauri::State<'_, AppState>,
    operation_id: String,
) -> Result<serde_json::Value, String> {
    let id = operation_id.trim().to_string();
    if id.is_empty() {
        return Err("operation_id is required".to_string());
    }
    let op = state
        .storage
        .get_operation(&id)
        .map_err(|e| format!("storage error: {e}"))?
        .ok_or_else(|| "operation not found".to_string())?;
    match op.state {
        crate::storage::LiveOperationState::Unknown
        | crate::storage::LiveOperationState::Expired => {}
        _ => {
            return Err(format!(
                "only unknown or expired operations can be resolved (current state: {:?})",
                op.state
            ));
        }
    }
    let detail = serde_json::json!({
        "acknowledged": true,
        "reason": "desktop_stale_resolve_dialog_closed",
        "previous_state": format!("{:?}", op.state).to_lowercase(),
        "resolved_by": "desktop:stale_resolve"
    });
    state
        .storage
        .transition_to_stale_resolved(&id, Some("desktop:stale_resolve"), &detail)
        .map_err(|e| e.to_string())?;
    if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
        state.service.confirmation_tokens.lock().await.remove(&uuid);
    }
    let updated = state
        .storage
        .get_operation(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "operation disappeared after resolve".to_string())?;
    Ok(serde_json::json!({
        "operation_id": updated.id,
        "previous_state": format!("{:?}", op.state).to_lowercase(),
        "state": format!("{:?}", updated.state).to_lowercase(),
        "message": "stale operation resolved; new prepares will no longer be blocked"
    }))
}

#[cfg(test)]
mod tests {
    use super::settings::{SaveSettingsRequest, load_public_settings, save_public_settings};
    use crate::storage::Storage;

    #[test]
    fn desktop_round_trip_preserves_restart_flag() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let req = SaveSettingsRequest {
            stock_service_url: Some("https://example.com".to_string()),
            bind_addr: "127.0.0.1:5190".to_string(),
            mcp_path: "/mcp".to_string(),
            target_bundle_id: "com.citics.mac.tdx".to_string(),
            target_process_name: "中信证券网上交易".to_string(),
            max_depth: 6,
            max_nodes: 300,
            network_mode: "loopback".to_string(),
            private_overlay_ack: false,
        };
        let resp = save_public_settings(
            &storage,
            req,
            "127.0.0.1:5190",
            "/mcp",
            "com.citics.mac.tdx",
            "中信证券网上交易",
            6,
            300,
            "loopback",
        )
        .unwrap();
        assert!(!resp.restart_required);
        let loaded = load_public_settings(&storage).unwrap();
        let json = serde_json::to_string(&loaded).unwrap();
        assert!(!json.to_lowercase().contains("token"));
        assert!(!json.to_lowercase().contains("bearer"));
    }

    #[test]
    fn repeated_save_no_restart_when_unchanged() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        let req1 = SaveSettingsRequest {
            stock_service_url: None,
            bind_addr: "127.0.0.1:5191".to_string(),
            mcp_path: "/mcp".to_string(),
            target_bundle_id: "com.citics.mac.tdx".to_string(),
            target_process_name: "中信证券网上交易".to_string(),
            max_depth: 6,
            max_nodes: 300,
            network_mode: "loopback".to_string(),
            private_overlay_ack: false,
        };
        let resp1 = save_public_settings(
            &storage,
            req1.clone(),
            "127.0.0.1:5190",
            "/mcp",
            "com.citics.mac.tdx",
            "中信证券网上交易",
            6,
            300,
            "loopback",
        )
        .unwrap();
        assert!(resp1.restart_required);
        let resp2 = save_public_settings(
            &storage,
            req1,
            "127.0.0.1:5191",
            "/mcp",
            "com.citics.mac.tdx",
            "中信证券网上交易",
            6,
            300,
            "loopback",
        )
        .unwrap();
        assert!(
            !resp2.restart_required,
            "second identical save should not require restart"
        );
    }
}
