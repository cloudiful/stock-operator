use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::IntoResponse,
};
use rmcp::{
    ErrorData as McpError, Json, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo},
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};

use super::{
    ax::AccessibilityInspector,
    config::OperatorConfig,
    http_api::{self, HttpState},
    operator_service::OperatorService,
    operator_types::{ConfirmOperationRequest, PrepareOrderRequest},
    pages::{
        ExecutionStructuredSnapshot, ExecutionsSnapshot, FundsSnapshot, FundsStructuredSnapshot,
        NavigationCandidates, NavigationResult, NavigationTarget, OcrSnapshot,
        OrderStructuredSnapshot, OrdersSnapshot, PageReader, PositionStructuredSnapshot,
        PositionsSnapshot, PositionsTableDiagnostic, StageOrderRequest, StageOrderResult,
        TradeFormSnapshot, TradePreflightRequest, TradePreflightResult, UiInventory,
        ViewDescriptor,
    },
};

#[derive(Clone)]
struct OperatorMcpServer {
    inspector: AccessibilityInspector,
    service: OperatorService,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct SnapshotArgs {
    #[schemars(description = "Maximum Accessibility tree depth")]
    max_depth: Option<usize>,
    #[schemars(description = "Maximum number of returned Accessibility nodes")]
    max_nodes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct NavigateArgs {
    target: NavigationTarget,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct HealthResult {
    service: &'static str,
    mode: &'static str,
    mutations_enabled: bool,
    endpoint_scope: &'static str,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ListOperationsArgs {
    #[schemars(description = "Maximum number of recent operations to return (1-100, default 20)")]
    limit: Option<usize>,
    #[schemars(description = "Filter by kind: submit_order or cancel_order")]
    kind: Option<String>,
    #[schemars(description = "Filter by state")]
    state: Option<String>,
}

#[tool_router(router = tool_router)]
impl OperatorMcpServer {
    fn new(inspector: AccessibilityInspector, service: OperatorService) -> Self {
        Self {
            inspector,
            service,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "operator_health",
        description = "Return the local operator safety mode. Read/stage operations and explicitly confirmed live operations are exposed separately; live actions require short-lived operation fingerprints and one-time confirmation."
    )]
    async fn operator_health(&self) -> Result<Json<HealthResult>, McpError> {
        Ok(Json(HealthResult {
            service: "stock-operator",
            mode: "staging_and_supervised_live",
            mutations_enabled: true,
            endpoint_scope: "loopback_only",
        }))
    }

    #[tool(
        name = "accessibility_status",
        description = "Check macOS Accessibility API permission and whether the configured target trading process is running."
    )]
    async fn accessibility_status(&self) -> Result<Json<super::ax::AccessibilityStatus>, McpError> {
        Ok(Json(self.inspector.status()))
    }

    #[tool(
        name = "inspect_target_app",
        description = "Read a bounded, read-only Accessibility tree from the configured trading application. No button press, text entry, keyboard event, or UI mutation is performed."
    )]
    async fn inspect_target_app(
        &self,
        Parameters(args): Parameters<SnapshotArgs>,
    ) -> Result<Json<super::ax::TargetSnapshot>, McpError> {
        self.service
            .snapshot(
                args.max_depth.unwrap_or(6).clamp(1, 12),
                args.max_nodes.unwrap_or(300).clamp(1, 2_000),
            )
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_trade_snapshot",
        description = "Read a bounded snapshot of the configured trading application's currently exposed UI. This is diagnostic only and does not place, modify, or cancel an order."
    )]
    async fn read_trade_snapshot(
        &self,
        Parameters(args): Parameters<SnapshotArgs>,
    ) -> Result<Json<super::ax::TargetSnapshot>, McpError> {
        self.service
            .snapshot(
                args.max_depth.unwrap_or(6).clamp(1, 12),
                args.max_nodes.unwrap_or(300).clamp(1, 2_000),
            )
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "ui_inventory",
        description = "Return a bounded list of semantic controls exposed by the current trading window. This is read-only and does not press or modify any control."
    )]
    async fn ui_inventory(&self) -> Result<Json<UiInventory>, McpError> {
        self.service
            .with_ui(|pages| pages.inventory(200))
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "current_view",
        description = "Identify the currently visible trading page from Accessibility evidence. This does not navigate or modify the application."
    )]
    async fn current_view(&self) -> Result<Json<ViewDescriptor>, McpError> {
        self.service.view().await.map(Json).map_err(internal_error)
    }

    #[tool(
        name = "read_trade_form",
        description = "Read currently exposed trade form fields such as text inputs and their values. This tool never types or submits an order."
    )]
    async fn read_trade_form(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<TradeFormSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::trade_form)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_positions",
        description = "Read the currently exposed positions table structure and values. This tool never navigates or modifies the trading application."
    )]
    async fn read_positions(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<PositionsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::positions)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_positions_ocr",
        description = "Read positions by matching macOS Vision OCR text to Accessibility table cell geometry. Results include OCR quality warnings and must be verified before use. This tool is read-only and never navigates or modifies the trading application."
    )]
    async fn read_positions_ocr(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<PositionsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::positions_ocr)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "diagnose_positions_table",
        description = "Inspect a bounded, read-only Accessibility representation of the positions table to determine where cell values are exposed. Sensitive account columns are redacted. This tool does not navigate or modify the trading application."
    )]
    async fn diagnose_positions_table(&self) -> Result<Json<PositionsTableDiagnostic>, McpError> {
        self.service
            .with_ui(|pages| pages.diagnose_positions_table(10, 32, 3))
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_orders",
        description = "Read the currently exposed same-day orders table. This tool never navigates, modifies, cancels, or submits an order."
    )]
    async fn read_orders(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<OrdersSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::orders)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_cancellations",
        description = "Read the cancellation table structure and values. This tool never selects, cancels, or submits an order."
    )]
    async fn read_cancellations(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<OrdersSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::cancellations)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_executions",
        description = "Read the currently exposed same-day executions table. This tool never navigates or modifies the trading application."
    )]
    async fn read_executions(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<ExecutionsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::executions)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_funds",
        description = "Read the currently exposed funds table structure and values. This tool never navigates or modifies the trading application."
    )]
    async fn read_funds(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<FundsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::funds)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_orders_ocr",
        description = "Read the currently exposed same-day orders table using Vision OCR matched to Accessibility cell geometry. This is read-only and returns partial-quality warnings when OCR is ambiguous."
    )]
    async fn read_orders_ocr(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<OrdersSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::orders_ocr)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_cancellations_ocr",
        description = "Read the cancellation table using Vision OCR and Accessibility geometry. This tool never selects, cancels, or submits an order."
    )]
    async fn read_cancellations_ocr(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<OrdersSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::cancellations_ocr)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_executions_ocr",
        description = "Read the currently exposed same-day executions table using Vision OCR matched to Accessibility cell geometry. This is read-only and returns partial-quality warnings when OCR is ambiguous."
    )]
    async fn read_executions_ocr(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<ExecutionsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::executions_ocr)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_funds_ocr",
        description = "Read the currently exposed funds table using Vision OCR matched to Accessibility cell geometry. This is read-only and returns partial-quality warnings when OCR is ambiguous."
    )]
    async fn read_funds_ocr(
        &self,
    ) -> Result<Json<super::pages::PageSnapshot<FundsSnapshot>>, McpError> {
        self.service
            .with_ui(PageReader::funds_ocr)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_positions_structured",
        description = "Read OCR positions and map table columns to structured position records with normalized values and quality metadata. This is read-only and never navigates or modifies the trading application."
    )]
    async fn read_positions_structured(
        &self,
    ) -> Result<Json<PositionStructuredSnapshot>, McpError> {
        self.service
            .with_ui(PageReader::positions_structured)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_orders_structured",
        description = "Read OCR orders and map table columns to structured order records with normalized values and quality metadata. This is read-only and never navigates or modifies the trading application."
    )]
    async fn read_orders_structured(&self) -> Result<Json<OrderStructuredSnapshot>, McpError> {
        self.service
            .with_ui(PageReader::orders_structured)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_executions_structured",
        description = "Read OCR executions and map table columns to structured execution records with normalized values and quality metadata. This is read-only and never navigates or modifies the trading application."
    )]
    async fn read_executions_structured(
        &self,
    ) -> Result<Json<ExecutionStructuredSnapshot>, McpError> {
        self.service
            .with_ui(PageReader::executions_structured)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "read_funds_structured",
        description = "Read OCR funds and map table columns to structured funds records with normalized values and quality metadata. This is read-only and never navigates or modifies the trading application."
    )]
    async fn read_funds_structured(&self) -> Result<Json<FundsStructuredSnapshot>, McpError> {
        self.service
            .with_ui(PageReader::funds_structured)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "navigation_candidates",
        description = "List only allowlisted read-only navigation controls currently exposed by the trading window. This does not press any control."
    )]
    async fn navigation_candidates(&self) -> Result<Json<NavigationCandidates>, McpError> {
        self.service
            .with_ui(PageReader::navigation_candidates)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "navigate_readonly",
        description = "Navigate only to an allowlisted read-only panel: positions, same-day orders, same-day executions, or funds. Uses a unique AX control or high-confidence OCR label and verifies the resulting panel. This cannot open buy, sell, cancel, or arbitrary controls."
    )]
    async fn navigate_readonly(
        &self,
        Parameters(request): Parameters<NavigateArgs>,
    ) -> Result<Json<NavigationResult>, McpError> {
        self.service
            .navigate(request.target)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "ocr_visible_text",
        description = "Run a one-shot, read-only macOS Vision OCR pass over the configured trading window. It does not click, type, navigate, or save a screenshot. Long numeric strings are redacted."
    )]
    async fn ocr_visible_text(&self) -> Result<Json<OcrSnapshot>, McpError> {
        self.service
            .with_ui(PageReader::ocr_visible_text)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "stage_order",
        description = "Select and verify a requested security in the current buy or sell form, validate broker-derived limits, write only verified price and quantity fields, and read them back. This tool never sends keyboard events, presses confirmation controls, or submits an order."
    )]
    async fn stage_order(
        &self,
        Parameters(request): Parameters<StageOrderRequest>,
    ) -> Result<Json<StageOrderResult>, McpError> {
        self.service
            .stage_order(request)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "trade_preflight",
        description = "Read broker-side security, form, quote, funds, buy/sell limits, and trade-session facts. Optionally validate an intended order without writing or submitting it."
    )]
    async fn trade_preflight(
        &self,
        Parameters(request): Parameters<TradePreflightRequest>,
    ) -> Result<Json<TradePreflightResult>, McpError> {
        self.service
            .trade_preflight(request)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "order_prepare",
        description = "Open and verify a live order confirmation dialog. Returns a short-lived one-time confirmation token; does not press confirm."
    )]
    async fn order_prepare(
        &self,
        Parameters(args): Parameters<PrepareOrderArgs>,
    ) -> Result<Json<super::operator_service::LiveOperationResponse>, McpError> {
        self.service
            .prepare_order(args.request, args.idempotency_key)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "operation_confirm",
        description = "Confirm one exact live order or cancellation operation using its one-time token and fingerprint."
    )]
    async fn operation_confirm(
        &self,
        Parameters(args): Parameters<ConfirmOperationArgs>,
    ) -> Result<Json<super::operator_service::LiveOperationResponse>, McpError> {
        self.service
            .confirm_operation(args.request, args.idempotency_key)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "operation_abort",
        description = "Close one exact live confirmation dialog without submitting it."
    )]
    async fn operation_abort(
        &self,
        Parameters(args): Parameters<AbortOperationArgs>,
    ) -> Result<Json<super::operator_service::LiveOperationResponse>, McpError> {
        self.service
            .abort_operation(&args.operation_id)
            .await
            .map(Json)
            .map_err(internal_error)
    }

    #[tool(
        name = "list_recent_operations",
        description = "List recent live operations with redacted summaries and state for audit history. Read-only and never exposes bearer or confirmation tokens. Prefer HTTP GET /api/v1/operator/operations for UI pagination."
    )]
    async fn list_recent_operations(
        &self,
        Parameters(args): Parameters<ListOperationsArgs>,
    ) -> Result<Json<super::operator_service::OperationHistoryResponse>, McpError> {
        let limit = args.limit.unwrap_or(20).clamp(1, 100);
        let kind = args.kind.and_then(|k| match k.as_str() {
            "submit_order" => Some(super::operator_service::LiveOperationKind::SubmitOrder),
            "cancel_order" => Some(super::operator_service::LiveOperationKind::CancelOrder),
            _ => None,
        });
        let state = args.state.and_then(|s| match s.as_str() {
            "confirmation_opened" => {
                Some(super::operator_service::LiveOperationState::ConfirmationOpened)
            }
            "confirming" => Some(super::operator_service::LiveOperationState::Confirming),
            "confirmed" => Some(super::operator_service::LiveOperationState::Confirmed),
            "unknown" => Some(super::operator_service::LiveOperationState::Unknown),
            "expired" => Some(super::operator_service::LiveOperationState::Expired),
            "aborted" => Some(super::operator_service::LiveOperationState::Aborted),
            _ => None,
        });
        self.service
            .list_operations(limit, 0, kind, state)
            .map(Json)
            .map_err(internal_error)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct PrepareOrderArgs {
    request: PrepareOrderRequest,
    idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct ConfirmOperationArgs {
    request: ConfirmOperationRequest,
    idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
struct AbortOperationArgs {
    operation_id: String,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OperatorMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_instructions(
        "Local macOS stock operator. It may inspect the configured trading application, navigate allowlisted panels, stage verified values, expose explicitly confirmed live operations, and list recent operation history with redacted summaries. Live operations require short-lived fingerprints and one-time confirmation; it cannot call arbitrary commands. History is available via GET /api/v1/operator/operations and GET /api/v1/operator/audit/events.",
            )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, McpError> {
        Ok(rmcp::model::ListResourcesResult::with_all_items(Vec::new()))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourceTemplatesResult, McpError> {
        Ok(rmcp::model::ListResourceTemplatesResult::with_all_items(
            Vec::new(),
        ))
    }
}

pub async fn serve(
    config: OperatorConfig,
    inspector: AccessibilityInspector,
    operator_service: OperatorService,
) -> Result<()> {
    let router = build_router(&config, inspector, operator_service)?;
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind stock-operator at {}", config.bind_addr))?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("stock-operator HTTP server failed")?;
    Ok(())
}

pub async fn serve_with_config(
    config: OperatorConfig,
    inspector: AccessibilityInspector,
    operator_service: OperatorService,
) -> Result<()> {
    serve(config, inspector, operator_service).await
}

pub(crate) fn build_router(
    config: &OperatorConfig,
    inspector: AccessibilityInspector,
    operator_service: OperatorService,
) -> Result<Router> {
    let server = OperatorMcpServer::new(inspector, operator_service.clone());
    let service = StreamableHttpService::new(
        move || Ok::<_, std::io::Error>(server.clone()),
        LocalSessionManager::default().into(),
        http_config(),
    );
    let token = Arc::new(
        config
            .auth_token
            .clone()
            .context("STOCK_OPERATOR_AUTH_TOKEN is required when starting the MCP server")?,
    );
    let mcp_service = Router::new()
        .fallback_service(service)
        .layer(middleware::from_fn_with_state(token.clone(), authenticate));
    let mcp_router = if config.mcp_path == "/" {
        Router::new().fallback_service(mcp_service)
    } else {
        Router::new().nest_service(&config.mcp_path, mcp_service)
    };
    let protected_api = http_api::router(HttpState {
        service: operator_service,
    })
    .layer(middleware::from_fn_with_state(token, authenticate));
    let router = Router::new()
        .route("/healthz", axum::routing::get(|| async { StatusCode::OK }))
        .route(
            "/api/openapi.json",
            axum::routing::get(|| async { axum::Json(http_api::openapi_document()) }),
        )
        .merge(protected_api)
        .merge(mcp_router);
    Ok(router)
}

fn http_config() -> rmcp::transport::streamable_http_server::StreamableHttpServerConfig {
    rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default()
        .with_sse_keep_alive(Some(Duration::from_secs(30)))
        .with_sse_retry(None)
        .with_legacy_session_mode(true)
        .disable_allowed_origins()
        .disable_allowed_hosts()
}

async fn authenticate(
    axum::extract::State(expected): axum::extract::State<Arc<String>>,
    request: Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| value.trim() == expected.as_str());
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn internal_error(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::build_router;
    use crate::{
        ax::AccessibilityInspector, config::OperatorConfig, operator_service::OperatorService,
        storage::Storage,
    };

    #[tokio::test]
    async fn http_auth_boundary_keeps_health_and_openapi_public() {
        let mut config = OperatorConfig::from_env().unwrap();
        config.auth_token = Some("test-token".to_string());
        let inspector = AccessibilityInspector::new(config.clone());
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let service = OperatorService::new(inspector.clone(), storage);
        let app = build_router(&config, inspector, service).unwrap();

        for path in ["/healthz", "/api/openapi.json"] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/v1/operator/view")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
