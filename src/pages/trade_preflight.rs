use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::stage_order::StageOrderRequest;
use super::types::{DataQuality, PanelKind, WorkspaceKind};

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct TradePreflightRequest {
    pub order: Option<StageOrderRequest>,
    pub reference_quote: Option<ReferenceQuote>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct ReferenceQuote {
    pub latest_price: String,
    pub source: String,
    pub captured_at: Option<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct TradePreflightResult {
    pub captured_at: String,
    pub workspace: WorkspaceKind,
    pub panel: PanelKind,
    pub security: Option<SecurityIdentity>,
    pub form: BrokerFormState,
    pub broker_quote: BrokerQuote,
    pub constraints: BrokerTradeConstraints,
    pub reference_comparison: Option<QuoteComparison>,
    pub decision: PreflightDecision,
    pub quality: DataQuality,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct SecurityIdentity {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct BrokerFormState {
    pub price: Option<String>,
    pub quantity: Option<u64>,
    pub control_count: usize,
    pub security_code_editable: bool,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct BrokerQuote {
    pub latest_price: Option<String>,
    pub pre_close: Option<String>,
    pub up_limit: Option<String>,
    pub down_limit: Option<String>,
    pub buy_ceiling: Option<String>,
    pub sell_floor: Option<String>,
    pub bid_price_1: Option<String>,
    pub ask_price_1: Option<String>,
    pub trade_status: Option<String>,
    pub captured_at: String,
    pub source: String,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct BrokerTradeConstraints {
    pub available_funds: Option<String>,
    pub max_buy_quantity: Option<u64>,
    pub max_sell_quantity: Option<u64>,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct QuoteComparison {
    pub reference_source: String,
    pub reference_price: String,
    pub broker_price: Option<String>,
    pub absolute_delta: Option<String>,
    pub percentage_delta: Option<String>,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreflightDecision {
    ObservedOnly,
    Ready,
    NeedsReview,
    Rejected,
}
