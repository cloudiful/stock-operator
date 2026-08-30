use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{
    reader::PageReader,
    stage_order::StageOrderRequest,
    trade_preflight_validation::{
        compare_quote, evaluate_order, integer_near_label, quality, selected_security,
        text_near_label, value_near_label,
    },
    types::{DataQuality, PanelKind, WorkspaceKind},
};

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

impl PageReader {
    pub fn trade_preflight(&self, request: TradePreflightRequest) -> Result<TradePreflightResult> {
        let window = self.focus_target_window()?;
        let view = self.view()?;
        let form_snapshot = self.trade_form()?;
        let ocr = self.ocr_visible_text()?;
        let captured_at = now_string();
        let security = selected_security(&ocr);
        let broker_quote = BrokerQuote {
            latest_price: value_near_label(&ocr, &["现价"]),
            pre_close: value_near_label(&ocr, &["昨收"]),
            up_limit: value_near_label(&ocr, &["涨停"]),
            down_limit: value_near_label(&ocr, &["跌停"]),
            buy_ceiling: value_near_label(&ocr, &["连续竞价买入上限"]),
            sell_floor: value_near_label(&ocr, &["连续竞价卖出下限"]),
            bid_price_1: value_near_label(&ocr, &["买一"]),
            ask_price_1: value_near_label(&ocr, &["卖一"]),
            trade_status: text_near_label(&ocr, &["交易状态"]),
            captured_at: captured_at.clone(),
            source: "broker_ui_ocr".to_string(),
        };
        let constraints = BrokerTradeConstraints {
            available_funds: value_near_label(&ocr, &["可用资金"]),
            max_buy_quantity: integer_near_label(&ocr, &["最大可买"]),
            max_sell_quantity: integer_near_label(&ocr, &["最大可卖"]),
        };
        let form = BrokerFormState {
            price: value_near_label(&ocr, &["价格"]),
            quantity: integer_near_label(&ocr, &["数量"]),
            control_count: form_snapshot.data.controls.len(),
            security_code_editable: super::security_selection::security_code_field(&window).is_ok(),
        };
        let reference_comparison = request
            .reference_quote
            .as_ref()
            .map(|reference| compare_quote(reference, broker_quote.latest_price.as_deref()));
        let mut warnings = ocr.warnings.clone();
        let quality = quality(&security, &broker_quote, &constraints, &mut warnings);
        let decision = evaluate_order(
            request.order.as_ref(),
            view.workspace,
            security.as_ref(),
            &form,
            &broker_quote,
            &constraints,
            reference_comparison.as_ref(),
            &mut warnings,
        )?;
        Ok(TradePreflightResult {
            captured_at,
            workspace: view.workspace,
            panel: view.panel,
            security,
            form,
            broker_quote,
            constraints,
            reference_comparison,
            decision,
            quality,
            warnings,
        })
    }
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339()
}
