use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    backend::Backend,
    pages::{
        CancellationTarget, NavigationResult, NavigationTarget, PageReader, StageOrderRequest,
        StageOrderResult, TradePreflightRequest, TradePreflightResult, ViewDescriptor,
    },
    storage::Storage,
};

pub use crate::operator_types::{
    AuditEventResponse, AuditHistoryResponse, LiveOperationResponse, OperationHistoryEntry,
    OperationHistoryResponse, OperatorError,
};
pub use crate::storage::{LiveOperationKind, LiveOperationState};

#[derive(Clone)]
pub struct OperatorService {
    pub(crate) backend: Arc<dyn Backend>,
    pub(crate) pages: PageReader,
    pub(crate) ui_lock: Arc<Mutex<()>>,
    pub(crate) storage: Arc<Storage>,
    pub(crate) confirmation_tokens: Arc<Mutex<HashMap<Uuid, String>>>,
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

impl OperatorService {
    pub fn new(backend: Arc<dyn Backend>, storage: Arc<Storage>) -> Self {
        Self {
            pages: PageReader::new(backend.clone()),
            backend,
            ui_lock: Arc::new(Mutex::new(())),
            storage,
            confirmation_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_in_memory(backend: Arc<dyn Backend>) -> Self {
        let storage = Storage::open_in_memory().expect("in-memory storage");
        Self::new(backend, Arc::new(storage))
    }

    pub async fn snapshot(&self, max_depth: usize, max_nodes: usize) -> Result<serde_json::Value> {
        let _guard = self.ui_lock.lock().await;
        self.backend.snapshot(max_depth, max_nodes)
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
                anyhow::bail!("structured cancellation records are not implemented")
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

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub async fn operation(
        &self,
        operation_id: &str,
    ) -> Result<LiveOperationResponse, OperatorError> {
        let op = self
            .storage
            .get_operation(operation_id)
            .map_err(OperatorError::Storage)?
            .ok_or(OperatorError::NotFound)?;
        let token = if op.state == LiveOperationState::ConfirmationOpened {
            let id = Uuid::parse_str(operation_id)
                .map_err(|_| OperatorError::Validation("invalid operation_id".to_string()))?;
            self.confirmation_tokens.lock().await.get(&id).cloned()
        } else {
            None
        };
        Ok(to_response(&op, token))
    }

    pub fn list_operations(
        &self,
        limit: usize,
        offset: usize,
        kind: Option<LiveOperationKind>,
        state: Option<LiveOperationState>,
    ) -> Result<OperationHistoryResponse, OperatorError> {
        let ops = self
            .storage
            .list_operations(limit, offset, kind, state)
            .map_err(OperatorError::Storage)?;
        let total = self
            .storage
            .count_operations(kind, state)
            .map_err(OperatorError::Storage)?;
        let entries = ops
            .iter()
            .map(|op| OperationHistoryEntry {
                operation_id: op.id.clone(),
                kind: op.kind,
                state: op.state,
                fingerprint: op.fingerprint.clone(),
                created_at: op.created_at,
                expires_at: op.expires_at,
                updated_at: op.updated_at,
                payload_summary: op.payload_summary.clone(),
                result_summary: op.result_summary.clone(),
                actor_source: op.actor_source.clone(),
            })
            .collect();
        Ok(OperationHistoryResponse {
            operations: entries,
            total,
            limit,
            offset,
        })
    }

    pub fn list_audit_events(
        &self,
        limit: usize,
        offset: usize,
        operation_id: Option<String>,
    ) -> Result<AuditHistoryResponse, OperatorError> {
        let events = self
            .storage
            .list_audit_events(limit, offset, operation_id.as_deref())
            .map_err(OperatorError::Storage)?;
        let total = self
            .storage
            .count_audit_events(operation_id.as_deref())
            .map_err(OperatorError::Storage)?;
        let responses = events
            .into_iter()
            .map(|e| AuditEventResponse {
                id: e.id,
                operation_id: e.operation_id,
                event_type: e.event_type,
                from_state: e.from_state,
                to_state: e.to_state,
                created_at: e.created_at,
                actor_source: e.actor_source,
                detail: e.detail,
                fingerprint: e.fingerprint,
            })
            .collect();
        Ok(AuditHistoryResponse {
            events: responses,
            total,
            limit,
            offset,
        })
    }
}

pub(crate) fn to_response(
    record: &crate::storage::OperationRecord,
    confirmation_token: Option<String>,
) -> LiveOperationResponse {
    let mut token = confirmation_token;
    if record.state != LiveOperationState::ConfirmationOpened {
        token = None;
    }
    LiveOperationResponse {
        operation_id: record.id.clone(),
        kind: record.kind,
        state: record.state,
        confirmation_token: token,
        fingerprint: record.fingerprint.clone(),
        expires_at: record.expires_at,
    }
}

fn json(value: impl Serialize) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(value)?)
}
