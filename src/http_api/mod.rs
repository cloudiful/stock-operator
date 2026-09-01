use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use utoipa::{
    IntoParams, Modify, OpenApi, ToSchema,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

use crate::{
    operator_service::{OperatorService, ReadRequest, SelectSecurityRequest},
    operator_types::{
        AuditHistoryResponse, ConfirmOperationRequest, LiveOperationResponse,
        OperationHistoryResponse, PrepareCancellationRequest, PrepareOrderRequest,
    },
    storage::{LiveOperationKind, LiveOperationState},
};

mod error;
pub use error::{ApiError, ErrorResponse, OperatorErrorBody};

#[derive(Clone)]
pub struct HttpState {
    pub service: OperatorService,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct OperationHistoryQuery {
    #[param(example = 20)]
    pub limit: Option<usize>,
    #[param(example = 0)]
    pub offset: Option<usize>,
    pub kind: Option<LiveOperationKind>,
    pub state: Option<LiveOperationState>,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct AuditHistoryQuery {
    #[param(example = 20)]
    pub limit: Option<usize>,
    #[param(example = 0)]
    pub offset: Option<usize>,
    pub operation_id: Option<String>,
}

pub fn router(state: HttpState) -> Router {
    Router::new()
        .route("/api/v1/operator/view", get(view))
        .route("/api/v1/operator/read", post(read))
        .route("/api/v1/operator/navigate/{target}", post(navigate))
        .route("/api/v1/operator/securities/select", post(select_security))
        .route("/api/v1/operator/orders/stage", post(stage_order))
        .route("/api/v1/operator/orders/preflight", post(trade_preflight))
        .route("/api/v1/operator/orders/prepare", post(prepare_order))
        .route(
            "/api/v1/operator/cancellations/prepare",
            post(prepare_cancellation),
        )
        .route(
            "/api/v1/operator/operations/confirm",
            post(confirm_operation),
        )
        .route(
            "/api/v1/operator/operations/{operation_id}/abort",
            post(abort_operation),
        )
        .route("/api/v1/operator/operations", get(list_operations))
        .route("/api/v1/operator/audit/events", get(list_audit_events))
        .route(
            "/api/v1/operator/operations/{operation_id}",
            get(get_operation),
        )
        .with_state(Arc::new(state))
}

#[utoipa::path(get, path = "/api/v1/operator/view", responses((status = 200, body = Object)), security(("bearer_auth" = [])))]
async fn view(State(state): State<Arc<HttpState>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(state.service.view().await.map_err(ApiError::from)?)
            .map_err(|error| ApiError::from(anyhow::Error::from(error)))?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/read", request_body = ReadRequest, responses((status = 200, body = Object), (status = 422, body = ErrorResponse)), security(("bearer_auth" = [])))]
async fn read(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<ReadRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        state.service.read(request).await.map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/navigate/{target}", params(("target" = String, Path)), responses((status = 200, body = Object)), security(("bearer_auth" = [])))]
async fn navigate(
    State(state): State<Arc<HttpState>>,
    Path(target): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let target = error::parse_navigation_target(&target)?;
    Ok(Json(
        serde_json::to_value(
            state
                .service
                .navigate(target)
                .await
                .map_err(ApiError::from)?,
        )
        .map_err(|error| ApiError::from(anyhow::Error::from(error)))?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/securities/select", request_body = SelectSecurityRequest, responses((status = 200, body = Object)), security(("bearer_auth" = [])))]
async fn select_security(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<SelectSecurityRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        state
            .service
            .select_security(request)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/orders/stage", request_body = crate::pages::StageOrderRequest, responses((status = 200, body = crate::pages::StageOrderResult)), security(("bearer_auth" = [])))]
async fn stage_order(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<crate::pages::StageOrderRequest>,
) -> Result<Json<crate::pages::StageOrderResult>, ApiError> {
    Ok(Json(
        state
            .service
            .stage_order(request)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/orders/preflight", request_body = crate::pages::TradePreflightRequest, responses((status = 200, body = crate::pages::TradePreflightResult), (status = 422, body = ErrorResponse)), security(("bearer_auth" = [])))]
async fn trade_preflight(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<crate::pages::TradePreflightRequest>,
) -> Result<Json<crate::pages::TradePreflightResult>, ApiError> {
    Ok(Json(
        state
            .service
            .trade_preflight(request)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/orders/prepare", request_body = PrepareOrderRequest, responses((status = 200, body = LiveOperationResponse)), security(("bearer_auth" = [])))]
async fn prepare_order(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(request): Json<PrepareOrderRequest>,
) -> Result<Json<LiveOperationResponse>, ApiError> {
    Ok(Json(
        state
            .service
            .prepare_order_http(request, error::idempotency_key(&headers)?)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/cancellations/prepare", request_body = PrepareCancellationRequest, responses((status = 200, body = LiveOperationResponse)), security(("bearer_auth" = [])))]
async fn prepare_cancellation(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(request): Json<PrepareCancellationRequest>,
) -> Result<Json<LiveOperationResponse>, ApiError> {
    Ok(Json(
        state
            .service
            .prepare_cancellation_http(request, error::idempotency_key(&headers)?)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/operations/confirm", request_body = ConfirmOperationRequest, responses((status = 200, body = LiveOperationResponse)), security(("bearer_auth" = [])))]
async fn confirm_operation(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(request): Json<ConfirmOperationRequest>,
) -> Result<Json<LiveOperationResponse>, ApiError> {
    Ok(Json(
        state
            .service
            .confirm_operation_http(request, error::idempotency_key(&headers)?)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/operator/operations/{operation_id}", params(("operation_id" = String, Path)), responses((status = 200, body = LiveOperationResponse)), security(("bearer_auth" = [])))]
async fn get_operation(
    State(state): State<Arc<HttpState>>,
    Path(operation_id): Path<String>,
) -> Result<Json<LiveOperationResponse>, ApiError> {
    Ok(Json(
        state
            .service
            .operation(&operation_id)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/operations/{operation_id}/abort", params(("operation_id" = String, Path)), responses((status = 200, body = LiveOperationResponse), (status = 409, body = ErrorResponse)), security(("bearer_auth" = [])))]
async fn abort_operation(
    State(state): State<Arc<HttpState>>,
    Path(operation_id): Path<String>,
) -> Result<Json<LiveOperationResponse>, ApiError> {
    Ok(Json(
        state
            .service
            .abort_operation_http(&operation_id)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/operator/operations", params(OperationHistoryQuery), responses((status = 200, body = OperationHistoryResponse)), security(("bearer_auth" = [])))]
async fn list_operations(
    State(state): State<Arc<HttpState>>,
    Query(query): Query<OperationHistoryQuery>,
) -> Result<Json<OperationHistoryResponse>, ApiError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0);
    Ok(Json(
        state
            .service
            .list_operations(limit, offset, query.kind, query.state)
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/operator/audit/events", params(AuditHistoryQuery), responses((status = 200, body = AuditHistoryResponse)), security(("bearer_auth" = [])))]
async fn list_audit_events(
    State(state): State<Arc<HttpState>>,
    Query(query): Query<AuditHistoryQuery>,
) -> Result<Json<AuditHistoryResponse>, ApiError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0);
    Ok(Json(
        state
            .service
            .list_audit_events(limit, offset, query.operation_id)
            .map_err(ApiError::from)?,
    ))
}

#[derive(OpenApi)]
#[openapi(
    info(title = "stock-operator", version = "0.3.0"),
    paths(view, read, navigate, select_security, stage_order, trade_preflight, prepare_order, prepare_cancellation, confirm_operation, abort_operation, get_operation, list_operations, list_audit_events),
    components(schemas(
        ReadRequest, SelectSecurityRequest, crate::pages::StageOrderRequest, crate::pages::StageOrderResult,
        crate::pages::TradePreflightRequest, crate::pages::TradePreflightResult,
        PrepareOrderRequest, PrepareCancellationRequest, crate::pages::CancellationTarget, ConfirmOperationRequest,
        LiveOperationResponse, OperationHistoryResponse, crate::operator_service::OperationHistoryEntry,
        AuditHistoryResponse, crate::operator_service::AuditEventResponse,
        LiveOperationKind, LiveOperationState, OperationHistoryQuery, AuditHistoryQuery,
        ErrorResponse, OperatorErrorBody
    )),
    modifiers(&SecurityAddon)
)]
struct OperatorApi;

struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

pub fn openapi_document() -> utoipa::openapi::OpenApi {
    OperatorApi::openapi()
}

#[cfg(test)]
mod tests {
    use super::openapi_document;

    #[test]
    fn openapi_contains_operator_routes_and_bearer_security() {
        let document = openapi_document();
        let value = serde_json::to_value(document).unwrap();
        assert_eq!(value["openapi"], "3.1.0");
        let paths = value["paths"].as_object().unwrap();
        for path in [
            "/api/v1/operator/read",
            "/api/v1/operator/orders/stage",
            "/api/v1/operator/orders/preflight",
            "/api/v1/operator/orders/prepare",
            "/api/v1/operator/cancellations/prepare",
            "/api/v1/operator/operations/confirm",
            "/api/v1/operator/operations/{operation_id}/abort",
            "/api/v1/operator/operations",
            "/api/v1/operator/audit/events",
        ] {
            assert!(paths.contains_key(path), "missing OpenAPI path: {path}");
        }
        assert_eq!(
            value["components"]["securitySchemes"]["bearer_auth"]["scheme"],
            "bearer"
        );
        assert!(value["components"]["schemas"]["StageOrderRequest"].is_object());
        assert!(value["components"]["schemas"]["OperationHistoryResponse"].is_object());
        assert!(value["components"]["schemas"]["AuditHistoryResponse"].is_object());
        let document = serde_json::to_string(&value).unwrap();
        assert!(document.contains("#/components/schemas/StageOrderRequest"));
    }

    #[test]
    fn history_endpoints_are_authenticated_documented() {
        let doc = openapi_document();
        let value = serde_json::to_value(doc).unwrap();
        let ops = &value["paths"]["/api/v1/operator/operations"]["get"];
        assert!(ops["security"].is_array());
        let audit = &value["paths"]["/api/v1/operator/audit/events"]["get"];
        assert!(audit["security"].is_array());
    }
}
