use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::operator_types::OperatorError;

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
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
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

impl From<OperatorError> for ApiError {
    fn from(err: OperatorError) -> Self {
        match err {
            OperatorError::NotFound => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: err.to_string(),
                retryable: false,
            },
            OperatorError::MissingIdempotencyKey => Self {
                status: StatusCode::BAD_REQUEST,
                code: "missing_idempotency_key",
                message: err.to_string(),
                retryable: false,
            },
            OperatorError::IdempotencyConflict
            | OperatorError::ActiveConflict
            | OperatorError::TokenMismatch
            | OperatorError::FingerprintMismatch
            | OperatorError::IdempotencyMismatch
            | OperatorError::NotAwaitingConfirmation
            | OperatorError::AbortConflict
            | OperatorError::Expired => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message: err.to_string(),
                retryable: false,
            },
            OperatorError::OutcomeUnknown => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "outcome_unknown",
                message: err.to_string(),
                retryable: false,
            },
            OperatorError::UiUnavailable(_) => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "ui_unavailable",
                message: "broker UI unavailable".to_string(),
                retryable: true,
            },
            OperatorError::Validation(msg) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "validation_failed",
                message: msg,
                retryable: false,
            },
            OperatorError::Storage(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "storage_error",
                message: "storage error".to_string(),
                retryable: false,
            },
            OperatorError::Internal(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: "internal error".to_string(),
                retryable: false,
            },
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        if let Some(op) = error.downcast_ref::<OperatorError>() {
            return Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: op.to_string(),
                retryable: false,
            };
        }
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "internal error".to_string(),
            retryable: false,
        }
    }
}

pub fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
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

pub fn parse_navigation_target(target: &str) -> Result<crate::pages::NavigationTarget, ApiError> {
    match target {
        "positions" => Ok(crate::pages::NavigationTarget::Positions),
        "orders" => Ok(crate::pages::NavigationTarget::Orders),
        "executions" => Ok(crate::pages::NavigationTarget::Executions),
        "funds" => Ok(crate::pages::NavigationTarget::Funds),
        _ => Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_target",
            message: format!("unsupported navigation target: {target}"),
            retryable: false,
        }),
    }
}
