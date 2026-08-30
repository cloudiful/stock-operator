use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Serialize;
use utoipa::{
    Modify, OpenApi,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

use crate::{
    operator_service::{
        ConfirmOperationRequest, LiveOperationResponse, OperatorService,
        PrepareCancellationRequest, PrepareOrderRequest, ReadRequest, SelectSecurityRequest,
    },
    pages::{NavigationTarget, StageOrderRequest, TradePreflightRequest, TradePreflightResult},
};

#[derive(Clone)]
pub struct HttpState {
    pub service: OperatorService,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: OperatorErrorBody,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OperatorErrorBody {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    retryable: bool,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: OperatorErrorBody {
                    code: self.code,
                    message: self.message,
                    retryable: self.retryable,
                },
            }),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        let message = format!("{error:#}");
        let (status, code, retryable) = if message.contains("outcome is unknown") {
            (StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown", false)
        } else if message.contains("not found") {
            (StatusCode::NOT_FOUND, "not_found", false)
        } else if message.contains("already")
            || message.contains("does not match")
            || message.contains("another live operation")
        {
            (StatusCode::CONFLICT, "state_conflict", false)
        } else if message.contains("unavailable") || message.contains("could not be focused") {
            (StatusCode::SERVICE_UNAVAILABLE, "ui_unavailable", true)
        } else {
            (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", false)
        };
        Self {
            status,
            code,
            message,
            retryable,
        }
    }
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
    let target = parse_navigation_target(&target)?;
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

#[utoipa::path(post, path = "/api/v1/operator/orders/stage", request_body = StageOrderRequest, responses((status = 200, body = crate::pages::StageOrderResult)), security(("bearer_auth" = [])))]
async fn stage_order(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<StageOrderRequest>,
) -> Result<Json<crate::pages::StageOrderResult>, ApiError> {
    Ok(Json(
        state
            .service
            .stage_order(request)
            .await
            .map_err(ApiError::from)?,
    ))
}

#[utoipa::path(post, path = "/api/v1/operator/orders/preflight", request_body = TradePreflightRequest, responses((status = 200, body = TradePreflightResult), (status = 422, body = ErrorResponse)), security(("bearer_auth" = [])))]
async fn trade_preflight(
    State(state): State<Arc<HttpState>>,
    Json(request): Json<TradePreflightRequest>,
) -> Result<Json<TradePreflightResult>, ApiError> {
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
            .prepare_order(request, idempotency_key(&headers)?)
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
            .prepare_cancellation(request, idempotency_key(&headers)?)
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
            .confirm_operation(request, idempotency_key(&headers)?)
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
            .abort_operation(&operation_id)
            .await
            .map_err(ApiError::from)?,
    ))
}

fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "missing_idempotency_key",
            message: "Idempotency-Key header is required".to_string(),
            retryable: false,
        })
}

fn parse_navigation_target(target: &str) -> Result<NavigationTarget, ApiError> {
    match target {
        "positions" => Ok(NavigationTarget::Positions),
        "orders" => Ok(NavigationTarget::Orders),
        "executions" => Ok(NavigationTarget::Executions),
        "funds" => Ok(NavigationTarget::Funds),
        _ => Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_target",
            message: format!("unsupported navigation target: {target}"),
            retryable: false,
        }),
    }
}

#[derive(OpenApi)]
#[openapi(
    info(title = "stock-operator", version = "0.2.12"),
    paths(view, read, navigate, select_security, stage_order, trade_preflight, prepare_order, prepare_cancellation, confirm_operation, abort_operation, get_operation),
    components(schemas(
        ReadRequest, SelectSecurityRequest, StageOrderRequest, crate::pages::StageOrderResult,
        TradePreflightRequest, TradePreflightResult,
        PrepareOrderRequest, PrepareCancellationRequest, crate::pages::CancellationTarget, ConfirmOperationRequest,
        LiveOperationResponse, ErrorResponse, OperatorErrorBody
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
        ] {
            assert!(paths.contains_key(path), "missing OpenAPI path: {path}");
        }
        assert_eq!(
            value["components"]["securitySchemes"]["bearer_auth"]["scheme"],
            "bearer"
        );
        assert!(value["components"]["schemas"]["StageOrderRequest"].is_object());
        let document = serde_json::to_string(&value).unwrap();
        assert!(document.contains("#/components/schemas/StageOrderRequest"));
    }
}
