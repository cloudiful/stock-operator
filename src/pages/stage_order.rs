use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::types::WorkspaceKind;

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct StageOrderRequest {
    pub security_code: String,
    pub side: OrderSide,
    pub price: String,
    pub quantity: u64,
}

#[derive(Clone, Debug, JsonSchema, Serialize, ToSchema)]
pub struct StageOrderResult {
    pub status: String,
    pub request: StageOrderRequest,
    pub workspace: WorkspaceKind,
    pub order_value: String,
    pub verified_price: String,
    pub verified_quantity: String,
    pub warnings: Vec<String>,
}
