use schemars::JsonSchema;
use serde::Serialize;

use super::super::types::{DataQuality, DataSource, PageSnapshot};

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct StructuredField {
    pub raw: Option<String>,
    pub normalized: Option<String>,
    pub quality: DataQuality,
    pub source: DataSource,
}

impl StructuredField {
    pub(crate) fn unavailable() -> Self {
        Self {
            raw: None,
            normalized: None,
            quality: DataQuality::Unavailable,
            source: DataSource::Unavailable,
        }
    }

    pub(crate) fn inferred(value: String) -> Self {
        Self {
            raw: Some(value.clone()),
            normalized: Some(value),
            quality: DataQuality::Partial,
            source: DataSource::Inferred,
        }
    }
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct BusinessTable<T: JsonSchema> {
    pub records: Vec<T>,
    pub row_count: usize,
    pub mapped_columns: Vec<String>,
    pub warnings: Vec<String>,
}

pub type PositionStructuredSnapshot = PageSnapshot<BusinessTable<PositionRecord>>;
pub type OrderStructuredSnapshot = PageSnapshot<BusinessTable<OrderRecord>>;
pub type ExecutionStructuredSnapshot = PageSnapshot<BusinessTable<ExecutionRecord>>;
pub type FundsStructuredSnapshot = PageSnapshot<BusinessTable<FundsRecord>>;

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct PositionRecord {
    pub security_code: StructuredField,
    pub security_name: StructuredField,
    pub yesterday_balance: StructuredField,
    pub reference_holding: StructuredField,
    pub available_quantity: StructuredField,
    pub frozen_quantity: StructuredField,
    pub cost_price: StructuredField,
    pub current_price: StructuredField,
    pub current_cost: StructuredField,
    pub market_value: StructuredField,
    pub floating_pnl: StructuredField,
    pub pnl_ratio: StructuredField,
    pub today_buy_quantity: StructuredField,
    pub reference_pnl: StructuredField,
    pub share_balance: StructuredField,
    pub shareholder_account: StructuredField,
    pub custodian_unit: StructuredField,
    pub fund_account: StructuredField,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct OrderRecord {
    pub order_id: StructuredField,
    pub submitted_at: StructuredField,
    pub security_code: StructuredField,
    pub security_name: StructuredField,
    pub side: StructuredField,
    pub order_type: StructuredField,
    pub price: StructuredField,
    pub quantity: StructuredField,
    pub filled_quantity: StructuredField,
    pub remaining_quantity: StructuredField,
    pub status: StructuredField,
    pub cancelable: StructuredField,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct ExecutionRecord {
    pub execution_id: StructuredField,
    pub executed_at: StructuredField,
    pub order_id: StructuredField,
    pub security_code: StructuredField,
    pub security_name: StructuredField,
    pub side: StructuredField,
    pub price: StructuredField,
    pub quantity: StructuredField,
    pub amount: StructuredField,
}

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct FundsRecord {
    pub total_assets: StructuredField,
    pub available_funds: StructuredField,
    pub frozen_funds: StructuredField,
    pub cash_balance: StructuredField,
    pub market_value: StructuredField,
    pub withdrawable_funds: StructuredField,
}
