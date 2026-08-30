use uuid::Uuid;

use crate::{
    operator_service::OperatorService,
    operator_types::{LiveOperationResponse, OperatorError},
    storage::{
        LiveOperationKind, LiveOperationState,
        redaction::{reconstruct_cancellation, reconstruct_order},
    },
};

impl OperatorService {
    pub async fn abort_operation(
        &self,
        operation_id: &str,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.abort_operation_with_actor(operation_id, Some("mcp".to_string()))
            .await
    }

    pub async fn abort_operation_with_actor(
        &self,
        operation_id: &str,
        actor_source: Option<String>,
    ) -> Result<LiveOperationResponse, OperatorError> {
        let _ = self
            .storage
            .get_operation(operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        let _guard = self.ui_lock.lock().await;
        let current = self
            .storage
            .get_operation(operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        if !matches!(
            current.state,
            LiveOperationState::ConfirmationOpened
                | LiveOperationState::Expired
                | LiveOperationState::Unknown
        ) {
            return Err(OperatorError::AbortConflict);
        }
        let ui_result = match current.kind {
            LiveOperationKind::SubmitOrder => {
                let order = reconstruct_order(&current.payload_summary)
                    .map_err(|e| OperatorError::Validation(e.to_string()))?;
                self.pages
                    .cancel_order_confirmation(&order)
                    .map_err(|e| OperatorError::UiUnavailable(e))
            }
            LiveOperationKind::CancelOrder => {
                let target = reconstruct_cancellation(&current.payload_summary)
                    .map_err(|e| OperatorError::Validation(e.to_string()))?;
                self.pages
                    .cancel_cancellation_confirmation(&target)
                    .map_err(|e| OperatorError::UiUnavailable(e))
            }
        };
        let is_ok = ui_result.is_ok();
        if is_ok {
            self.storage
                .transition_to_aborted(operation_id, actor_source.as_deref())
                .map_err(OperatorError::Storage)?;
        } else {
            let detail = serde_json::json!({"error": ui_result.unwrap_err().to_string(), "outcome":"unknown"});
            let _ =
                self.storage
                    .transition_to_unknown(operation_id, &detail, actor_source.as_deref());
            if let Ok(uuid) = Uuid::parse_str(operation_id) {
                self.confirmation_tokens.lock().await.remove(&uuid);
            }
            return Err(OperatorError::OutcomeUnknown);
        }
        if let Ok(uuid) = Uuid::parse_str(operation_id) {
            self.confirmation_tokens.lock().await.remove(&uuid);
        }
        let final_op = self
            .storage
            .get_operation(operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        Ok(crate::operator_service::to_response(&final_op, None))
    }

    pub async fn abort_operation_http(
        &self,
        operation_id: &str,
    ) -> Result<LiveOperationResponse, OperatorError> {
        self.abort_operation_with_actor(operation_id, Some("http".to_string()))
            .await
    }
}
