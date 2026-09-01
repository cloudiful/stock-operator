use uuid::Uuid;

use crate::{
    operator_service::OperatorService,
    operator_types::{ConfirmOperationRequest, LiveOperationResponse, OperatorError},
    storage::{
        LiveOperationKind, LiveOperationState,
        redaction::{reconstruct_cancellation, reconstruct_order},
    },
};

impl OperatorService {
    pub async fn confirm_operation(
        &self,
        request: ConfirmOperationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.confirm_operation_with_actor(request, idempotency_key, Some("mcp".to_string()))
            .await
    }

    pub async fn confirm_operation_with_actor(
        &self,
        request: ConfirmOperationRequest,
        idempotency_key: String,
        actor_source: Option<String>,
    ) -> Result<LiveOperationResponse, OperatorError> {
        let trimmed = idempotency_key.trim().to_string();
        if trimmed.is_empty() {
            return Err(OperatorError::MissingIdempotencyKey);
        }
        let operation_id = Uuid::parse_str(&request.operation_id)
            .map_err(|_| OperatorError::Validation("invalid operation_id".to_string()))?;
        let stored = self
            .storage
            .get_operation(&request.operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        if stored.idempotency_key.as_deref() != Some(trimmed.as_str()) {
            return Err(OperatorError::IdempotencyMismatch);
        }
        if stored.fingerprint != request.fingerprint {
            return Err(OperatorError::FingerprintMismatch);
        }
        {
            let tokens = self.confirmation_tokens.lock().await;
            match tokens.get(&operation_id) {
                Some(t) if t == &request.confirmation_token => {}
                _ => return Err(OperatorError::TokenMismatch),
            }
        }
        let _guard = self.ui_lock.lock().await;
        self.storage
            .expire_stale_operations()
            .map_err(OperatorError::Storage)?;
        let current = self
            .storage
            .get_operation(&request.operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        if current.state != LiveOperationState::ConfirmationOpened {
            return Err(OperatorError::NotAwaitingConfirmation);
        }
        self.storage
            .transition_to_confirming(&request.operation_id, actor_source.as_deref())
            .map_err(|e| {
                if e.to_string().contains("expired") {
                    OperatorError::Expired
                } else {
                    OperatorError::Storage(e)
                }
            })?;
        let target_op = self
            .storage
            .get_operation(&request.operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        let ui_result = match target_op.kind {
            LiveOperationKind::SubmitOrder => {
                let order = reconstruct_order(&target_op.payload_summary)
                    .map_err(|e| OperatorError::Validation(e.to_string()))?;
                self.pages
                    .confirm_open_order(order)
                    .map(|_| ())
                    .map_err(|e| OperatorError::UiUnavailable(e))
            }
            LiveOperationKind::CancelOrder => {
                let target = reconstruct_cancellation(&target_op.payload_summary)
                    .map_err(|e| OperatorError::Validation(e.to_string()))?;
                self.pages
                    .confirm_cancel_order(&target)
                    .map_err(|e| OperatorError::UiUnavailable(e))
            }
        };
        let is_ok = ui_result.is_ok();
        if is_ok {
            self.storage
                .transition_to_confirmed(
                    &request.operation_id,
                    Some(&serde_json::json!({"outcome":"confirmed"})),
                    actor_source.as_deref(),
                )
                .map_err(OperatorError::Storage)?;
        } else {
            let detail = serde_json::json!({"error": ui_result.unwrap_err().to_string(), "outcome":"unknown"});
            let _ = self.storage.transition_to_unknown(
                &request.operation_id,
                &detail,
                actor_source.as_deref(),
            );
        }
        self.confirmation_tokens.lock().await.remove(&operation_id);
        let final_op = self
            .storage
            .get_operation(&request.operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        let response = crate::operator_service::to_response(&final_op, None);
        if !is_ok {
            return Err(OperatorError::OutcomeUnknown);
        }
        Ok(response)
    }

    pub async fn confirm_operation_http(
        &self,
        request: ConfirmOperationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.confirm_operation_with_actor(request, idempotency_key, Some("http".to_string()))
            .await
    }
}
