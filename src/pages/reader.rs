use std::sync::Arc;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::Serialize;

use crate::backend::Backend;

use super::{
    cancel_order::{CancelNavigationResult, CancellationTarget},
    navigation::{NavigationCandidates, NavigationResult, NavigationTarget},
    ocr::OcrSnapshot,
    stage_order::{StageOrderRequest, StageOrderResult},
    trade_preflight::{TradePreflightRequest, TradePreflightResult},
    types::{
        ExecutionsSnapshot, FundsSnapshot, OrdersSnapshot, PageSnapshot, PositionsSnapshot,
        PositionsTableDiagnostic, TradeFormSnapshot, UiInventory, ViewDescriptor,
    },
};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SubmitOrderResult {
    pub status: String,
    pub request: StageOrderRequest,
    pub after: ViewDescriptor,
    pub visible_prompts: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct SecuritySelectionResult {
    pub security_code: String,
    pub selected_text: String,
    pub derived_price: String,
}

#[derive(Clone)]
pub struct PageReader {
    #[allow(dead_code)]
    backend: Arc<dyn Backend>,
}

impl PageReader {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }

    pub fn inventory(&self, _max_controls: usize) -> Result<UiInventory> {
        stub()
    }

    pub fn view(&self) -> Result<ViewDescriptor> {
        stub()
    }

    pub fn trade_form(&self) -> Result<PageSnapshot<TradeFormSnapshot>> {
        stub()
    }

    pub fn positions(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        stub()
    }

    pub fn diagnose_positions_table(
        &self,
        _max_rows: usize,
        _max_cells: usize,
        _max_depth: usize,
    ) -> Result<PositionsTableDiagnostic> {
        stub()
    }

    pub fn navigation_candidates(&self) -> Result<NavigationCandidates> {
        stub()
    }

    pub fn navigate_readonly(&self, _target: NavigationTarget) -> Result<NavigationResult> {
        stub()
    }

    pub fn navigate_to_cancellations(&self) -> Result<CancelNavigationResult> {
        stub()
    }

    pub fn open_cancel_confirmation(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn close_cancel_selection_warning(&self) -> Result<()> {
        stub()
    }

    pub fn confirm_cancel_order(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn close_cancel_submitted_notice(&self) -> Result<()> {
        stub()
    }

    pub fn cancel_cancellation_confirmation(&self, _target: &CancellationTarget) -> Result<()> {
        stub()
    }

    pub fn ocr_visible_text(&self) -> Result<OcrSnapshot> {
        stub()
    }

    pub fn cancellations(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn orders(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn executions(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        stub()
    }

    pub fn funds(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        stub()
    }

    pub fn cancellations_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn orders_ocr(&self) -> Result<PageSnapshot<OrdersSnapshot>> {
        stub()
    }

    pub fn executions_ocr(&self) -> Result<PageSnapshot<ExecutionsSnapshot>> {
        stub()
    }

    pub fn funds_ocr(&self) -> Result<PageSnapshot<FundsSnapshot>> {
        stub()
    }

    pub fn positions_ocr(&self) -> Result<PageSnapshot<PositionsSnapshot>> {
        stub()
    }

    pub fn select_trade_security(&self, _security_code: &str) -> Result<SecuritySelectionResult> {
        stub()
    }

    pub fn stage_order(&self, _request: StageOrderRequest) -> Result<StageOrderResult> {
        stub()
    }

    pub fn trade_preflight(
        &self,
        _request: TradePreflightRequest,
    ) -> Result<TradePreflightResult> {
        stub()
    }

    pub fn open_order_confirmation(&self, _request: StageOrderRequest) -> Result<SubmitOrderResult> {
        stub()
    }

    pub fn confirm_open_order(&self, _request: StageOrderRequest) -> Result<SubmitOrderResult> {
        stub()
    }

    pub fn close_submitted_notice(&self, _contract_id: &str) -> Result<()> {
        stub()
    }

    pub fn cancel_order_confirmation(&self, _request: &StageOrderRequest) -> Result<()> {
        stub()
    }
}

fn stub<T>() -> Result<T> {
    Err(anyhow!("win backend not implemented (task 1 stub)"))
}
