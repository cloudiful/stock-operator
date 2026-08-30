use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::ax::{AccessibilityInspector, TargetSnapshot};
use crate::pages::{
    CancellationTarget, NavigationResult, NavigationTarget, PageReader, StageOrderRequest,
    StageOrderResult, TradePreflightRequest, TradePreflightResult, ViewDescriptor,
};

const OPERATION_TTL_SECONDS: i64 = 30;

#[derive(Clone)]
pub struct OperatorService {
    inspector: AccessibilityInspector,
    pages: PageReader,
    ui_lock: Arc<Mutex<()>>,
    operations: Arc<Mutex<HashMap<Uuid, LiveOperation>>>,
    idempotency: Arc<Mutex<HashMap<String, Uuid>>>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadPanel {
    Positions,
    Orders,
    Executions,
    Funds,
    Cancellations,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadRepresentation {
    Ax,
    Ocr,
    Structured,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct ReadRequest {
    pub panel: ReadPanel,
    pub representation: Option<ReadRepresentation>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct SelectSecurityRequest {
    pub security_code: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveOperationKind {
    SubmitOrder,
    CancelOrder,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveOperationState {
    ConfirmationOpened,
    Confirming,
    Confirmed,
    Unknown,
    Expired,
    Aborted,
}

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

#[derive(Clone, Debug)]
struct LiveOperation {
    response: LiveOperationResponse,
    confirmation_token: String,
    payload: LivePayload,
    idempotency_key: Option<String>,
}

#[derive(Clone, Debug)]
enum LivePayload {
    Order(StageOrderRequest),
    Cancellation(CancellationTarget),
}

impl OperatorService {
    pub fn new(inspector: AccessibilityInspector) -> Self {
        Self {
            pages: PageReader::new(inspector.clone()),
            inspector,
            ui_lock: Arc::new(Mutex::new(())),
            operations: Arc::new(Mutex::new(HashMap::new())),
            idempotency: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<TargetSnapshot> {
        let _guard = self.ui_lock.lock().await;
        self.inspector.snapshot(max_depth, max_nodes)
    }

    pub async fn with_ui<T>(&self, operation: impl FnOnce(&PageReader) -> Result<T>) -> Result<T> {
        let _guard = self.ui_lock.lock().await;
        operation(&self.pages)
    }

    pub async fn inventory(&self, limit: usize) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.inventory(limit)?)
    }

    pub async fn trade_form(&self) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.trade_form()?)
    }

    pub async fn ocr_visible_text(&self) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.ocr_visible_text()?)
    }

    pub async fn positions_diagnostic(&self) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.diagnose_positions_table(10, 32, 3)?)
    }

    pub async fn navigation_candidates(&self) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.navigation_candidates()?)
    }

    pub async fn navigate_cancellations(&self) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.navigate_to_cancellations()?)
    }

    pub async fn view(&self) -> Result<ViewDescriptor> {
        let _guard = self.ui_lock.lock().await;
        self.pages.view()
    }

    pub async fn read(&self, request: ReadRequest) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        let representation = request.representation.unwrap_or(match request.panel {
            ReadPanel::Cancellations => ReadRepresentation::Ocr,
            _ => ReadRepresentation::Structured,
        });
        match (request.panel, representation) {
            (ReadPanel::Positions, ReadRepresentation::Ax) => json(self.pages.positions()?),
            (ReadPanel::Positions, ReadRepresentation::Ocr) => json(self.pages.positions_ocr()?),
            (ReadPanel::Positions, ReadRepresentation::Structured) => {
                json(self.pages.positions_structured()?)
            }
            (ReadPanel::Orders, ReadRepresentation::Ax) => json(self.pages.orders()?),
            (ReadPanel::Orders, ReadRepresentation::Ocr) => json(self.pages.orders_ocr()?),
            (ReadPanel::Orders, ReadRepresentation::Structured) => {
                json(self.pages.orders_structured()?)
            }
            (ReadPanel::Executions, ReadRepresentation::Ax) => json(self.pages.executions()?),
            (ReadPanel::Executions, ReadRepresentation::Ocr) => json(self.pages.executions_ocr()?),
            (ReadPanel::Executions, ReadRepresentation::Structured) => {
                json(self.pages.executions_structured()?)
            }
            (ReadPanel::Funds, ReadRepresentation::Ax) => json(self.pages.funds()?),
            (ReadPanel::Funds, ReadRepresentation::Ocr) => json(self.pages.funds_ocr()?),
            (ReadPanel::Funds, ReadRepresentation::Structured) => {
                json(self.pages.funds_structured()?)
            }
            (ReadPanel::Cancellations, ReadRepresentation::Ax) => json(self.pages.cancellations()?),
            (ReadPanel::Cancellations, ReadRepresentation::Ocr) => {
                json(self.pages.cancellations_ocr()?)
            }
            (ReadPanel::Cancellations, ReadRepresentation::Structured) => {
                bail!("structured cancellation records are not implemented")
            }
        }
    }

    pub async fn navigate(&self, target: NavigationTarget) -> Result<NavigationResult> {
        let _guard = self.ui_lock.lock().await;
        self.pages.navigate_readonly(target)
    }

    pub async fn select_security(
        &self,
        request: SelectSecurityRequest,
    ) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.select_trade_security(&request.security_code)?)
    }

    pub async fn stage_order(&self, request: StageOrderRequest) -> Result<StageOrderResult> {
        let _guard = self.ui_lock.lock().await;
        self.pages.stage_order(request)
    }

    pub async fn trade_preflight(
        &self,
        request: TradePreflightRequest,
    ) -> Result<TradePreflightResult> {
        let _guard = self.ui_lock.lock().await;
        self.pages.trade_preflight(request)
    }

    pub async fn close_submitted_notice(&self, contract_id: &str) -> Result<()> {
        let _guard = self.ui_lock.lock().await;
        self.pages.close_submitted_notice(contract_id)
    }

    pub async fn close_cancel_submitted_notice(&self) -> Result<()> {
        let _guard = self.ui_lock.lock().await;
        self.pages.close_cancel_submitted_notice()
    }

    pub async fn close_cancel_selection_warning(&self) -> Result<()> {
        let _guard = self.ui_lock.lock().await;
        self.pages.close_cancel_selection_warning()
    }

    pub async fn open_order_confirmation_direct(
        &self,
        request: StageOrderRequest,
    ) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.open_order_confirmation(request)?)
    }

    pub async fn confirm_order_direct(
        &self,
        request: StageOrderRequest,
    ) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        json(self.pages.confirm_open_order(request)?)
    }

    pub async fn open_cancellation_direct(&self, target: &CancellationTarget) -> Result<()> {
        let _guard = self.ui_lock.lock().await;
        self.pages.open_cancel_confirmation(target)
    }

    pub async fn confirm_cancellation_direct(&self, target: &CancellationTarget) -> Result<()> {
        let _guard = self.ui_lock.lock().await;
        self.pages.confirm_cancel_order(target)
    }

    pub async fn prepare_order(
        &self,
        request: PrepareOrderRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse> {
        let _guard = self.ui_lock.lock().await;
        self.require_new_idempotency_key(&idempotency_key).await?;
        self.require_no_active_live_operation().await?;
        self.pages.open_order_confirmation(request.order.clone())?;
        self.create_operation(
            LiveOperationKind::SubmitOrder,
            LivePayload::Order(request.order),
            Some(idempotency_key),
        )
        .await
    }

    pub async fn prepare_cancellation(
        &self,
        request: PrepareCancellationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse> {
        let _guard = self.ui_lock.lock().await;
        self.require_new_idempotency_key(&idempotency_key).await?;
        self.require_no_active_live_operation().await?;
        self.pages.open_cancel_confirmation(&request.target)?;
        self.create_operation(
            LiveOperationKind::CancelOrder,
            LivePayload::Cancellation(request.target),
            Some(idempotency_key),
        )
        .await
    }

    pub async fn confirm_operation(
        &self,
        request: ConfirmOperationRequest,
        idempotency_key: String,
    ) -> Result<LiveOperationResponse> {
        let operation = self.validated_operation(&request, &idempotency_key).await?;
        let _guard = self.ui_lock.lock().await;
        let confirmation = match &operation.payload {
            LivePayload::Order(order) => self.pages.confirm_open_order(order.clone()).map(|_| ()),
            LivePayload::Cancellation(target) => self.pages.confirm_cancel_order(target),
        };
        let state = if confirmation.is_ok() {
            LiveOperationState::Confirmed
        } else {
            LiveOperationState::Unknown
        };
        let mut operations = self.operations.lock().await;
        let operation_id =
            Uuid::parse_str(&request.operation_id).context("invalid operation_id")?;
        let stored = operations
            .get_mut(&operation_id)
            .context("operation disappeared during confirmation")?;
        stored.response.state = state;
        stored.confirmation_token.clear();
        if let Err(error) = confirmation {
            return Err(
                error.context("confirmation outcome is unknown; do not retry automatically")
            );
        }
        Ok(public_response(stored))
    }

    pub async fn operation(&self, operation_id: &str) -> Result<LiveOperationResponse> {
        let operation_id = Uuid::parse_str(operation_id).context("invalid operation_id")?;
        let mut operations = self.operations.lock().await;
        let operation = operations
            .get_mut(&operation_id)
            .context("operation not found")?;
        expire(operation);
        Ok(public_response(operation))
    }

    pub async fn abort_operation(&self, operation_id: &str) -> Result<LiveOperationResponse> {
        let operation_id = Uuid::parse_str(operation_id).context("invalid operation_id")?;
        let operation = {
            let operations = self.operations.lock().await;
            operations
                .get(&operation_id)
                .context("operation not found")?
                .clone()
        };
        if !matches!(
            operation.response.state,
            LiveOperationState::ConfirmationOpened | LiveOperationState::Expired
        ) {
            bail!("operation cannot be aborted in its current state");
        }
        let _guard = self.ui_lock.lock().await;
        let result = match &operation.payload {
            LivePayload::Order(order) => self.pages.cancel_order_confirmation(order),
            LivePayload::Cancellation(target) => {
                self.pages.cancel_cancellation_confirmation(target)
            }
        };
        let mut operations = self.operations.lock().await;
        let stored = operations
            .get_mut(&operation_id)
            .context("operation not found")?;
        stored.response.state = if result.is_ok() {
            LiveOperationState::Aborted
        } else {
            LiveOperationState::Unknown
        };
        stored.confirmation_token.clear();
        result.context("abort outcome is unknown; inspect the broker dialog")?;
        Ok(public_response(stored))
    }

    async fn create_operation(
        &self,
        kind: LiveOperationKind,
        payload: LivePayload,
        idempotency_key: Option<String>,
    ) -> Result<LiveOperationResponse> {
        let operation_id = Uuid::new_v4();
        let confirmation_token = Uuid::new_v4().simple().to_string();
        let fingerprint = fingerprint(kind, &payload)?;
        let expires_at = Utc::now() + Duration::seconds(OPERATION_TTL_SECONDS);
        let response = LiveOperationResponse {
            operation_id: operation_id.to_string(),
            kind,
            state: LiveOperationState::ConfirmationOpened,
            confirmation_token: Some(confirmation_token.clone()),
            fingerprint,
            expires_at,
        };
        self.operations.lock().await.insert(
            operation_id,
            LiveOperation {
                response: response.clone(),
                confirmation_token,
                payload,
                idempotency_key: idempotency_key.clone(),
            },
        );
        if let Some(key) = idempotency_key {
            self.idempotency.lock().await.insert(key, operation_id);
        }
        Ok(response)
    }

    async fn validated_operation(
        &self,
        request: &ConfirmOperationRequest,
        idempotency_key: &str,
    ) -> Result<LiveOperation> {
        let operation_id =
            Uuid::parse_str(&request.operation_id).context("invalid operation_id")?;
        if self.idempotency.lock().await.get(idempotency_key) != Some(&operation_id) {
            bail!("idempotency key is not bound to the requested operation");
        }
        let mut operations = self.operations.lock().await;
        let operation = operations
            .get_mut(&operation_id)
            .context("operation not found")?;
        expire(operation);
        if operation.response.state != LiveOperationState::ConfirmationOpened {
            bail!("operation is not awaiting confirmation");
        }
        if operation.confirmation_token != request.confirmation_token {
            bail!("confirmation token does not match");
        }
        if operation.response.fingerprint != request.fingerprint {
            bail!("operation fingerprint does not match");
        }
        if operation.idempotency_key.as_deref() != Some(idempotency_key) {
            bail!("idempotency key does not match the prepared operation");
        }
        operation.response.state = LiveOperationState::Confirming;
        Ok(operation.clone())
    }

    async fn require_new_idempotency_key(&self, key: &str) -> Result<()> {
        if key.trim().is_empty() {
            bail!("Idempotency-Key is required");
        }
        if self.idempotency.lock().await.contains_key(key) {
            bail!("Idempotency-Key has already been used");
        }
        Ok(())
    }

    async fn require_no_active_live_operation(&self) -> Result<()> {
        let mut operations = self.operations.lock().await;
        for operation in operations.values_mut() {
            expire(operation);
            if matches!(
                operation.response.state,
                LiveOperationState::ConfirmationOpened
                    | LiveOperationState::Confirming
                    | LiveOperationState::Expired
                    | LiveOperationState::Unknown
            ) {
                bail!("another live operation still owns the broker dialog");
            }
        }
        Ok(())
    }
}

fn json(value: impl Serialize) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(value)?)
}

fn fingerprint(kind: LiveOperationKind, payload: &LivePayload) -> Result<String> {
    let payload = match payload {
        LivePayload::Order(order) => serde_json::to_vec(&(kind, order))?,
        LivePayload::Cancellation(target) => serde_json::to_vec(&(kind, target))?,
    };
    Ok(Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn expire(operation: &mut LiveOperation) {
    if matches!(
        operation.response.state,
        LiveOperationState::ConfirmationOpened | LiveOperationState::Confirming
    ) && Utc::now() >= operation.response.expires_at
    {
        operation.response.state = LiveOperationState::Expired;
        operation.confirmation_token.clear();
    }
}

fn public_response(operation: &LiveOperation) -> LiveOperationResponse {
    let mut response = operation.response.clone();
    if response.state != LiveOperationState::ConfirmationOpened {
        response.confirmation_token = None;
    }
    response
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::{
        LiveOperation, LiveOperationKind, LiveOperationResponse, LiveOperationState, LivePayload,
        expire, fingerprint, public_response,
    };
    use crate::pages::{CancellationTarget, OrderSide, StageOrderRequest};

    fn order() -> StageOrderRequest {
        StageOrderRequest {
            security_code: "600028".to_string(),
            side: OrderSide::Buy,
            price: "4.55".to_string(),
            quantity: 100,
        }
    }

    #[test]
    fn fingerprint_is_stable_and_payload_bound() {
        let first =
            fingerprint(LiveOperationKind::SubmitOrder, &LivePayload::Order(order())).unwrap();
        let second =
            fingerprint(LiveOperationKind::SubmitOrder, &LivePayload::Order(order())).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(
            first,
            fingerprint(
                LiveOperationKind::CancelOrder,
                &LivePayload::Cancellation(CancellationTarget {
                    contract_id: "3506784".to_string(),
                    security_code: "600028".to_string(),
                    security_name: "中国石化".to_string(),
                    side: "买入".to_string(),
                    price: "4.55".to_string(),
                    quantity: 100,
                }),
            )
            .unwrap()
        );
    }

    #[test]
    fn expired_operation_hides_confirmation_token() {
        let mut operation = LiveOperation {
            response: LiveOperationResponse {
                operation_id: uuid::Uuid::new_v4().to_string(),
                kind: LiveOperationKind::SubmitOrder,
                state: LiveOperationState::ConfirmationOpened,
                confirmation_token: Some("secret".to_string()),
                fingerprint: "fingerprint".to_string(),
                expires_at: Utc::now() - Duration::seconds(1),
            },
            confirmation_token: "secret".to_string(),
            payload: LivePayload::Order(order()),
            idempotency_key: Some("key".to_string()),
        };
        expire(&mut operation);
        assert_eq!(operation.response.state, LiveOperationState::Expired);
        assert!(public_response(&operation).confirmation_token.is_none());
    }
}
