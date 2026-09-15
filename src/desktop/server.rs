use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{
    backend::Backend, config::OperatorConfig, operator_service::OperatorService, storage::Storage,
    win_backend::WinStub,
};

#[derive(Clone)]
pub struct ServerState {
    pub running: bool,
    pub bind_addr: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub service: OperatorService,
    pub config: Arc<Mutex<OperatorConfig>>,
    pub initial_config: OperatorConfig,
    pub server_state: Arc<Mutex<ServerState>>,
    pub server_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

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

async fn stop_server(app_state: &AppState) {
    let handle_opt = { app_state.server_handle.lock().await.take() };
    if let Some(handle) = handle_opt {
        handle.abort();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    let mut srv = app_state.server_state.lock().await;
    srv.running = false;
}

async fn start_server(app_state: &AppState) -> Result<(), String> {
    let (token_opt, _) = crate::desktop::keychain::resolve_token();
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
    let mut cfg = app_state.config.lock().await.clone();
    cfg.auth_token = Some(effective_token);
    let bind = cfg.bind_addr;
    let backend: Arc<dyn Backend> = Arc::new(WinStub::new());
    let router = crate::mcp::build_router(&cfg, backend, app_state.service.clone())
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

async fn try_start_server_if_needed(app_state: &AppState) {
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

pub async fn spawn_background_server(app_state: &AppState) {
    try_start_server_if_needed(app_state).await;
}

pub(crate) async fn stop_server_pub(app_state: &AppState) {
    stop_server(app_state).await;
}

pub(crate) async fn start_server_pub(app_state: &AppState) -> Result<(), String> {
    start_server(app_state).await
}
