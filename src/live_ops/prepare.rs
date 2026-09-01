use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    operator_service::OperatorService,
    operator_types::{
        LiveOperationResponse, OperatorError, PrepareCancellationRequest, PrepareOrderRequest,
    },
    storage::{
        LiveOperationKind, LiveOperationState,
        redaction::{LivePayload, fingerprint},
    },
};

const OPERATION_TTL_SECONDS: i64 = 30;

impl OperatorService {
    pub async fn prepare_order(
        &self,
        request: PrepareOrderRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.prepare_order_with_actor(request, idempotency_key, Some("mcp".to_string()))
            .await
    }

    pub async fn prepare_order_with_actor(
        &self,
        request: PrepareOrderRequest,
        idempotency_key: String,
        actor_source: Option<String>,
    ) -> Result<LiveOperationResponse, OperatorError> {
        let trimmed = idempotency_key.trim().to_string();
        if trimmed.is_empty() {
            return Err(OperatorError::MissingIdempotencyKey);
        }
        let _guard = self.ui_lock.lock().await;
        if self
            .storage
            .idempotency_exists(&trimmed)
            .map_err(OperatorError::Storage)?
        {
            return Err(OperatorError::IdempotencyConflict);
        }
        if self
            .storage
            .has_active_live_operation()
            .map_err(OperatorError::Storage)?
        {
            return Err(OperatorError::ActiveConflict);
        }
        let payload = LivePayload::Order(request.order.clone());
        let fingerprint = fingerprint(LiveOperationKind::SubmitOrder, &payload)
            .map_err(OperatorError::Internal)?;
        let operation_id = Uuid::new_v4();
        let confirmation_token = Uuid::new_v4().simple().to_string();
        let now = Utc::now();
        let expires_at = now + Duration::seconds(OPERATION_TTL_SECONDS);
        let payload_summary = crate::storage::redacted_order_summary(&request.order);
        self.storage
            .insert_prepare_operation(
                &operation_id.to_string(),
                LiveOperationKind::SubmitOrder,
                &fingerprint,
                &trimmed,
                now,
                expires_at,
                &payload_summary,
                actor_source.as_deref(),
            )
            .map_err(OperatorError::Storage)?;
        self.confirmation_tokens
            .lock()
            .await
            .insert(operation_id, confirmation_token.clone());
        let ui_result = self
            .pages
            .open_order_confirmation(request.order.clone())
            .map_err(|e| OperatorError::UiUnavailable(e));
        if let Err(ui_err) = ui_result {
            let detail = serde_json::json!({"error": ui_err.to_string(), "outcome":"unknown"});
            let _ = self.storage.transition_to_unknown(
                &operation_id.to_string(),
                &detail,
                actor_source.as_deref(),
            );
            self.confirmation_tokens.lock().await.remove(&operation_id);
            return Err(ui_err);
        }
        Ok(LiveOperationResponse {
            operation_id: operation_id.to_string(),
            kind: LiveOperationKind::SubmitOrder,
            state: LiveOperationState::ConfirmationOpened,
            confirmation_token: Some(confirmation_token),
            fingerprint,
            expires_at,
        })
    }

    #[allow(dead_code)]
    pub async fn prepare_cancellation(
        &self,
        request: PrepareCancellationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.prepare_cancellation_with_actor(request, idempotency_key, Some("mcp".to_string()))
            .await
    }

    pub async fn prepare_cancellation_with_actor(
        &self,
        request: PrepareCancellationRequest,
        idempotency_key: String,
        actor_source: Option<String>,
    ) -> Result<LiveOperationResponse, OperatorError> {
        let trimmed = idempotency_key.trim().to_string();
        if trimmed.is_empty() {
            return Err(OperatorError::MissingIdempotencyKey);
        }
        let _guard = self.ui_lock.lock().await;
        if self
            .storage
            .idempotency_exists(&trimmed)
            .map_err(OperatorError::Storage)?
        {
            return Err(OperatorError::IdempotencyConflict);
        }
        if self
            .storage
            .has_active_live_operation()
            .map_err(OperatorError::Storage)?
        {
            return Err(OperatorError::ActiveConflict);
        }
        let payload = LivePayload::Cancellation(request.target.clone());
        let fingerprint = fingerprint(LiveOperationKind::CancelOrder, &payload)
            .map_err(OperatorError::Internal)?;
        let operation_id = Uuid::new_v4();
        let confirmation_token = Uuid::new_v4().simple().to_string();
        let now = Utc::now();
        let expires_at = now + Duration::seconds(OPERATION_TTL_SECONDS);
        let payload_summary = crate::storage::redacted_cancellation_summary(&request.target);
        self.storage
            .insert_prepare_operation(
                &operation_id.to_string(),
                LiveOperationKind::CancelOrder,
                &fingerprint,
                &trimmed,
                now,
                expires_at,
                &payload_summary,
                actor_source.as_deref(),
            )
            .map_err(OperatorError::Storage)?;
        self.confirmation_tokens
            .lock()
            .await
            .insert(operation_id, confirmation_token.clone());
        let ui_result = self
            .pages
            .open_cancel_confirmation(&request.target)
            .map_err(|e| OperatorError::UiUnavailable(e));
        if let Err(ui_err) = ui_result {
            let detail = serde_json::json!({"error": ui_err.to_string(), "outcome":"unknown"});
            let _ = self.storage.transition_to_unknown(
                &operation_id.to_string(),
                &detail,
                actor_source.as_deref(),
            );
            self.confirmation_tokens.lock().await.remove(&operation_id);
            return Err(ui_err);
        }
        Ok(LiveOperationResponse {
            operation_id: operation_id.to_string(),
            kind: LiveOperationKind::CancelOrder,
            state: LiveOperationState::ConfirmationOpened,
            confirmation_token: Some(confirmation_token),
            fingerprint,
            expires_at,
        })
    }

    pub async fn prepare_order_http(
        &self,
        request: PrepareOrderRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.prepare_order_with_actor(request, idempotency_key, Some("http".to_string()))
            .await
    }

    pub async fn prepare_cancellation_http(
        &self,
        request: PrepareCancellationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.prepare_cancellation_with_actor(request, idempotency_key, Some("http".to_string()))
            .await
    }
}
