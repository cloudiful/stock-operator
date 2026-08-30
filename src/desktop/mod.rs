#[cfg(target_os = "macos")]
pub mod keychain;
#[cfg(target_os = "macos")]
pub mod settings;
#[cfg(target_os = "macos")]
pub mod status;
#[cfg(target_os = "macos")]
pub mod validation;

#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tokio::{sync::Mutex, task::JoinHandle};

#[cfg(target_os = "macos")]
use crate::{config::OperatorConfig, operator_service::OperatorService, storage::Storage};

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub struct ServerState {
    pub running: bool,
    pub bind_addr: Option<String>,
    pub error: Option<String>,
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub service: OperatorService,
    pub config: Arc<Mutex<OperatorConfig>>,
    pub initial_config: OperatorConfig,
    pub server_state: Arc<Mutex<ServerState>>,
    pub server_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[cfg(target_os = "macos")]
impl AppState {
    pub fn new(storage: Arc<Storage>, service: OperatorService, config: OperatorConfig) -> Self {
        let initial = config.clone();
        Self {
            storage,
            service,
            config: Arc::new(Mutex::new(config)),
            initial_config: initial,
            server_state: Arc::new(Mutex::new(ServerState {
                running: false,
                bind_addr: None,
                error: None,
            })),
            server_handle: Arc::new(Mutex::new(None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn get_settings(
    state: tauri::State<'_, AppState>,
) -> Result<settings::PublicSettings, String> {
    settings::load_public_settings(state.storage.as_ref())
}

#[cfg(target_os = "macos")]
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
    // Update effective config so repeated saves correctly report no restart
    if let Ok(reloaded) =
        crate::config::OperatorConfig::from_env_with_storage(state.storage.as_ref())
    {
        let mut guard = state.config.lock().await;
        *guard = reloaded;
    }
    Ok(resp)
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn get_runtime_status(
    state: tauri::State<'_, AppState>,
) -> Result<status::RuntimeStatus, String> {
    let cfg = state.config.lock().await.clone();
    let server = state.server_state.lock().await.clone();
    let token = keychain::token_status();
    let inspector = crate::ax::AccessibilityInspector::new(state.initial_config.clone());
    let accessibility = inspector.status();

    // Compare active runtime (initial_config) vs persisted storage for restart
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

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn get_token_status(
    _state: tauri::State<'_, AppState>,
) -> Result<keychain::TokenStatus, String> {
    Ok(keychain::token_status())
}

#[cfg(target_os = "macos")]
fn env_token_present() -> bool {
    std::env::var("STOCK_OPERATOR_AUTH_TOKEN")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn save_token(
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<keychain::TokenStatus, String> {
    keychain::save_keychain_token(&token)?;
    if env_token_present() {
        // Env takes precedence; keychain saved for future but server keeps env token
        return Ok(keychain::token_status());
    }
    // Rotate server to pick up new keychain token
    stop_server(state.inner()).await;
    // Start only if token now configured
    let status = keychain::token_status();
    if status.configured {
        if let Err(e) = start_server(state.inner()).await {
            let mut srv = state.inner().server_state.lock().await;
            srv.running = false;
            srv.error = Some(e);
        }
    }
    Ok(keychain::token_status())
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn clear_token(
    state: tauri::State<'_, AppState>,
) -> Result<keychain::TokenStatus, String> {
    keychain::clear_keychain_token()?;
    if env_token_present() {
        // Env token still provides auth; keep server running
        return Ok(keychain::token_status());
    }
    // No env token and keychain cleared -> stop server, it was serving old token
    stop_server(state.inner()).await;
    {
        let mut srv = state.inner().server_state.lock().await;
        srv.running = false;
        srv.bind_addr = None;
        srv.error = Some("token cleared".to_string());
    }
    Ok(keychain::token_status())
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

// ---------------------------------------------------------------------------
// Background server startup helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
async fn stop_server(app_state: &AppState) {
    // Abort handle if present and wait briefly to release port
    let handle_opt = { app_state.server_handle.lock().await.take() };
    if let Some(handle) = handle_opt {
        handle.abort();
        // Give OS time to release the socket; poll handle to avoid stale error overwrite
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let mut srv = app_state.server_state.lock().await;
    srv.running = false;
    // keep bind_addr/error for caller to set appropriately
}

#[cfg(target_os = "macos")]
async fn start_server(app_state: &AppState) -> Result<(), String> {
    // Resolve effective token (env or keychain)
    let (token_opt, _) = keychain::resolve_token();
    let effective = {
        let cfg = app_state.config.lock().await;
        if let Some(env_tok) = cfg.auth_token.clone().filter(|v| !v.trim().is_empty()) {
            Some(env_tok)
        } else {
            token_opt
        }
    };
    let Some(effective_token) = effective.filter(|v| !v.trim().is_empty()) else {
        let mut srv = app_state.server_state.lock().await;
        srv.running = false;
        srv.bind_addr = None;
        srv.error = Some("token not configured".to_string());
        return Err("token not configured".to_string());
    };
    // Build effective config for server
    let mut cfg = app_state.config.lock().await.clone();
    cfg.auth_token = Some(effective_token);
    let bind = cfg.bind_addr;
    // Build router and bind synchronously before marking running
    let inspector = crate::ax::AccessibilityInspector::new(cfg.clone());
    let router = crate::mcp::build_router(&cfg, inspector, app_state.service.clone())
        .map_err(|e| format!("failed to build router: {e}"))?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| format!("failed to bind {}: {e}", bind))?;

    let bind_str = bind.to_string();
    {
        let mut srv = app_state.server_state.lock().await;
        srv.running = true;
        srv.bind_addr = Some(bind_str.clone());
        srv.error = None;
    }
    let server_state = app_state.server_state.clone();
    let handle = tokio::spawn(async move {
        let res = axum::serve(listener, router).await;
        let mut srv = server_state.lock().await;
        // Only overwrite if still marked running with same bind (avoid stale overwrite after rotation)
        if srv.running && srv.bind_addr.as_deref() == Some(&bind_str) {
            match res {
                Ok(()) => {
                    srv.running = false;
                    srv.error = Some("server exited".to_string());
                }
                Err(e) => {
                    srv.running = false;
                    srv.error = Some(format!("server failed: {e}"));
                    tracing::warn!(error=%e, "desktop background server failed");
                }
            }
            srv.bind_addr = None;
        }
    });
    *app_state.server_handle.lock().await = Some(handle);
    Ok(())
}

#[cfg(target_os = "macos")]
async fn try_start_server_if_needed(app_state: &AppState) {
    // Only start if not already running
    {
        let srv = app_state.server_state.lock().await;
        if srv.running {
            return;
        }
    }
    if app_state.server_handle.lock().await.is_some() {
        return;
    }
    let _ = start_server(app_state).await;
}

#[cfg(target_os = "macos")]
pub async fn spawn_background_server(app_state: &AppState) {
    try_start_server_if_needed(app_state).await;
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::settings::{SaveSettingsRequest, load_public_settings, save_public_settings};
    #[cfg(target_os = "macos")]
    use crate::storage::Storage;

    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
    #[test]
    fn repeated_save_no_restart_when_unchanged() {
        let storage = Storage::open_in_memory().unwrap();
        storage.ensure_instance_id(None).unwrap();
        // First save changes bind from default 5190 to 5191
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
        // Second save with same values as persisted should report no restart
        // Simulate AppState.config having been updated to 5191 after first save
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
