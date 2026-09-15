use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::types::PanelKind;

#[derive(Clone, Debug, JsonSchema, Serialize)]
pub struct CancelNavigationResult {
    pub status: String,
    pub panel: PanelKind,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, ToSchema)]
pub struct CancellationTarget {
    pub contract_id: String,
    pub security_code: String,
    pub security_name: String,
    pub side: String,
    pub price: String,
    pub quantity: u64,
}
