use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    pages::{CancellationTarget, StageOrderRequest},
    storage::{LiveOperationKind, LiveOperationState},
};

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct PrepareOrderRequest {
    pub order: StageOrderRequest,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct PrepareCancellationRequest {
    pub target: CancellationTarget,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct ConfirmOperationRequest {
    pub operation_id: String,
    pub confirmation_token: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct LiveOperationResponse {
    pub operation_id: String,
    pub kind: LiveOperationKind,
    pub state: LiveOperationState,
    pub confirmation_token: Option<String>,
    pub fingerprint: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct OperationHistoryEntry {
    pub operation_id: String,
    pub kind: LiveOperationKind,
    pub state: LiveOperationState,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub payload_summary: serde_json::Value,
    pub result_summary: Option<serde_json::Value>,
    pub actor_source: Option<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct OperationHistoryResponse {
    pub operations: Vec<OperationHistoryEntry>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct AuditEventResponse {
    pub id: i64,
    pub operation_id: String,
    pub event_type: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub created_at: DateTime<Utc>,
    pub actor_source: Option<String>,
    pub detail: Option<serde_json::Value>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct AuditHistoryResponse {
    pub events: Vec<AuditEventResponse>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    #[error("operation not found")]
    NotFound,
    #[error("Idempotency-Key is required")]
    MissingIdempotencyKey,
    #[error("Idempotency-Key has already been used")]
    IdempotencyConflict,
    #[error("another live operation still owns the broker dialog")]
    ActiveConflict,
    #[error("confirmation token does not match")]
    TokenMismatch,
    #[error("operation fingerprint does not match")]
    FingerprintMismatch,
    #[error("idempotency key does not match the prepared operation")]
    IdempotencyMismatch,
    #[error("operation is not awaiting confirmation")]
    NotAwaitingConfirmation,
    #[error("operation cannot be aborted in its current state")]
    AbortConflict,
    #[error("operation has expired")]
    Expired,
    #[error("confirmation outcome is unknown; do not retry automatically")]
    OutcomeUnknown,
    #[error("broker UI unavailable")]
    UiUnavailable(#[source] anyhow::Error),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("internal storage error")]
    Storage(#[source] anyhow::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
