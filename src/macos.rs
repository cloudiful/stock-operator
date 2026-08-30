use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Parser;
use tracing::{info, warn};

use crate::{
    ax::AccessibilityInspector,
    cli::{self, Cli, Command, InspectCommand, NavigateCommand, OrderCommand, ReadCommand},
    config::OperatorConfig,
    mcp::serve,
    operator_service::{
        OperatorService, ReadPanel, ReadRepresentation, ReadRequest, SelectSecurityRequest,
    },
    pages::NavigationTarget,
    storage::Storage,
};

#[cfg(target_os = "macos")]
use crate::desktop;
#[cfg(target_os = "macos")]
use tauri::{Emitter, Manager};

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    // Resolve DB path before opening storage so we can locate SQLite on first run.
    let initial_db_path = crate::config::resolve_db_path();
    let storage = Storage::open(&initial_db_path).with_context(|| {
        format!(
            "failed to initialize SQLite at {}",
            initial_db_path.display()
        )
    })?;

    // Effective config: env overrides persisted SQLite settings, then defaults.
    let mut config = OperatorConfig::from_env_with_storage(&storage)
        .context("failed to load operator configuration")?;

    // Seed ordinary settings for future restarts; do not store secrets here.
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
        .context("failed to seed operator settings")?;

    // Note: private-overlay with non-loopback and no token is allowed for desktop startup;
    // the UI will show server stopped/missing token and allow saving a Keychain token.
    // `stock-operator serve` and any listener still fail-closed without a token (checked below).

    // Ensure instance id exists (generated if not configured)
    let instance_id = storage
        .ensure_instance_id(config.instance_id.clone())
        .context("failed to ensure operator instance id")?;
    config.instance_id = Some(instance_id.clone());
    info!(db_path = %config.db_path.display(), instance_id = %instance_id, "operator storage initialized");

    let reconciled = storage
        .reconcile_pending_operations()
        .context("failed to reconcile pending operations")?;
    if reconciled > 0 {
        warn!(
            count = reconciled,
            "reconciled abandoned live operations to Unknown/Expired on startup"
        );
    }

    let inspector = AccessibilityInspector::new(config.clone());
    let service = OperatorService::new(inspector.clone(), Arc::new(storage.clone()));
    if !inspector.request_permission_prompt() {
        info!("Accessibility permission is not granted yet");
    }

    match cli.command {
        Some(Command::Serve) => {
            // Explicit headless server mode: `stock-operator serve`
            let effective_token = resolve_effective_token(&config);
            let Some(token) = effective_token else {
                bail!(
                    "STOCK_OPERATOR_AUTH_TOKEN is required (set env var or save via desktop Keychain) when starting the server"
                )
            };
            let mut serve_config = config.clone();
            serve_config.auth_token = Some(token);
            info!(bind_addr = %serve_config.bind_addr, endpoint = %serve_config.endpoint(), "starting headless operator server");
            serve(serve_config, inspector, service).await
        }
        Some(cmd) => dispatch(cmd, &config, &service).await,
        None => {
            // No CLI command: launch Tauri desktop. Double-clicking the .app arrives here.
            #[cfg(target_os = "macos")]
            {
                return launch_desktop(config, inspector, service, storage).await;
            }
            #[cfg(not(target_os = "macos"))]
            {
                info!(bind_addr = %config.bind_addr, endpoint = %config.endpoint(), "starting stock operator HTTP and MCP server");
                let effective_token = resolve_effective_token(&config);
                let Some(token) = effective_token else {
                    bail!("STOCK_OPERATOR_AUTH_TOKEN is required")
                };
                let mut serve_config = config.clone();
                serve_config.auth_token = Some(token);
                serve(serve_config, inspector, service).await
            }
        }
    }
}

fn resolve_effective_token(config: &OperatorConfig) -> Option<String> {
    if let Some(tok) = config.auth_token.clone().filter(|v| !v.trim().is_empty()) {
        return Some(tok);
    }
    #[cfg(target_os = "macos")]
    {
        let (tok, _) = desktop::keychain::resolve_token();
        if let Some(t) = tok.filter(|v| !v.trim().is_empty()) {
            return Some(t);
        }
    }
    None
}

#[cfg(target_os = "macos")]
async fn launch_desktop(
    config: OperatorConfig,
    _inspector: AccessibilityInspector,
    service: OperatorService,
    storage: Storage,
) -> Result<()> {
    use std::sync::Arc;

    // Merge keychain token into config for background server, but keep UI usable without it.
    let effective_token = resolve_effective_token(&config);
    let has_token = effective_token.is_some();
    let mut desktop_config = config.clone();
    if desktop_config.auth_token.is_none() {
        desktop_config.auth_token = effective_token.clone();
    }

    let storage_arc = Arc::new(storage);
    let app_state =
        desktop::AppState::new(storage_arc.clone(), service.clone(), desktop_config.clone());
    let app_state_clone = app_state;
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!(?argv, "single-instance second launch detected");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.show();
            }
            // Emit event for frontend to optionally refresh
            let _ = app.emit(
                "single-instance",
                serde_json::json!({"args": argv, "cwd": _cwd}),
            );
        }))
        .manage(app_state_clone)
        .invoke_handler(tauri::generate_handler![
            desktop::get_settings,
            desktop::save_settings,
            desktop::get_runtime_status,
            desktop::get_token_status,
            desktop::save_token,
            desktop::clear_token,
            desktop::test_stock_service_url,
            desktop::list_operations,
            desktop::list_audit_events,
            desktop::resolve_stale_operation
        ])
        .setup(move |app| {
            let state = app.state::<desktop::AppState>();
            let s = state.inner().clone();
            // Spawn background server if token is configured; otherwise UI stays usable and status shows not running.
            if has_token {
                tauri::async_runtime::spawn(async move {
                    desktop::spawn_background_server(&s).await;
                });
            } else {
                tracing::info!(
                    "desktop started without bearer token; server not running until token is saved"
                );
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("Tauri desktop failed")?;
    Ok(())
}

async fn dispatch(
    command: Command,
    config: &OperatorConfig,
    service: &OperatorService,
) -> Result<()> {
    match command {
        Command::Serve => unreachable!("Serve handled earlier"),
        Command::Inspect(command) => inspect(command, config, service).await,
        Command::Read(command) => read(command, service).await,
        Command::Navigate(command) => navigate(command, service).await,
        Command::Order(command) => order(command, service).await,
        Command::Cancel(command) => cancel(command, service).await,
    }
}

async fn inspect(
    command: InspectCommand,
    config: &OperatorConfig,
    service: &OperatorService,
) -> Result<()> {
    let value = match command {
        InspectCommand::Openapi => serde_json::to_value(crate::http_api::openapi_document())?,
        InspectCommand::Probe(args) => serde_json::to_value(
            service
                .snapshot(
                    args.max_depth.unwrap_or(config.max_depth).clamp(1, 12),
                    args.max_nodes.unwrap_or(config.max_nodes).clamp(1, 2_000),
                )
                .await?,
        )?,
        InspectCommand::View => serde_json::to_value(service.view().await?)?,
        InspectCommand::Inventory { limit } => service.inventory(limit).await?,
        InspectCommand::Form => service.trade_form().await?,
        InspectCommand::Ocr => service.ocr_visible_text().await?,
        InspectCommand::PositionsTable => service.positions_diagnostic().await?,
        InspectCommand::Preflight(args) => {
            serde_json::to_value(service.trade_preflight(args.into_request()?).await?)?
        }
    };
    print_json(value)
}

async fn read(command: ReadCommand, service: &OperatorService) -> Result<()> {
    let (panel, representation) = match command {
        ReadCommand::Positions { source } => (ReadPanel::Positions, source),
        ReadCommand::Orders { source } => (ReadPanel::Orders, source),
        ReadCommand::Executions { source } => (ReadPanel::Executions, source),
        ReadCommand::Funds { source } => (ReadPanel::Funds, source),
        ReadCommand::Cancellations { source } => (ReadPanel::Cancellations, source),
    };
    print_json(
        service
            .read(ReadRequest {
                panel,
                representation: representation.map(read_representation),
            })
            .await?,
    )
}

async fn navigate(command: NavigateCommand, service: &OperatorService) -> Result<()> {
    let value = match command {
        NavigateCommand::Positions => {
            serde_json::to_value(service.navigate(NavigationTarget::Positions).await?)?
        }
        NavigateCommand::Orders => {
            serde_json::to_value(service.navigate(NavigationTarget::Orders).await?)?
        }
        NavigateCommand::Executions => {
            serde_json::to_value(service.navigate(NavigationTarget::Executions).await?)?
        }
        NavigateCommand::Funds => {
            serde_json::to_value(service.navigate(NavigationTarget::Funds).await?)?
        }
        NavigateCommand::Cancellations => service.navigate_cancellations().await?,
        NavigateCommand::Candidates => service.navigation_candidates().await?,
    };
    print_json(value)
}

async fn order(command: OrderCommand, service: &OperatorService) -> Result<()> {
    match command {
        OrderCommand::SelectSecurity { security_code } => print_json(
            service
                .select_security(SelectSecurityRequest { security_code })
                .await?,
        ),
        OrderCommand::Stage { request } => print_json(serde_json::to_value(
            service.stage_order(request.into()).await?,
        )?),
        OrderCommand::OpenConfirmation { request, live } => {
            require_live(live)?;
            print_json(
                service
                    .open_order_confirmation_direct(request.into())
                    .await?,
            )
        }
        OrderCommand::Confirm { request, live } => {
            require_live(live)?;
            print_json(service.confirm_order_direct(request.into()).await?)
        }
        OrderCommand::CloseNotice { contract_id, live } => {
            require_live(live)?;
            service.close_submitted_notice(&contract_id).await
        }
    }
}

async fn cancel(command: cli::CancelCommand, service: &OperatorService) -> Result<()> {
    match command {
        cli::CancelCommand::OpenConfirmation { target, live } => {
            require_live(live)?;
            let target = target.into();
            service.open_cancellation_direct(&target).await
        }
        cli::CancelCommand::Confirm { target, live } => {
            require_live(live)?;
            let target = target.into();
            service.confirm_cancellation_direct(&target).await
        }
        cli::CancelCommand::CloseNotice { live } => {
            require_live(live)?;
            service.close_cancel_submitted_notice().await
        }
        cli::CancelCommand::DismissSelectionWarning { live } => {
            require_live(live)?;
            service.close_cancel_selection_warning().await
        }
    }
}

fn read_representation(source: cli::ReadSource) -> ReadRepresentation {
    match source {
        cli::ReadSource::Ax => ReadRepresentation::Ax,
        cli::ReadSource::Ocr => ReadRepresentation::Ocr,
        cli::ReadSource::Structured => ReadRepresentation::Structured,
    }
}

fn require_live(live: bool) -> Result<()> {
    if !live {
        bail!("this command can mutate the broker UI; add --live explicitly")
    }
    Ok(())
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
